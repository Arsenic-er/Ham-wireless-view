import type { LayerSpecification, Map as MapLibreMap } from "maplibre-gl";

import type { BasemapInfo, OnlineBasemapInfo, ResolvedTheme } from "./types";

export const TIANDITU_TILE_PATH_TEMPLATE =
  "/api/basemap/tianditu/{layer}/{z}/{x}/{y}";
export const TIANDITU_VECTOR_SOURCE_ID = "basemap-tianditu-vector";
export const TIANDITU_LABEL_SOURCE_ID = "basemap-tianditu-label";
export const TIANDITU_VECTOR_LAYER_ID = "basemap-tianditu-vector-layer";
export const TIANDITU_LABEL_LAYER_ID = "basemap-tianditu-label-layer";
export const TIANDITU_IMAGERY_SOURCE_ID = "basemap-tianditu-imagery";
export const TIANDITU_IMAGERY_LABEL_SOURCE_ID = "basemap-tianditu-imagery-label";
export const TIANDITU_IMAGERY_LAYER_ID = "basemap-tianditu-imagery-layer";
export const TIANDITU_IMAGERY_LABEL_LAYER_ID = "basemap-tianditu-imagery-label-layer";

export const TAURI_TIANDITU_VECTOR_TEMPLATE =
  "tianditu://localhost/vec/{z}/{x}/{y}";
export const TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE =
  "tianditu://localhost/cva/{z}/{x}/{y}";
export const TAURI_TIANDITU_IMAGERY_TEMPLATE =
  "tianditu://localhost/img/{z}/{x}/{y}";
export const TAURI_TIANDITU_IMAGERY_LABEL_TEMPLATE =
  "tianditu://localhost/cia/{z}/{x}/{y}";


export const SATELLITE_TILE_PATH_TEMPLATE =
  "/api/basemap/satellite/{z}/{x}/{y}";
export const SATELLITE_SOURCE_ID = "basemap-satellite";
export const SATELLITE_LAYER_ID = "basemap-satellite-layer";
export const SATELLITE_ATTRIBUTION =
  "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)";

export type BasemapPresentation = "map" | "satellite";


export function isTrustedTiandituBasemap(
  basemap: BasemapInfo | null | undefined,
): basemap is BasemapInfo {
  if (
    !basemap?.enabled ||
    basemap.providerId !== "tianditu" ||
    basemap.mode !== "same-origin-proxy" ||
    basemap.tilePathTemplate !== TIANDITU_TILE_PATH_TEMPLATE ||
    !Number.isInteger(basemap.maxZoom) ||
    basemap.maxZoom < 1 ||
    basemap.maxZoom > 18
  ) {
    return false;
  }
  const layerIds = new Set(basemap.layers.map(({ id }) => id));
  return layerIds.has("vec") && layerIds.has("cva");
}
export function isTrustedOnlineBasemap(
  basemap: OnlineBasemapInfo | null | undefined,
): basemap is OnlineBasemapInfo {
  return (
    basemap?.configured === true &&
    basemap.provider === "Tianditu" &&
    basemap.protocolScheme === "tianditu" &&
    basemap.vectorTemplate === TAURI_TIANDITU_VECTOR_TEMPLATE &&
    basemap.vectorLabelTemplate === TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE &&
    basemap.imageryTemplate === TAURI_TIANDITU_IMAGERY_TEMPLATE &&
    basemap.imageryLabelTemplate === TAURI_TIANDITU_IMAGERY_LABEL_TEMPLATE &&
    basemap.minZoom === 1 &&
    basemap.maxZoom === 18 &&
    typeof basemap.attribution === "string" &&
    basemap.attribution.length > 0 &&
    basemap.attribution.length <= 256
  );
}

export function isTrustedSatelliteBasemap(
  basemap: BasemapInfo | null | undefined,
): boolean {
  const satellite = basemap?.satellite;
  return (
    satellite?.enabled === true &&
    satellite.providerId === "eoxcloudless" &&
    satellite.mode === "same-origin-proxy" &&
    satellite.tilePathTemplate === SATELLITE_TILE_PATH_TEMPLATE &&
    satellite.maxZoom === 14 &&
    satellite.attribution === SATELLITE_ATTRIBUTION
  );
}

export function isTrustedBasemap(
  basemap: BasemapInfo | null | undefined,
): basemap is BasemapInfo {
  return isTrustedTiandituBasemap(basemap);
}

function tilePath(layer: "vec" | "cva"): string {
  return TIANDITU_TILE_PATH_TEMPLATE.replace("{layer}", layer);
}

function removeTiandituBasemap(map: MapLibreMap): void {
  for (const layerId of [
    TIANDITU_IMAGERY_LABEL_LAYER_ID,
    TIANDITU_IMAGERY_LAYER_ID,
    TIANDITU_LABEL_LAYER_ID,
    TIANDITU_VECTOR_LAYER_ID,
  ]) {
    if (map.getLayer(layerId)) map.removeLayer(layerId);
  }
  for (const sourceId of [
    TIANDITU_IMAGERY_LABEL_SOURCE_ID,
    TIANDITU_IMAGERY_SOURCE_ID,
    TIANDITU_LABEL_SOURCE_ID,
    TIANDITU_VECTOR_SOURCE_ID,
  ]) {
    if (map.getSource(sourceId)) map.removeSource(sourceId);
  }
}

function removeRasterPair(
  map: MapLibreMap,
  sourceIds: readonly [string, string],
  layerIds: readonly [string, string],
): void {
  for (const layerId of [...layerIds].reverse()) {
    if (map.getLayer(layerId)) map.removeLayer(layerId);
  }
  for (const sourceId of sourceIds) {
    if (map.getSource(sourceId)) map.removeSource(sourceId);
  }
}

function removeSatelliteBasemap(map: MapLibreMap): void {
  if (map.getLayer(SATELLITE_LAYER_ID)) map.removeLayer(SATELLITE_LAYER_ID);
  if (map.getSource(SATELLITE_SOURCE_ID)) map.removeSource(SATELLITE_SOURCE_ID);
}

function setBaseGeometryVisibility(map: MapLibreMap, visible: boolean): void {
  const visibility = visible ? "visible" : "none";
  for (const layerId of [TIANDITU_VECTOR_LAYER_ID]) {
    if (map.getLayer(layerId)) {
      map.setLayoutProperty(layerId, "visibility", visibility);
    }
  }
}

function synchronizeSatelliteBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo,
  presentation: BasemapPresentation,
): void {
  const enabled =
    presentation === "satellite" && isTrustedSatelliteBasemap(basemap);
  setBaseGeometryVisibility(map, !enabled);
  if (!enabled) {
    removeSatelliteBasemap(map);
    return;
  }
  const satellite = basemap.satellite!;
  if (!map.getSource(SATELLITE_SOURCE_ID)) {
    map.addSource(SATELLITE_SOURCE_ID, {
      type: "raster",
      tiles: [satellite.tilePathTemplate],
      tileSize: 256,
      minzoom: 0,
      maxzoom: satellite.maxZoom,
      attribution: satellite.attribution,
    });
  }
  if (!map.getLayer(SATELLITE_LAYER_ID)) {
    addLayerBeforeGraticule(map, {
      id: SATELLITE_LAYER_ID,
      type: "raster",
      source: SATELLITE_SOURCE_ID,
      paint: {
        "raster-fade-duration": 0,
        "raster-saturation": -0.08,
        "raster-contrast": 0.06,
      },
    });
  }
}

function addLayerBeforeGraticule(map: MapLibreMap, layer: LayerSpecification): void {
  map.addLayer(layer, map.getLayer("graticule-lines") ? "graticule-lines" : undefined);
}

function firstTransmitterLayerId(map: MapLibreMap): string | undefined {
  if (map.getLayer("completed-point-halo")) return "completed-point-halo";
  return map.getLayer("selected-point-halo") ? "selected-point-halo" : undefined;
}

function synchronizeTiandituBasemap(map: MapLibreMap, basemap: BasemapInfo): void {
  if (!map.getSource(TIANDITU_VECTOR_SOURCE_ID)) {
    map.addSource(TIANDITU_VECTOR_SOURCE_ID, {
      type: "raster",
      tiles: [tilePath("vec")],
      tileSize: 256,
      minzoom: 0,
      maxzoom: basemap.maxZoom,
      attribution: basemap.attribution,
    });
  }
  if (!map.getLayer(TIANDITU_VECTOR_LAYER_ID)) {
    addLayerBeforeGraticule(map, {
      id: TIANDITU_VECTOR_LAYER_ID,
      type: "raster",
      source: TIANDITU_VECTOR_SOURCE_ID,
      paint: { "raster-fade-duration": 0 },
    });
  }

  if (!map.getSource(TIANDITU_LABEL_SOURCE_ID)) {
    map.addSource(TIANDITU_LABEL_SOURCE_ID, {
      type: "raster",
      tiles: [tilePath("cva")],
      tileSize: 256,
      minzoom: 0,
      maxzoom: basemap.maxZoom,
      attribution: basemap.attribution,
    });
  }
  if (!map.getLayer(TIANDITU_LABEL_LAYER_ID)) {
    map.addLayer(
      {
        id: TIANDITU_LABEL_LAYER_ID,
        type: "raster",
        source: TIANDITU_LABEL_SOURCE_ID,
        paint: { "raster-fade-duration": 0 },
      },
      firstTransmitterLayerId(map),
    );
  }
}

function addOnlineRasterPair(
  map: MapLibreMap,
  basemap: OnlineBasemapInfo,
  base: { sourceId: string; layerId: string; template: string },
  labels: { sourceId: string; layerId: string; template: string },
): void {
  if (!map.getSource(base.sourceId)) {
    map.addSource(base.sourceId, {
      type: "raster",
      tiles: [base.template],
      tileSize: 256,
      minzoom: basemap.minZoom,
      maxzoom: basemap.maxZoom,
      attribution: basemap.attribution,
    });
  }
  if (!map.getLayer(base.layerId)) {
    addLayerBeforeGraticule(map, {
      id: base.layerId,
      type: "raster",
      source: base.sourceId,
      paint: { "raster-fade-duration": 0 },
    });
  }
  if (!map.getSource(labels.sourceId)) {
    map.addSource(labels.sourceId, {
      type: "raster",
      tiles: [labels.template],
      tileSize: 256,
      minzoom: basemap.minZoom,
      maxzoom: basemap.maxZoom,
      attribution: basemap.attribution,
    });
  }
  if (!map.getLayer(labels.layerId)) {
    map.addLayer(
      {
        id: labels.layerId,
        type: "raster",
        source: labels.sourceId,
        paint: { "raster-fade-duration": 0 },
      },
      firstTransmitterLayerId(map),
    );
  }
}

function synchronizeOnlineBasemap(
  map: MapLibreMap,
  basemap: OnlineBasemapInfo,
  presentation: BasemapPresentation,
): void {
  if (presentation === "satellite") {
    removeRasterPair(
      map,
      [TIANDITU_VECTOR_SOURCE_ID, TIANDITU_LABEL_SOURCE_ID],
      [TIANDITU_VECTOR_LAYER_ID, TIANDITU_LABEL_LAYER_ID],
    );
    addOnlineRasterPair(
      map,
      basemap,
      { sourceId: TIANDITU_IMAGERY_SOURCE_ID, layerId: TIANDITU_IMAGERY_LAYER_ID, template: basemap.imageryTemplate },
      { sourceId: TIANDITU_IMAGERY_LABEL_SOURCE_ID, layerId: TIANDITU_IMAGERY_LABEL_LAYER_ID, template: basemap.imageryLabelTemplate },
    );
    return;
  }
  removeRasterPair(
    map,
    [TIANDITU_IMAGERY_SOURCE_ID, TIANDITU_IMAGERY_LABEL_SOURCE_ID],
    [TIANDITU_IMAGERY_LAYER_ID, TIANDITU_IMAGERY_LABEL_LAYER_ID],
  );
  addOnlineRasterPair(
    map,
    basemap,
    { sourceId: TIANDITU_VECTOR_SOURCE_ID, layerId: TIANDITU_VECTOR_LAYER_ID, template: basemap.vectorTemplate },
    { sourceId: TIANDITU_LABEL_SOURCE_ID, layerId: TIANDITU_LABEL_LAYER_ID, template: basemap.vectorLabelTemplate },
  );
}

export function firstBasemapLabelLayerId(map: MapLibreMap): string | undefined {
  for (const layerId of [TIANDITU_LABEL_LAYER_ID, TIANDITU_IMAGERY_LABEL_LAYER_ID]) {
    if (map.getLayer(layerId)) return layerId;
  }
  return undefined;
}

export function synchronizeBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo | null | undefined,
  _theme: ResolvedTheme,
  presentation: BasemapPresentation = "map",
  onlineBasemap?: OnlineBasemapInfo | null,
): void {
  if (isTrustedOnlineBasemap(onlineBasemap)) {
    removeSatelliteBasemap(map);
    synchronizeOnlineBasemap(map, onlineBasemap, presentation);
    return;
  }

  if (isTrustedTiandituBasemap(basemap)) {
    synchronizeTiandituBasemap(map, basemap);
    synchronizeSatelliteBasemap(map, basemap, presentation);
    return;
  }
  removeTiandituBasemap(map);
  if (isTrustedSatelliteBasemap(basemap)) {
    synchronizeSatelliteBasemap(map, basemap!, presentation);
    return;
  }
  removeSatelliteBasemap(map);
}
