import type { Map as MapLibreMap } from "maplibre-gl";

import type { BasemapInfo } from "./types";

export const TIANDITU_TILE_PATH_TEMPLATE =
  "/api/basemap/tianditu/{layer}/{z}/{x}/{y}";
export const TIANDITU_VECTOR_SOURCE_ID = "basemap-tianditu-vector";
export const TIANDITU_LABEL_SOURCE_ID = "basemap-tianditu-label";
export const TIANDITU_VECTOR_LAYER_ID = "basemap-tianditu-vector-layer";
export const TIANDITU_LABEL_LAYER_ID = "basemap-tianditu-label-layer";

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

function tilePath(layer: "vec" | "cva"): string {
  return TIANDITU_TILE_PATH_TEMPLATE.replace("{layer}", layer);
}

function removeBasemap(map: MapLibreMap): void {
  for (const layerId of [TIANDITU_LABEL_LAYER_ID, TIANDITU_VECTOR_LAYER_ID]) {
    if (map.getLayer(layerId)) map.removeLayer(layerId);
  }
  for (const sourceId of [TIANDITU_LABEL_SOURCE_ID, TIANDITU_VECTOR_SOURCE_ID]) {
    if (map.getSource(sourceId)) map.removeSource(sourceId);
  }
}

export function synchronizeBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo | null | undefined,
): void {
  if (!isTrustedTiandituBasemap(basemap)) {
    removeBasemap(map);
    return;
  }

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
    map.addLayer(
      {
        id: TIANDITU_VECTOR_LAYER_ID,
        type: "raster",
        source: TIANDITU_VECTOR_SOURCE_ID,
        paint: { "raster-fade-duration": 0 },
      },
      map.getLayer("graticule-lines") ? "graticule-lines" : undefined,
    );
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
      map.getLayer("selected-point-halo") ? "selected-point-halo" : undefined,
    );
  }
}
