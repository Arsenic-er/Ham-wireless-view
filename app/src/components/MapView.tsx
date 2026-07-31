import { useEffect, useRef, useState } from "react";
import type { FeatureCollection } from "geojson";
import maplibregl, {
  type GeoJSONSource,
  type ImageSource,
  type Map as MapLibreMap,
} from "maplibre-gl";

import {
  coverageCircleCoordinates,
  graticuleGeoJson,
  maidenheadLocator,
} from "../lib/geodesy";
import {
  SATELLITE_SOURCE_ID,
  acquirePmtilesProtocol,
  applyProtomapsTheme,
  firstBasemapLabelLayerId,
  isTrustedBasemap,
  isTrustedProtomapsBasemap,
  isTrustedSatelliteBasemap,
  isTrustedTiandituBasemap,
  type BasemapPresentation,
  synchronizeBasemap,
} from "../lib/basemap";
import { MapOverlayBlobUrlLease, buildMapOverlayImageSpec } from "../lib/mapOverlay";
import type {
  BasemapInfo,
  CalculationPreview,
  CalculationResult,
  MapPoint,
  ResolvedTheme,
} from "../lib/types";

interface MapViewProps {
  theme: ResolvedTheme;
  point: MapPoint | null;
  heatmap: CalculationResult | null;
  preview: CalculationPreview | null;
  heatmapStale: boolean;
  onPointSelect: (point: MapPoint) => void;
  basemap?: BasemapInfo | null;
}

const EMPTY_FEATURE_COLLECTION: FeatureCollection = {
  type: "FeatureCollection",
  features: [],
};

function selectedPointData(point: MapPoint | null): FeatureCollection {
  if (!point) return EMPTY_FEATURE_COLLECTION;
  return {
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        properties: {},
        geometry: { type: "Point", coordinates: [point.lon, point.lat] },
      },
    ],
  };
}

function coverageCircleData(point: MapPoint | null): FeatureCollection {
  if (!point) return EMPTY_FEATURE_COLLECTION;
  return {
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        properties: {},
        geometry: {
          type: "Polygon",
          coordinates: [coverageCircleCoordinates(point)],
        },
      },
    ],
  };
}

function updateSelection(map: MapLibreMap, point: MapPoint | null): void {
  (map.getSource("selected-point") as GeoJSONSource | undefined)?.setData(
    selectedPointData(point),
  );
  (map.getSource("coverage-circle") as GeoJSONSource | undefined)?.setData(
    coverageCircleData(point),
  );
}

function updateHeatmap(
  map: MapLibreMap,
  heatmap: CalculationResult | CalculationPreview | null,
  stale: boolean,
  blobUrls: MapOverlayBlobUrlLease,
): void {
  const source = map.getSource("coverage-heatmap") as ImageSource | undefined;
  if (!heatmap) {
    if (map.getLayer("coverage-heatmap-layer")) map.removeLayer("coverage-heatmap-layer");
    if (source) map.removeSource("coverage-heatmap");
    blobUrls.clear();
    return;
  }
  const dataImage = buildMapOverlayImageSpec(heatmap);
  const image = { ...dataImage, url: blobUrls.acquire(dataImage.url) };
  if (source) {
    source.updateImage(image);
    map.setPaintProperty("coverage-heatmap-layer", "raster-opacity", stale ? 0.28 : 0.84);
    return;
  }
  map.addSource("coverage-heatmap", { type: "image", ...image });
  map.addLayer(
    {
      id: "coverage-heatmap-layer",
      type: "raster",
      source: "coverage-heatmap",
      paint: { "raster-opacity": stale ? 0.28 : 0.84, "raster-resampling": "linear" },
    },
    firstBasemapLabelLayerId(map) ?? "coverage-circle-fill",
  );
}

export function MapView({
  theme,
  point,
  heatmap,
  preview,
  heatmapStale,
  onPointSelect,
  basemap,
}: MapViewProps) {
  const [basemapPresentation, setBasemapPresentation] =
    useState<BasemapPresentation>("map");
  const [satelliteFallback, setSatelliteFallback] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const synchronizeMapStateRef = useRef<(() => void) | null>(null);
  const protomapsViewFittedRef = useRef(false);
  const heatmapBlobUrlsRef = useRef<MapOverlayBlobUrlLease | null>(null);
  if (!heatmapBlobUrlsRef.current) {
    heatmapBlobUrlsRef.current = new MapOverlayBlobUrlLease();
  }
  const themeRef = useRef(theme);
  const pointRef = useRef(point);
  const heatmapRef = useRef(heatmap);
  const previewRef = useRef(preview);
  const heatmapStaleRef = useRef(heatmapStale);
  const onPointSelectRef = useRef(onPointSelect);
  const basemapRef = useRef(basemap);
  const basemapPresentationRef = useRef(basemapPresentation);
  themeRef.current = theme;
  pointRef.current = point;
  heatmapRef.current = heatmap;
  previewRef.current = preview;
  heatmapStaleRef.current = heatmapStale;
  onPointSelectRef.current = onPointSelect;
  basemapRef.current = basemap;
  basemapPresentationRef.current = basemapPresentation;

  useEffect(() => {
    if (!containerRef.current || !heatmapBlobUrlsRef.current) return;
    const heatmapBlobUrls = heatmapBlobUrlsRef.current;
    const releasePmtilesProtocol = acquirePmtilesProtocol();
    const dark = theme === "dark";
    const map = new maplibregl.Map({
      container: containerRef.current,
      center: [104, 35],
      zoom: 3.25,
      minZoom: 2.3,
      maxZoom: 12,
      localIdeographFontFamily:
        "Microsoft YaHei, Noto Sans CJK SC, PingFang SC, sans-serif",
      maxBounds: [
        [65, 5],
        [145, 65],
      ],
      attributionControl: false,
      style: {
        version: 8,
        sources: {},
        layers: [
          {
            id: "neutral-background",
            type: "background",
            paint: { "background-color": dark ? "#101820" : "#eaf0f2" },
          },
        ],
      },
    });
    mapRef.current = map;
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-left");
    map.addControl(
      new maplibregl.ScaleControl({ maxWidth: 120, unit: "metric" }),
      "bottom-right",
    );
    let overlayScaffoldReady = false;
    let synchronizationPending = true;
    const synchronizeDesiredMapState = () => {
      if (!overlayScaffoldReady || !map.isStyleLoaded()) {
        synchronizationPending = true;
        return;
      }
      synchronizationPending = false;
      synchronizeBasemap(
        map,
        basemapRef.current,
        themeRef.current,
        basemapPresentationRef.current,
      );
      if (
        !protomapsViewFittedRef.current &&
        isTrustedProtomapsBasemap(basemapRef.current)
      ) {
        protomapsViewFittedRef.current = true;
        if (!pointRef.current) {
          const [west, south, east, north] = basemapRef.current.bounds;
          map.fitBounds(
            [
              [west, south],
              [east, north],
            ],
            { padding: 48, duration: 0, maxZoom: 4.5 },
          );
        }
      }
      updateSelection(map, pointRef.current);
      updateHeatmap(
        map,
        heatmapRef.current ?? previewRef.current,
        heatmapRef.current ? heatmapStaleRef.current : false,
        heatmapBlobUrls,
      );
    };
    const replayPendingMapState = () => {
      if (synchronizationPending) synchronizeDesiredMapState();
    };
    synchronizeMapStateRef.current = synchronizeDesiredMapState;
    map.on("styledata", replayPendingMapState);
    map.on("idle", replayPendingMapState);
    map.on("load", () => {
      map.addSource("graticule", { type: "geojson", data: graticuleGeoJson() });
      map.addLayer({
        id: "graticule-lines",
        type: "line",
        source: "graticule",
        paint: {
          "line-color": dark ? "#35505f" : "#bfd0d7",
          "line-width": 1,
          "line-opacity": 0.68,
        },
      });
      map.addSource("coverage-circle", {
        type: "geojson",
        data: coverageCircleData(pointRef.current),
      });
      map.addLayer({
        id: "coverage-circle-fill",
        type: "fill",
        source: "coverage-circle",
        paint: {
          "fill-color": dark ? "#5fd0c4" : "#087f74",
          "fill-opacity": 0.055,
        },
      });
      map.addLayer({
        id: "coverage-circle-line",
        type: "line",
        source: "coverage-circle",
        paint: {
          "line-color": dark ? "#7be1d7" : "#087f74",
          "line-width": 2,
          "line-dasharray": [3, 2],
        },
      });
      map.addSource("selected-point", {
        type: "geojson",
        data: selectedPointData(pointRef.current),
      });
      map.addLayer({
        id: "selected-point-halo",
        type: "circle",
        source: "selected-point",
        paint: {
          "circle-radius": 12,
          "circle-color": "#ff5c35",
          "circle-opacity": 0.18,
        },
      });
      map.addLayer({
        id: "selected-point-core",
        type: "circle",
        source: "selected-point",
        paint: {
          "circle-radius": 5,
          "circle-color": "#ff5c35",
          "circle-stroke-color": "#ffffff",
          "circle-stroke-width": 2,
        },
      });
      overlayScaffoldReady = true;
      synchronizationPending = true;
      synchronizeDesiredMapState();
    });
    map.on("click", (event) => {
      onPointSelectRef.current({ lat: event.lngLat.lat, lon: event.lngLat.lng });
    });
    map.on("error", (event) => {
      if ((event as { sourceId?: string }).sourceId === SATELLITE_SOURCE_ID) {
        setSatelliteFallback(true);
        setBasemapPresentation("map");
      }
    });
    return () => {
      if (synchronizeMapStateRef.current === synchronizeDesiredMapState) {
        synchronizeMapStateRef.current = null;
      }
      mapRef.current = null;
      heatmapBlobUrls.clear();
      map.remove();
      releasePmtilesProtocol();
    };
  }, []);

  useEffect(() => {
    const map = mapRef.current;
    if (!map?.isStyleLoaded()) {
      synchronizeMapStateRef.current?.();
      return;
    }
    map.setPaintProperty(
      "neutral-background",
      "background-color",
      theme === "dark" ? "#101820" : "#eaf0f2",
    );
    if (map.getLayer("graticule-lines")) {
      map.setPaintProperty(
        "graticule-lines",
        "line-color",
        theme === "dark" ? "#35505f" : "#bfd0d7",
      );
    }
    applyProtomapsTheme(map, theme, basemapPresentation);
  }, [theme, basemapPresentation]);

  useEffect(() => {
    synchronizeMapStateRef.current?.();
  }, [point]);

  useEffect(() => {
    if (
      basemapPresentation === "satellite" &&
      !isTrustedSatelliteBasemap(basemap)
    ) {
      setBasemapPresentation("map");
      return;
    }
    synchronizeMapStateRef.current?.();
  }, [basemap, basemapPresentation]);

  useEffect(() => {
    synchronizeMapStateRef.current?.();
  }, [heatmap, preview, heatmapStale]);

  useEffect(() => {
    const returnToOfflineMap = () => {
      if (basemapPresentationRef.current === "satellite") {
        setSatelliteFallback(true);
        setBasemapPresentation("map");
      }
    };
    window.addEventListener("offline", returnToOfflineMap);
    return () => window.removeEventListener("offline", returnToOfflineMap);
  }, []);

  const trustedProtomaps = isTrustedProtomapsBasemap(basemap);
  const trustedTianditu = isTrustedTiandituBasemap(basemap);
  const satelliteAvailable = isTrustedSatelliteBasemap(basemap);
  const usingSatellite = satelliteAvailable && basemapPresentation === "satellite";
  return (
    <section className="map-shell" aria-label="发射点选择地图">
      <div ref={containerRef} className="map-canvas" />
      <div className="map-warning">
        <span className="map-warning-dot" />
        {usingSatellite
          ? "Sentinel-2 卫星影像（联网）· 中文地名 · 内部验证"
          : satelliteFallback
          ? "卫星影像不可用，已切回区域离线地图"
          : trustedProtomaps
          ? "区域离线底图 · 内部验证"
          : trustedTianditu
          ? "天地图在线真实底图 · 内部验证"
          : "WGS84 内部测试画布 · 未配置真实底图"}
      </div>
      {satelliteAvailable && (
        <div className="map-style-switch" role="group" aria-label="底图样式">
          <button
            type="button"
            className={basemapPresentation === "map" ? "active" : undefined}
            aria-pressed={basemapPresentation === "map"}
            onClick={() => {
              setSatelliteFallback(false);
              setBasemapPresentation("map");
            }}
          >
            地图
          </button>
          <button
            type="button"
            className={basemapPresentation === "satellite" ? "active" : undefined}
            aria-pressed={basemapPresentation === "satellite"}
            onClick={() => {
              setSatelliteFallback(false);
              setBasemapPresentation("satellite");
            }}
          >
            卫星 <small>联网</small>
          </button>
        </div>
      )}
      {isTrustedBasemap(basemap) && (
        <div className="map-attribution">
          {usingSatellite
            ? `${basemap.satellite?.attribution} · ${basemap.attribution} 地名`
            : `${basemap.attribution} · ${trustedProtomaps ? "本地区域底图" : "在线底图"}`}
        </div>
      )}
      {!point && (
        <div className="map-empty-state">
          <div className="map-crosshair" aria-hidden="true" />
          <strong>在地图上单击设置发射点</strong>
          <span>选择后显示固定 200 km 计算范围</span>
        </div>
      )}
      {point && (
        <div className="map-point-card">
          <span>发射点</span>
          <strong>
            {point.lat.toFixed(5)}°, {point.lon.toFixed(5)}°
          </strong>
          <small>{maidenheadLocator(point)}</small>
        </div>
      )}
    </section>
  );
}
