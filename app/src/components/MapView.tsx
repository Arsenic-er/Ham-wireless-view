import { useEffect, useRef } from "react";
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
import { buildMapOverlayImageSpec } from "../lib/mapOverlay";
import type { CalculationResult, MapPoint, ResolvedTheme } from "../lib/types";

interface MapViewProps {
  theme: ResolvedTheme;
  point: MapPoint | null;
  heatmap: CalculationResult | null;
  heatmapStale: boolean;
  onPointSelect: (point: MapPoint) => void;
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
  heatmap: CalculationResult | null,
  stale: boolean,
): void {
  const source = map.getSource("coverage-heatmap") as ImageSource | undefined;
  if (!heatmap) {
    if (map.getLayer("coverage-heatmap-layer")) map.removeLayer("coverage-heatmap-layer");
    if (source) map.removeSource("coverage-heatmap");
    return;
  }
  const image = buildMapOverlayImageSpec(heatmap);
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
    "coverage-circle-fill",
  );
}

export function MapView({
  theme,
  point,
  heatmap,
  heatmapStale,
  onPointSelect,
}: MapViewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const pointRef = useRef(point);
  const heatmapRef = useRef(heatmap);
  const heatmapStaleRef = useRef(heatmapStale);
  const onPointSelectRef = useRef(onPointSelect);
  pointRef.current = point;
  heatmapRef.current = heatmap;
  heatmapStaleRef.current = heatmapStale;
  onPointSelectRef.current = onPointSelect;

  useEffect(() => {
    if (!containerRef.current) return;
    const dark = theme === "dark";
    const map = new maplibregl.Map({
      container: containerRef.current,
      center: [104, 35],
      zoom: 3.25,
      minZoom: 2.3,
      maxZoom: 10,
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
      updateHeatmap(map, heatmapRef.current, heatmapStaleRef.current);
    });
    map.on("click", (event) => {
      onPointSelectRef.current({ lat: event.lngLat.lat, lon: event.lngLat.lng });
    });
    return () => {
      mapRef.current = null;
      map.remove();
    };
  }, []);

  useEffect(() => {
    const map = mapRef.current;
    if (!map?.isStyleLoaded()) return;
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
  }, [theme]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map?.isStyleLoaded()) return;
    updateSelection(map, point);
  }, [point]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map?.isStyleLoaded()) return;
    updateHeatmap(map, heatmap, heatmapStale);
  }, [heatmap, heatmapStale]);

  return (
    <section className="map-shell" aria-label="发射点选择地图">
      <div ref={containerRef} className="map-canvas" />
      <div className="map-warning">
        <span className="map-warning-dot" />
        WGS84 内部测试画布 · 不含行政边界
      </div>
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
