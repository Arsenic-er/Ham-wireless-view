// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

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
  "http://tianditu.localhost/vec/{z}/{x}/{y}";
export const TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE =
  "http://tianditu.localhost/cva/{z}/{x}/{y}";
export const TAURI_TIANDITU_IMAGERY_TEMPLATE =
  "http://tianditu.localhost/img/{z}/{x}/{y}";
export const TAURI_TIANDITU_IMAGERY_LABEL_TEMPLATE =
  "http://tianditu.localhost/cia/{z}/{x}/{y}";

export const CARTO_TILE_PATH_TEMPLATE =
  "/api/basemap/carto/{layer}/{z}/{x}/{y}";
export const TAURI_CARTO_TILE_PATH_TEMPLATE =
  "http://basemap.localhost/carto/{layer}/{z}/{x}/{y}";
export const CARTO_ATTRIBUTION =
  "\u00a9 OpenStreetMap contributors \u00a9 CARTO";
export const CARTO_BASE_SOURCE_ID = "basemap-carto-base";
export const CARTO_LABEL_SOURCE_ID = "basemap-carto-labels";
export const CARTO_BASE_LAYER_ID = "basemap-carto-base-layer";
export const CARTO_LABEL_LAYER_ID = "basemap-carto-labels-layer";

export const SATELLITE_TILE_PATH_TEMPLATE =
  "/api/basemap/satellite/{z}/{x}/{y}";
export const TAURI_SATELLITE_TILE_PATH_TEMPLATE =
  "http://basemap.localhost/eox/satellite/{z}/{x}/{y}";
export const SATELLITE_SOURCE_ID = "basemap-satellite";
export const SATELLITE_LAYER_ID = "basemap-satellite-layer";
export const SATELLITE_ATTRIBUTION =
  "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)";

export type BasemapPresentation = "map" | "satellite";

function hasExactlyLayers(
  basemap: BasemapInfo,
  expected: readonly BasemapInfo["layers"][number]["id"][],
): boolean {
  const layerIds = new Set(basemap.layers.map(({ id }) => id));
  return (
    basemap.layers.length === expected.length &&
    layerIds.size === expected.length &&
    expected.every((id) => layerIds.has(id))
  );
}

export function isTrustedTiandituBasemap(
  basemap: BasemapInfo | null | undefined,
): basemap is BasemapInfo {
  return Boolean(
    basemap?.enabled &&
      basemap.providerId === "tianditu" &&
      basemap.mode === "same-origin-proxy" &&
      basemap.tilePathTemplate === TIANDITU_TILE_PATH_TEMPLATE &&
      Number.isInteger(basemap.maxZoom) &&
      basemap.maxZoom >= 1 &&
      basemap.maxZoom <= 18 &&
      hasExactlyLayers(basemap, ["vec", "cva"]),
  );
}

export function isTrustedCartoBasemap(
  basemap: BasemapInfo | null | undefined,
): basemap is BasemapInfo {
  const trustedTransport =
    (basemap?.mode === "same-origin-proxy" &&
      basemap.tilePathTemplate === CARTO_TILE_PATH_TEMPLATE) ||
    (basemap?.mode === "desktop-protocol-proxy" &&
      basemap.tilePathTemplate === TAURI_CARTO_TILE_PATH_TEMPLATE);
  return Boolean(
    basemap?.enabled &&
      basemap.providerId === "carto-voyager" &&
      trustedTransport &&
      basemap.maxZoom === 18 &&
      basemap.attribution === CARTO_ATTRIBUTION &&
      hasExactlyLayers(basemap, ["base", "labels"]),
  );
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
  const trustedTransport =
    (satellite?.mode === "same-origin-proxy" &&
      satellite.tilePathTemplate === SATELLITE_TILE_PATH_TEMPLATE) ||
    (satellite?.mode === "desktop-protocol-proxy" &&
      satellite.tilePathTemplate === TAURI_SATELLITE_TILE_PATH_TEMPLATE);
  return (
    satellite?.enabled === true &&
    satellite.providerId === "eoxcloudless" &&
    trustedTransport &&
    satellite.maxZoom === 14 &&
    satellite.attribution === SATELLITE_ATTRIBUTION
  );
}

export function isTrustedBasemap(
  basemap: BasemapInfo | null | undefined,
): basemap is BasemapInfo {
  return isTrustedTiandituBasemap(basemap) || isTrustedCartoBasemap(basemap);
}

function tilePath(template: string, layer: string): string {
  return template.replace("{layer}", layer);
}

function removeRasterLayerSource(
  map: MapLibreMap,
  sourceId: string,
  layerId: string,
): void {
  if (map.getLayer(layerId)) map.removeLayer(layerId);
  if (map.getSource(sourceId)) map.removeSource(sourceId);
}

function removeTiandituBasemap(map: MapLibreMap): void {
  for (const [sourceId, layerId] of [
    [TIANDITU_IMAGERY_LABEL_SOURCE_ID, TIANDITU_IMAGERY_LABEL_LAYER_ID],
    [TIANDITU_IMAGERY_SOURCE_ID, TIANDITU_IMAGERY_LAYER_ID],
    [TIANDITU_LABEL_SOURCE_ID, TIANDITU_LABEL_LAYER_ID],
    [TIANDITU_VECTOR_SOURCE_ID, TIANDITU_VECTOR_LAYER_ID],
  ] as const) {
    removeRasterLayerSource(map, sourceId, layerId);
  }
}

function removeCartoBasemap(map: MapLibreMap): void {
  removeRasterLayerSource(map, CARTO_LABEL_SOURCE_ID, CARTO_LABEL_LAYER_ID);
  removeRasterLayerSource(map, CARTO_BASE_SOURCE_ID, CARTO_BASE_LAYER_ID);
}

function removeSatelliteBasemap(map: MapLibreMap): void {
  removeRasterLayerSource(map, SATELLITE_SOURCE_ID, SATELLITE_LAYER_ID);
}

function addLayerBeforeGraticule(map: MapLibreMap, layer: LayerSpecification): void {
  map.addLayer(layer, map.getLayer("graticule-lines") ? "graticule-lines" : undefined);
}

function firstTransmitterLayerId(map: MapLibreMap): string | undefined {
  if (map.getLayer("completed-point-halo")) return "completed-point-halo";
  return map.getLayer("selected-point-halo") ? "selected-point-halo" : undefined;
}

function synchronizeRasterLayer(
  map: MapLibreMap,
  config: {
    sourceId: string;
    layerId: string;
    template: string;
    minZoom: number;
    maxZoom: number;
    attribution: string;
    labels: boolean;
    paint?: Record<string, unknown>;
  },
  unavailableSourceIds: ReadonlySet<string>,
): void {
  if (unavailableSourceIds.has(config.sourceId)) {
    removeRasterLayerSource(map, config.sourceId, config.layerId);
    return;
  }
  if (!map.getSource(config.sourceId)) {
    map.addSource(config.sourceId, {
      type: "raster",
      tiles: [config.template],
      tileSize: 256,
      minzoom: config.minZoom,
      maxzoom: config.maxZoom,
      attribution: config.attribution,
    });
  }
  if (map.getLayer(config.layerId)) return;
  const layer: LayerSpecification = {
    id: config.layerId,
    type: "raster",
    source: config.sourceId,
    paint: {
      "raster-fade-duration": 0,
      ...config.paint,
    },
  };
  if (config.labels) {
    map.addLayer(layer, firstTransmitterLayerId(map));
  } else {
    addLayerBeforeGraticule(map, layer);
  }
}

function setBaseGeometryVisibility(map: MapLibreMap, visible: boolean): void {
  const visibility = visible ? "visible" : "none";
  for (const layerId of [TIANDITU_VECTOR_LAYER_ID, CARTO_BASE_LAYER_ID]) {
    if (map.getLayer(layerId)) {
      map.setLayoutProperty(layerId, "visibility", visibility);
    }
  }
}

function synchronizeSatelliteBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo,
  presentation: BasemapPresentation,
  unavailableSourceIds: ReadonlySet<string>,
): void {
  const enabled =
    presentation === "satellite" &&
    isTrustedSatelliteBasemap(basemap) &&
    !unavailableSourceIds.has(SATELLITE_SOURCE_ID);
  setBaseGeometryVisibility(map, !enabled);
  if (!enabled) {
    removeSatelliteBasemap(map);
    return;
  }
  const satellite = basemap.satellite!;
  synchronizeRasterLayer(
    map,
    {
      sourceId: SATELLITE_SOURCE_ID,
      layerId: SATELLITE_LAYER_ID,
      template: satellite.tilePathTemplate,
      minZoom: 0,
      maxZoom: satellite.maxZoom,
      attribution: satellite.attribution,
      labels: false,
      paint: {
        "raster-saturation": -0.08,
        "raster-contrast": 0.06,
      },
    },
    unavailableSourceIds,
  );
}

function synchronizeTiandituBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo,
  unavailableSourceIds: ReadonlySet<string>,
): void {
  synchronizeRasterLayer(
    map,
    {
      sourceId: TIANDITU_VECTOR_SOURCE_ID,
      layerId: TIANDITU_VECTOR_LAYER_ID,
      template: tilePath(TIANDITU_TILE_PATH_TEMPLATE, "vec"),
      minZoom: 0,
      maxZoom: basemap.maxZoom,
      attribution: basemap.attribution,
      labels: false,
    },
    unavailableSourceIds,
  );
  synchronizeRasterLayer(
    map,
    {
      sourceId: TIANDITU_LABEL_SOURCE_ID,
      layerId: TIANDITU_LABEL_LAYER_ID,
      template: tilePath(TIANDITU_TILE_PATH_TEMPLATE, "cva"),
      minZoom: 0,
      maxZoom: basemap.maxZoom,
      attribution: basemap.attribution,
      labels: true,
    },
    unavailableSourceIds,
  );
}

function synchronizeCartoBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo,
  unavailableSourceIds: ReadonlySet<string>,
): void {
  synchronizeRasterLayer(
    map,
    {
      sourceId: CARTO_BASE_SOURCE_ID,
      layerId: CARTO_BASE_LAYER_ID,
      template: tilePath(basemap.tilePathTemplate!, "base"),
      minZoom: 0,
      maxZoom: basemap.maxZoom,
      attribution: basemap.attribution,
      labels: false,
    },
    unavailableSourceIds,
  );
  synchronizeRasterLayer(
    map,
    {
      sourceId: CARTO_LABEL_SOURCE_ID,
      layerId: CARTO_LABEL_LAYER_ID,
      template: tilePath(basemap.tilePathTemplate!, "labels"),
      minZoom: 0,
      maxZoom: basemap.maxZoom,
      attribution: basemap.attribution,
      labels: true,
    },
    unavailableSourceIds,
  );
}

function removeRasterPair(
  map: MapLibreMap,
  sourceIds: readonly [string, string],
  layerIds: readonly [string, string],
): void {
  removeRasterLayerSource(map, sourceIds[1], layerIds[1]);
  removeRasterLayerSource(map, sourceIds[0], layerIds[0]);
}

function addOnlineRasterPair(
  map: MapLibreMap,
  basemap: OnlineBasemapInfo,
  base: { sourceId: string; layerId: string; template: string },
  labels: { sourceId: string; layerId: string; template: string },
  unavailableSourceIds: ReadonlySet<string>,
): void {
  synchronizeRasterLayer(
    map,
    {
      ...base,
      minZoom: basemap.minZoom,
      maxZoom: basemap.maxZoom,
      attribution: basemap.attribution,
      labels: false,
    },
    unavailableSourceIds,
  );
  synchronizeRasterLayer(
    map,
    {
      ...labels,
      minZoom: basemap.minZoom,
      maxZoom: basemap.maxZoom,
      attribution: basemap.attribution,
      labels: true,
    },
    unavailableSourceIds,
  );
}

function synchronizeOnlineBasemap(
  map: MapLibreMap,
  basemap: OnlineBasemapInfo,
  presentation: BasemapPresentation,
  unavailableSourceIds: ReadonlySet<string>,
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
      {
        sourceId: TIANDITU_IMAGERY_SOURCE_ID,
        layerId: TIANDITU_IMAGERY_LAYER_ID,
        template: basemap.imageryTemplate,
      },
      {
        sourceId: TIANDITU_IMAGERY_LABEL_SOURCE_ID,
        layerId: TIANDITU_IMAGERY_LABEL_LAYER_ID,
        template: basemap.imageryLabelTemplate,
      },
      unavailableSourceIds,
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
    {
      sourceId: TIANDITU_VECTOR_SOURCE_ID,
      layerId: TIANDITU_VECTOR_LAYER_ID,
      template: basemap.vectorTemplate,
    },
    {
      sourceId: TIANDITU_LABEL_SOURCE_ID,
      layerId: TIANDITU_LABEL_LAYER_ID,
      template: basemap.vectorLabelTemplate,
    },
    unavailableSourceIds,
  );
}

export function firstBasemapLabelLayerId(map: MapLibreMap): string | undefined {
  for (const layerId of [
    CARTO_LABEL_LAYER_ID,
    TIANDITU_LABEL_LAYER_ID,
    TIANDITU_IMAGERY_LABEL_LAYER_ID,
  ]) {
    if (map.getLayer(layerId)) return layerId;
  }
  return undefined;
}

const NO_UNAVAILABLE_SOURCES: ReadonlySet<string> = new Set();

export function synchronizeBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo | null | undefined,
  _theme: ResolvedTheme,
  presentation: BasemapPresentation = "map",
  onlineBasemap?: OnlineBasemapInfo | null,
  unavailableSourceIds: ReadonlySet<string> = NO_UNAVAILABLE_SOURCES,
): void {
  if (isTrustedOnlineBasemap(onlineBasemap)) {
    removeCartoBasemap(map);
    removeSatelliteBasemap(map);
    synchronizeOnlineBasemap(
      map,
      onlineBasemap,
      presentation,
      unavailableSourceIds,
    );
    return;
  }

  if (isTrustedTiandituBasemap(basemap)) {
    removeCartoBasemap(map);
    synchronizeTiandituBasemap(map, basemap, unavailableSourceIds);
    synchronizeSatelliteBasemap(
      map,
      basemap,
      presentation,
      unavailableSourceIds,
    );
    return;
  }
  if (isTrustedCartoBasemap(basemap)) {
    removeTiandituBasemap(map);
    synchronizeCartoBasemap(map, basemap, unavailableSourceIds);
    synchronizeSatelliteBasemap(
      map,
      basemap,
      presentation,
      unavailableSourceIds,
    );
    return;
  }

  removeTiandituBasemap(map);
  removeCartoBasemap(map);
  if (isTrustedSatelliteBasemap(basemap)) {
    synchronizeSatelliteBasemap(
      map,
      basemap!,
      presentation,
      unavailableSourceIds,
    );
    return;
  }
  removeSatelliteBasemap(map);
}
