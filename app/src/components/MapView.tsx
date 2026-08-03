// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { FeatureCollection } from "geojson";
import maplibregl, {
  type CanvasSource,
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
  CARTO_BASE_SOURCE_ID,
  CARTO_LABEL_SOURCE_ID,
  SATELLITE_SOURCE_ID,
  TIANDITU_IMAGERY_LABEL_SOURCE_ID,
  TIANDITU_IMAGERY_SOURCE_ID,
  TIANDITU_LABEL_SOURCE_ID,
  TIANDITU_VECTOR_SOURCE_ID,
  firstBasemapLabelLayerId,
  isTrustedBasemap,
  isTrustedCartoBasemap,
  isTrustedOnlineBasemap,
  isTrustedSatelliteBasemap,
  isTrustedTiandituBasemap,
  type BasemapPresentation,
  synchronizeBasemap,
} from "../lib/basemap";
import { MapOverlayCanvasLease } from "../lib/mapOverlayCanvas";
import { MapOverlayBlobUrlLease, buildMapOverlayImageSpec } from "../lib/mapOverlay";
import type {
  AnalysisMode,
  BasemapInfo,
  CalculationPreview,
  LinkAnalysisResult,
  MapPoint,
  OnlineBasemapInfo,
  ResolvedTheme,
  SessionCoverageResult,
} from "../lib/types";

interface MapViewProps {
  theme: ResolvedTheme;
  point: MapPoint | null;
  analysisMode?: AnalysisMode;
  linkTx?: MapPoint | null;
  linkRx?: MapPoint | null;
  linkResult?: LinkAnalysisResult | null;
  heatmaps: readonly SessionCoverageResult[];
  activeHeatmapId: string | null;
  preview: CalculationPreview | null;
  heatmapStale: boolean;
  visibleSignalThresholdDbm?: number;
  onPointSelect: (point: MapPoint) => void;
  basemap?: BasemapInfo | null;
  onlineBasemap?: OnlineBasemapInfo | null;
}

const BASEMAP_LABEL_SOURCE_IDS = new Set([
  CARTO_LABEL_SOURCE_ID,
  TIANDITU_LABEL_SOURCE_ID,
  TIANDITU_IMAGERY_LABEL_SOURCE_ID,
]);

const ORDINARY_BASEMAP_SOURCE_IDS = new Set([
  CARTO_BASE_SOURCE_ID,
  TIANDITU_VECTOR_SOURCE_ID,
]);

const SATELLITE_BASEMAP_SOURCE_IDS = new Set([
  SATELLITE_SOURCE_ID,
  TIANDITU_IMAGERY_SOURCE_ID,
]);

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

function linkEndpointData(
  tx: MapPoint | null,
  rx: MapPoint | null,
): FeatureCollection {
  const features: FeatureCollection["features"] = [];
  if (tx) {
    features.push({
      type: "Feature",
      properties: { role: "tx" },
      geometry: { type: "Point", coordinates: [tx.lon, tx.lat] },
    });
  }
  if (rx) {
    features.push({
      type: "Feature",
      properties: { role: "rx" },
      geometry: { type: "Point", coordinates: [rx.lon, rx.lat] },
    });
  }
  return { type: "FeatureCollection", features };
}

function linkPathData(result: LinkAnalysisResult | null): FeatureCollection {
  if (!result || result.profile.length < 2) return EMPTY_FEATURE_COLLECTION;
  return {
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        properties: {},
        geometry: {
          type: "LineString",
          coordinates: result.profile.map((sample) => [sample.lon, sample.lat]),
        },
      },
    ],
  };
}

function completedPointData(heatmaps: readonly SessionCoverageResult[]): FeatureCollection {
  return {
    type: "FeatureCollection",
    features: heatmaps.map(({ id, result }) => ({
      type: "Feature",
      properties: { id },
      geometry: {
        type: "Point",
        coordinates: [result.center.lon, result.center.lat],
      },
    })),
  };
}

function updateSelection(
  map: MapLibreMap,
  point: MapPoint | null,
  heatmaps: readonly SessionCoverageResult[],
  linkTx: MapPoint | null,
  linkRx: MapPoint | null,
  linkResult: LinkAnalysisResult | null,
): void {
  (map.getSource("selected-point") as GeoJSONSource | undefined)?.setData(
    selectedPointData(point),
  );
  (map.getSource("coverage-circle") as GeoJSONSource | undefined)?.setData(
    coverageCircleData(point),
  );
  (map.getSource("completed-points") as GeoJSONSource | undefined)?.setData(
    completedPointData(heatmaps),
  );
  (map.getSource("link-endpoints") as GeoJSONSource | undefined)?.setData(
    linkEndpointData(linkTx, linkRx),
  );
  (map.getSource("link-path") as GeoJSONSource | undefined)?.setData(
    linkPathData(linkResult),
  );
}

function overlayIds(id: string): { sourceId: string; layerId: string } {
  const safeId = id.replace(/[^a-zA-Z0-9_-]/g, "-");
  return {
    sourceId: `coverage-heatmap-${safeId}`,
    layerId: `coverage-heatmap-layer-${safeId}`,
  };
}

function removeMapOverlay(map: MapLibreMap, sourceId: string, layerId: string): void {
  if (map.getLayer(layerId)) map.removeLayer(layerId);
  if (map.getSource(sourceId)) map.removeSource(sourceId);
}

function updateImageOverlay(
  map: MapLibreMap,
  sourceId: string,
  layerId: string,
  heatmap: SessionCoverageResult["result"] | CalculationPreview,
  opacity: number,
  blobUrls: MapOverlayBlobUrlLease,
): void {
  const source = map.getSource(sourceId) as ImageSource | undefined;
  const dataImage = buildMapOverlayImageSpec(heatmap);
  const image = { ...dataImage, url: blobUrls.acquire(dataImage.url) };
  if (source) {
    source.updateImage(image);
    map.setPaintProperty(layerId, "raster-opacity", opacity);
    return;
  }
  map.addSource(sourceId, { type: "image", ...image });
  map.addLayer(
    {
      id: layerId,
      type: "raster",
      source: sourceId,
      paint: { "raster-opacity": opacity, "raster-resampling": "linear" },
    },
    firstBasemapLabelLayerId(map) ?? "coverage-circle-fill",
  );
}

function refreshCanvasSource(map: MapLibreMap, sourceId: string): boolean {
  const source = map.getSource(sourceId) as CanvasSource | undefined;
  if (!source || Object.keys(source.tiles).length === 0) {
    return false;
  }
  source.play();
  source.pause();
  return true;
}

function canvasOverlayIntersectsViewport(
  map: MapLibreMap,
  coordinates: readonly (readonly [number, number])[],
): boolean {
  const bounds = map.getBounds();
  const west = Math.min(...coordinates.map(([lon]) => lon));
  const east = Math.max(...coordinates.map(([lon]) => lon));
  const south = Math.min(...coordinates.map(([, lat]) => lat));
  const north = Math.max(...coordinates.map(([, lat]) => lat));
  return (
    east >= bounds.getWest() &&
    west <= bounds.getEast() &&
    north >= bounds.getSouth() &&
    south <= bounds.getNorth()
  );
}

function refreshDirtyCanvasOverlay(
  map: MapLibreMap,
  sourceId: string,
  lease: MapOverlayCanvasLease,
): boolean {
  const coordinates = lease.coordinates;
  if (
    !lease.ready ||
    !lease.dirty ||
    !coordinates ||
    !canvasOverlayIntersectsViewport(map, coordinates) ||
    !refreshCanvasSource(map, sourceId)
  ) {
    return false;
  }
  lease.markUploaded();
  return true;
}

function refreshDirtyCanvasOverlays(
  map: MapLibreMap,
  canvasLeases: ReadonlyMap<string, MapOverlayCanvasLease>,
): void {
  for (const [id, lease] of canvasLeases) {
    refreshDirtyCanvasOverlay(map, overlayIds(id).sourceId, lease);
  }
}

function ensureCanvasOverlay(
  map: MapLibreMap,
  sourceId: string,
  layerId: string,
  lease: MapOverlayCanvasLease,
  opacity: number,
): void {
  const coordinates = lease.coordinates;
  if (!lease.ready || !coordinates) {
    removeMapOverlay(map, sourceId, layerId);
    return;
  }
  const source = map.getSource(sourceId) as CanvasSource | undefined;
  if (source) {
    source.setCoordinates(coordinates);
    map.setPaintProperty(layerId, "raster-opacity", opacity);
    refreshDirtyCanvasOverlay(map, sourceId, lease);
    return;
  }
  map.addSource(sourceId, {
    type: "canvas",
    canvas: lease.canvas,
    animate: false,
    coordinates,
  });
  map.addLayer(
    {
      id: layerId,
      type: "raster",
      source: sourceId,
      paint: { "raster-opacity": opacity, "raster-resampling": "linear" },
    },
    firstBasemapLabelLayerId(map) ?? "coverage-circle-fill",
  );
  refreshDirtyCanvasOverlay(map, sourceId, lease);
}

function updateHeatmaps(
  map: MapLibreMap,
  heatmaps: readonly SessionCoverageResult[],
  activeHeatmapId: string | null,
  preview: CalculationPreview | null,
  stale: boolean,
  visibleSignalThresholdDbm: number,
  canvasLeases: Map<string, MapOverlayCanvasLease>,
  renderedIds: Set<string>,
  previewBlobUrls: MapOverlayBlobUrlLease,
  onCanvasReady: () => void,
): void {
  const desiredIds = new Set(heatmaps.map(({ id }) => id));
  for (const id of [...renderedIds]) {
    if (desiredIds.has(id)) continue;
    const { sourceId, layerId } = overlayIds(id);
    removeMapOverlay(map, sourceId, layerId);
    canvasLeases.get(id)?.dispose();
    canvasLeases.delete(id);
    renderedIds.delete(id);
  }

  for (const entry of heatmaps) {
    const ids = overlayIds(entry.id);
    let lease = canvasLeases.get(entry.id);
    if (!lease) {
      lease = new MapOverlayCanvasLease();
      canvasLeases.set(entry.id, lease);
    }
    lease.update(entry.result, visibleSignalThresholdDbm, onCanvasReady);
    ensureCanvasOverlay(
      map,
      ids.sourceId,
      ids.layerId,
      lease,
      stale && entry.id === activeHeatmapId ? 0.28 : 0.84,
    );
    renderedIds.add(entry.id);
  }

  const previewIds = overlayIds("preview");
  if (preview) {
    updateImageOverlay(
      map,
      previewIds.sourceId,
      previewIds.layerId,
      preview,
      0.84,
      previewBlobUrls,
    );
  } else {
    removeMapOverlay(map, previewIds.sourceId, previewIds.layerId);
    previewBlobUrls.clear();
  }
}

export function MapView({
  theme,
  point,
  analysisMode = "coverage",
  linkTx = null,
  linkRx = null,
  linkResult = null,
  heatmaps,
  activeHeatmapId,
  preview,
  heatmapStale,
  visibleSignalThresholdDbm = -140,
  onPointSelect,
  basemap,
  onlineBasemap,
}: MapViewProps) {
  const { t, i18n } = useTranslation();
  const [basemapPresentation, setBasemapPresentation] =
    useState<BasemapPresentation>("map");
  const [satelliteFallback, setSatelliteFallback] = useState(false);
  const [ordinaryMapFallback, setOrdinaryMapFallback] = useState(false);
  const [onlineBasemapFailed, setOnlineBasemapFailed] = useState(false);
  const [unavailableSourceIds, setUnavailableSourceIds] = useState<
    ReadonlySet<string>
  >(() => new Set());
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const synchronizeMapStateRef = useRef<(() => void) | null>(null);
  const heatmapCanvasLeasesRef = useRef<Map<string, MapOverlayCanvasLease> | null>(null);
  const previewBlobUrlsRef = useRef<MapOverlayBlobUrlLease | null>(null);
  const renderedHeatmapIdsRef = useRef<Set<string>>(new Set());
  const thresholdAnimationFrameRef = useRef<number | null>(null);
  const thresholdTimerRef = useRef<number | null>(null);
  const lastThresholdPaintRef = useRef(Number.NEGATIVE_INFINITY);
  if (!heatmapCanvasLeasesRef.current) {
    heatmapCanvasLeasesRef.current = new Map();
  }
  if (!previewBlobUrlsRef.current) {
    previewBlobUrlsRef.current = new MapOverlayBlobUrlLease();
  }
  const themeRef = useRef(theme);
  const pointRef = useRef(point);
  const linkTxRef = useRef(linkTx);
  const linkRxRef = useRef(linkRx);
  const linkResultRef = useRef(linkResult);
  const heatmapsRef = useRef(heatmaps);
  const activeHeatmapIdRef = useRef(activeHeatmapId);
  const previewRef = useRef(preview);
  const heatmapStaleRef = useRef(heatmapStale);
  const visibleSignalThresholdDbmRef = useRef(visibleSignalThresholdDbm);
  const onPointSelectRef = useRef(onPointSelect);
  const basemapRef = useRef(basemap);
  const onlineBasemapRef = useRef(onlineBasemap);
  const onlineBasemapFailedRef = useRef(onlineBasemapFailed);
  const unavailableSourceIdsRef =
    useRef<ReadonlySet<string>>(unavailableSourceIds);
  const basemapPresentationRef = useRef(basemapPresentation);
  themeRef.current = theme;
  pointRef.current = point;
  linkTxRef.current = linkTx;
  linkRxRef.current = linkRx;
  linkResultRef.current = linkResult;
  heatmapsRef.current = heatmaps;
  activeHeatmapIdRef.current = activeHeatmapId;
  previewRef.current = preview;
  heatmapStaleRef.current = heatmapStale;
  visibleSignalThresholdDbmRef.current = visibleSignalThresholdDbm;
  onPointSelectRef.current = onPointSelect;
  basemapRef.current = basemap;
  basemapPresentationRef.current = basemapPresentation;
  onlineBasemapRef.current = onlineBasemap;
  onlineBasemapFailedRef.current = onlineBasemapFailed;
  unavailableSourceIdsRef.current = unavailableSourceIds;

  useEffect(() => {
    if (!containerRef.current || !heatmapCanvasLeasesRef.current || !previewBlobUrlsRef.current) return;
    const heatmapCanvasLeases = heatmapCanvasLeasesRef.current;
    const previewBlobUrls = previewBlobUrlsRef.current;
    const renderedHeatmapIds = renderedHeatmapIdsRef.current;
    const dark = theme === "dark";
    const map = new maplibregl.Map({
      container: containerRef.current,
      center: [104, 35],
      zoom: 3.25,
      minZoom: 2.3,
      maxZoom: 18,
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
      map.setMaxZoom(18);
      const failed = onlineBasemapFailedRef.current;
      synchronizeBasemap(
        map,
        failed ? null : basemapRef.current,
        themeRef.current,
        basemapPresentationRef.current,
        failed ? null : onlineBasemapRef.current,
        unavailableSourceIdsRef.current,
      );
      updateSelection(
        map,
        pointRef.current,
        heatmapsRef.current,
        linkTxRef.current,
        linkRxRef.current,
        linkResultRef.current,
      );
      updateHeatmaps(
        map,
        heatmapsRef.current,
        activeHeatmapIdRef.current,
        previewRef.current,
        heatmapStaleRef.current,
        visibleSignalThresholdDbmRef.current,
        heatmapCanvasLeases,
        renderedHeatmapIds,
        previewBlobUrls,
        () => synchronizeMapStateRef.current?.(),
      );
    };
    const replayPendingMapState = () => {
      if (synchronizationPending) synchronizeDesiredMapState();
      refreshDirtyCanvasOverlays(map, heatmapCanvasLeases);
    };
    const refreshSettledCanvasOverlays = () =>
      refreshDirtyCanvasOverlays(map, heatmapCanvasLeases);
    synchronizeMapStateRef.current = synchronizeDesiredMapState;
    map.on("styledata", replayPendingMapState);
    map.on("idle", replayPendingMapState);
    map.on("moveend", refreshSettledCanvasOverlays);
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
      map.addSource("link-path", {
        type: "geojson",
        data: linkPathData(linkResultRef.current),
      });
      map.addLayer({
        id: "link-path-casing",
        type: "line",
        source: "link-path",
        paint: {
          "line-color": "#ffffff",
          "line-width": 5,
          "line-opacity": 0.8,
        },
      });
      map.addLayer({
        id: "link-path-line",
        type: "line",
        source: "link-path",
        paint: {
          "line-color": "#8b5cf6",
          "line-width": 3,
          "line-dasharray": [2, 1.5],
        },
      });
      map.addSource("completed-points", {
        type: "geojson",
        data: completedPointData(heatmapsRef.current),
      });
      map.addLayer({
        id: "completed-point-halo",
        type: "circle",
        source: "completed-points",
        paint: {
          "circle-radius": 9,
          "circle-color": "#087f74",
          "circle-opacity": 0.2,
        },
      });
      map.addLayer({
        id: "completed-point-core",
        type: "circle",
        source: "completed-points",
        paint: {
          "circle-radius": 4,
          "circle-color": "#087f74",
          "circle-stroke-color": "#ffffff",
          "circle-stroke-width": 1.5,
        },
      });
      map.addSource("link-endpoints", {
        type: "geojson",
        data: linkEndpointData(linkTxRef.current, linkRxRef.current),
      });
      map.addLayer({
        id: "link-tx-marker",
        type: "circle",
        source: "link-endpoints",
        filter: ["==", ["get", "role"], "tx"],
        paint: {
          "circle-radius": 7,
          "circle-color": "#ff5c35",
          "circle-stroke-color": "#ffffff",
          "circle-stroke-width": 2.5,
        },
      });
      map.addLayer({
        id: "link-rx-marker",
        type: "circle",
        source: "link-endpoints",
        filter: ["==", ["get", "role"], "rx"],
        paint: {
          "circle-radius": 7,
          "circle-color": "#2563eb",
          "circle-stroke-color": "#ffffff",
          "circle-stroke-width": 2.5,
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
      const sourceId = (event as { sourceId?: string }).sourceId;
      if (!sourceId || !map.getSource(sourceId)) return;

      const trustedCustom = isTrustedOnlineBasemap(onlineBasemapRef.current);
      const trustedSameOrigin = isTrustedTiandituBasemap(basemapRef.current);
      const trustedCarto = isTrustedCartoBasemap(basemapRef.current);
      const trustedSatellite = isTrustedSatelliteBasemap(basemapRef.current);
      const ordinarySourceId =
        trustedCustom || trustedSameOrigin
          ? TIANDITU_VECTOR_SOURCE_ID
          : trustedCarto
            ? CARTO_BASE_SOURCE_ID
            : null;
      const satelliteSourceId = trustedCustom
        ? TIANDITU_IMAGERY_SOURCE_ID
        : trustedSatellite
          ? SATELLITE_SOURCE_ID
          : null;
      const ordinaryAvailable =
        ordinarySourceId !== null &&
        !unavailableSourceIdsRef.current.has(ordinarySourceId);
      const satelliteAvailable =
        satelliteSourceId !== null &&
        !unavailableSourceIdsRef.current.has(satelliteSourceId);
      const markUnavailable = () => {
        const next = new Set(unavailableSourceIdsRef.current);
        next.add(sourceId);
        unavailableSourceIdsRef.current = next;
        setUnavailableSourceIds(next);
      };
      const failAllOnlineSources = () => {
        setSatelliteFallback(false);
        setOrdinaryMapFallback(false);
        onlineBasemapFailedRef.current = true;
        setOnlineBasemapFailed(true);
        synchronizeDesiredMapState();
      };

      if (BASEMAP_LABEL_SOURCE_IDS.has(sourceId)) {
        markUnavailable();
        synchronizeDesiredMapState();
        return;
      }

      if (
        SATELLITE_BASEMAP_SOURCE_IDS.has(sourceId) &&
        satelliteAvailable
      ) {
        markUnavailable();
        if (ordinaryAvailable) {
          setOrdinaryMapFallback(false);
          setSatelliteFallback(true);
          basemapPresentationRef.current = "map";
          setBasemapPresentation("map");
          synchronizeDesiredMapState();
        } else {
          failAllOnlineSources();
        }
        return;
      }

      if (
        ORDINARY_BASEMAP_SOURCE_IDS.has(sourceId) &&
        ordinaryAvailable
      ) {
        markUnavailable();
        if (satelliteAvailable) {
          setSatelliteFallback(false);
          setOrdinaryMapFallback(true);
          basemapPresentationRef.current = "satellite";
          setBasemapPresentation("satellite");
          synchronizeDesiredMapState();
        } else {
          failAllOnlineSources();
        }
      }
    });
    return () => {
      if (synchronizeMapStateRef.current === synchronizeDesiredMapState) {
        synchronizeMapStateRef.current = null;
      }
      mapRef.current = null;
      for (const lease of heatmapCanvasLeases.values()) lease.dispose();
      heatmapCanvasLeases.clear();
      renderedHeatmapIds.clear();
      previewBlobUrls.clear();
      map.remove();
    };
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const zoomIn = container.querySelector<HTMLButtonElement>(".maplibregl-ctrl-zoom-in");
    const zoomOut = container.querySelector<HTMLButtonElement>(".maplibregl-ctrl-zoom-out");
    for (const [button, label] of [[zoomIn, t("zoomIn")], [zoomOut, t("zoomOut")]] as const) {
      if (!button) continue;
      button.title = label;
      button.setAttribute("aria-label", label);
    }
  }, [i18n.resolvedLanguage, t]);

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
  }, [theme]);

  useEffect(() => {
    synchronizeMapStateRef.current?.();
  }, [point, linkTx, linkRx, linkResult]);

  useEffect(() => {
    if (
      basemapPresentation === "satellite" &&
      !onlineBasemapFailed &&
      !isTrustedSatelliteBasemap(basemap) &&
      !isTrustedOnlineBasemap(onlineBasemap)
    ) {
      setBasemapPresentation("map");
      return;
    }
    synchronizeMapStateRef.current?.();
  }, [
    basemap,
    onlineBasemap,
    onlineBasemapFailed,
    basemapPresentation,
    unavailableSourceIds,
  ]);

  useEffect(() => {
    const available = new Set<string>();
    unavailableSourceIdsRef.current = available;
    setUnavailableSourceIds(available);
    onlineBasemapFailedRef.current = false;
    setOnlineBasemapFailed(false);
    setSatelliteFallback(false);
    setOrdinaryMapFallback(false);
    synchronizeMapStateRef.current?.();
  }, [basemap, onlineBasemap]);
  useEffect(() => {
    synchronizeMapStateRef.current?.();
  }, [heatmaps, activeHeatmapId, preview, heatmapStale]);

  useEffect(() => {
    const requestFrame =
      window.requestAnimationFrame?.bind(window) ??
      ((callback: FrameRequestCallback) =>
        window.setTimeout(() => callback(performance.now()), 16));
    const cancelFrame =
      window.cancelAnimationFrame?.bind(window) ?? window.clearTimeout.bind(window);
    const paintThreshold = (timestamp: number) => {
      thresholdAnimationFrameRef.current = null;
      const waitMs = 1000 / 30 - (timestamp - lastThresholdPaintRef.current);
      if (waitMs > 1) {
        thresholdTimerRef.current = window.setTimeout(() => {
          thresholdTimerRef.current = null;
          thresholdAnimationFrameRef.current = requestFrame(paintThreshold);
        }, waitMs);
        return;
      }
      lastThresholdPaintRef.current = timestamp;
      const map = mapRef.current;
      const leases = heatmapCanvasLeasesRef.current;
      if (!map?.isStyleLoaded() || !leases) return;
      for (const [id, lease] of leases) {
        lease.applyThreshold(visibleSignalThresholdDbmRef.current);
        refreshDirtyCanvasOverlay(map, overlayIds(id).sourceId, lease);
      }
    };
    thresholdAnimationFrameRef.current = requestFrame(paintThreshold);
    return () => {
      if (thresholdAnimationFrameRef.current !== null) {
        cancelFrame(thresholdAnimationFrameRef.current);
        thresholdAnimationFrameRef.current = null;
      }
      if (thresholdTimerRef.current !== null) {
        window.clearTimeout(thresholdTimerRef.current);
        thresholdTimerRef.current = null;
      }
    };
  }, [visibleSignalThresholdDbm]);

  const retryOnlineBasemap = () => {
    if (
      !isTrustedOnlineBasemap(onlineBasemapRef.current) &&
      !isTrustedTiandituBasemap(basemapRef.current) &&
      !isTrustedCartoBasemap(basemapRef.current) &&
      !isTrustedSatelliteBasemap(basemapRef.current)
    ) {
      return;
    }
    const available = new Set<string>();
    unavailableSourceIdsRef.current = available;
    setUnavailableSourceIds(available);
    onlineBasemapFailedRef.current = false;
    setOnlineBasemapFailed(false);
    setSatelliteFallback(false);
    setOrdinaryMapFallback(false);
    synchronizeMapStateRef.current?.();
  };

  useEffect(() => {
    const returnToGrid = () => {
      if (
        isTrustedOnlineBasemap(onlineBasemapRef.current) ||
        isTrustedTiandituBasemap(basemapRef.current) ||
        isTrustedCartoBasemap(basemapRef.current) ||
        isTrustedSatelliteBasemap(basemapRef.current)
      ) {
        setSatelliteFallback(false);
        setOrdinaryMapFallback(false);
        onlineBasemapFailedRef.current = true;
        setOnlineBasemapFailed(true);
        synchronizeMapStateRef.current?.();
      }
    };
    window.addEventListener("offline", returnToGrid);
    window.addEventListener("online", retryOnlineBasemap);
    return () => {
      window.removeEventListener("offline", returnToGrid);
      window.removeEventListener("online", retryOnlineBasemap);
    };
  }, []);

  const trustedTianditu =
    !onlineBasemapFailed && isTrustedTiandituBasemap(basemap);
  const trustedCarto =
    !onlineBasemapFailed && isTrustedCartoBasemap(basemap);
  const trustedLegacyBasemap =
    !onlineBasemapFailed && isTrustedBasemap(basemap) ? basemap : null;
  const trustedOnlineBasemap =
    !onlineBasemapFailed && isTrustedOnlineBasemap(onlineBasemap);
  const trustedSatelliteBasemap =
    !onlineBasemapFailed && isTrustedSatelliteBasemap(basemap);
  const satelliteAvailable = trustedOnlineBasemap || trustedSatelliteBasemap;
  const usingSatellite = satelliteAvailable && basemapPresentation === "satellite";
  const activeLabelSourceId = trustedOnlineBasemap
    ? usingSatellite
      ? TIANDITU_IMAGERY_LABEL_SOURCE_ID
      : TIANDITU_LABEL_SOURCE_ID
    : trustedCarto
      ? CARTO_LABEL_SOURCE_ID
      : trustedTianditu
        ? TIANDITU_LABEL_SOURCE_ID
        : null;
  const activeLabelsUnavailable =
    activeLabelSourceId !== null &&
    unavailableSourceIds.has(activeLabelSourceId);
  const onlineMapUnavailable =
    onlineBasemapFailed &&
    (isTrustedOnlineBasemap(onlineBasemap) ||
      isTrustedTiandituBasemap(basemap) ||
      isTrustedCartoBasemap(basemap) ||
      isTrustedSatelliteBasemap(basemap));
  return (
    <section className="map-shell" aria-label={t("mapAria")}>
      <div ref={containerRef} className="map-canvas" />
      <div className="map-warning">
        <span className="map-warning-dot" />
        {onlineMapUnavailable
          ? t("mapUnavailable")
          : ordinaryMapFallback
            ? t("mapOrdinaryFallback")
            : satelliteFallback
              ? t("mapSatelliteFallback")
              : activeLabelsUnavailable
                ? t("mapLabelsUnavailable")
                : usingSatellite
                  ? trustedOnlineBasemap
                    ? t("mapTiandituSatellite")
                    : trustedCarto
                      ? t("mapCartoSatellite")
                      : trustedTianditu
                        ? t("mapSentinelLabels")
                        : t("mapSentinel")
                  : trustedOnlineBasemap
                    ? t("mapTiandituVector")
                    : trustedCarto
                      ? t("mapCartoVector")
                      : trustedTianditu
                        ? t("mapValidationVector")
                        : t("mapGrid")}
        {(onlineMapUnavailable ||
          ordinaryMapFallback ||
          satelliteFallback ||
          activeLabelsUnavailable) && (
          <button
            type="button"
            className="map-retry"
            onClick={retryOnlineBasemap}
          >
            {t("retry")}
          </button>
        )}
      </div>
      {satelliteAvailable && (
        <div className="map-style-switch" role="group" aria-label={t("basemapStyle")}>
          <button
            type="button"
            className={basemapPresentation === "map" ? "active" : undefined}
            aria-pressed={basemapPresentation === "map"}
            onClick={() => {
              retryOnlineBasemap();
              basemapPresentationRef.current = "map";
              setBasemapPresentation("map");
            }}
          >
            {t("map")}
          </button>
          <button
            type="button"
            className={basemapPresentation === "satellite" ? "active" : undefined}
            aria-pressed={basemapPresentation === "satellite"}
            onClick={() => {
              retryOnlineBasemap();
              basemapPresentationRef.current = "satellite";
              setBasemapPresentation("satellite");
            }}
          >
            {t("satellite")} <small>{t("online")}</small>
          </button>
        </div>
      )}
      {(trustedOnlineBasemap || trustedLegacyBasemap || usingSatellite) && (
        <div className="map-attribution">
          {trustedOnlineBasemap
            ? `${onlineBasemap.attribution} · ${t("onlineBasemapAttribution")}`
            : usingSatellite
              ? trustedLegacyBasemap && !activeLabelsUnavailable
                ? `${basemap?.satellite?.attribution} · ${trustedLegacyBasemap.attribution} ${t("placeLabelsAttribution")}`
                : `${basemap?.satellite?.attribution} · ${t("onlineImageryAttribution")}`
              : `${trustedLegacyBasemap?.attribution} · ${t("onlineBasemapAttribution")}`}
        </div>
      )}
      {analysisMode === "coverage" && !point && (
        <div className="map-empty-state" role="status" aria-live="polite">
          <div className="map-crosshair" aria-hidden="true" />
          <div className="map-empty-copy">
            <strong>{t("mapEmpty")}</strong>
            <span>{t("mapEmptyDetail")}</span>
          </div>
        </div>
      )}
      {analysisMode === "link" && (!linkTx || !linkRx) && (
        <div className="map-empty-state" role="status" aria-live="polite">
          <div className="map-crosshair" aria-hidden="true" />
          <div className="map-empty-copy">
            <strong>{linkTx ? t("selectLinkRx") : t("selectLinkTx")}</strong>
            <span>{linkTx ? t("selectLinkRxDetail") : t("selectLinkTxDetail")}</span>
          </div>
        </div>
      )}
      {analysisMode === "coverage" && point && (
        <div className="map-point-card">
          <span>{t("transmitter")}</span>
          <strong>
            {point.lat.toFixed(5)}°, {point.lon.toFixed(5)}°
          </strong>
          <small>{maidenheadLocator(point)}</small>
        </div>
      )}
      {analysisMode === "link" && linkTx && (
        <div className="map-point-card link-point-card">
          <span>{t("linkTx")}</span>
          <strong>{linkTx.lat.toFixed(5)}°, {linkTx.lon.toFixed(5)}°</strong>
          <small>{maidenheadLocator(linkTx)}</small>
          {linkRx && (
            <>
              <span>{t("linkRx")}</span>
              <strong>{linkRx.lat.toFixed(5)}°, {linkRx.lon.toFixed(5)}°</strong>
              <small>{maidenheadLocator(linkRx)}</small>
            </>
          )}
        </div>
      )}
    </section>
  );
}
