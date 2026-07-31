import maplibregl, {
  type LayerSpecification,
  type Map as MapLibreMap,
} from "maplibre-gl";
import { Protocol } from "pmtiles";

import type { BasemapInfo, ResolvedTheme } from "./types";

export const TIANDITU_TILE_PATH_TEMPLATE =
  "/api/basemap/tianditu/{layer}/{z}/{x}/{y}";
export const TIANDITU_VECTOR_SOURCE_ID = "basemap-tianditu-vector";
export const TIANDITU_LABEL_SOURCE_ID = "basemap-tianditu-label";
export const TIANDITU_VECTOR_LAYER_ID = "basemap-tianditu-vector-layer";
export const TIANDITU_LABEL_LAYER_ID = "basemap-tianditu-label-layer";

export const PROTOMAPS_RESOURCE_PATH =
  "/api/basemap/pmtiles/four-provinces.pmtiles";
export const PROTOMAPS_SOURCE_ID = "basemap-protomaps";
export const PROTOMAPS_LAYER_IDS = [
  "basemap-protomaps-earth",
  "basemap-protomaps-landcover",
  "basemap-protomaps-landuse",
  "basemap-protomaps-water",
  "basemap-protomaps-roads",
] as const;

const PROTOMAPS_MAX_ZOOM = 9;
const PROTOMAPS_ARCHIVE_BYTES = 33_044_072;
const PROTOMAPS_BOUNDS = [107.5, 18, 125.5, 33.5] as const;
const PROTOMAPS_ATTRIBUTION = "© OpenStreetMap contributors";
const PROTOMAPS_SOURCE_LAYERS = [
  "earth",
  "landcover",
  "landuse",
  "water",
  "roads",
] as const;

const PROTOMAPS_THEME_PALETTES = {
  light: {
    earth: "#d9ddd7",
    landcover: "#c7d9c0",
    landuse: "#d9d2bd",
    water: "#9fc9d8",
    roads: "#8b8174",
  },
  dark: {
    earth: "#17242b",
    landcover: "#1f3a32",
    landuse: "#3b3327",
    water: "#123b4c",
    roads: "#7e898e",
  },
} as const;

type TrustedProtomapsBasemap = BasemapInfo &
  Required<
    Pick<BasemapInfo, "resourcePath" | "bounds" | "archiveBytes">
  >;

let pmtilesProtocol: Protocol | null = null;
let pmtilesProtocolUsers = 0;

export function acquirePmtilesProtocol(): () => void {
  if (!pmtilesProtocol) {
    pmtilesProtocol = new Protocol();
    maplibregl.addProtocol("pmtiles", pmtilesProtocol.tile);
  }
  pmtilesProtocolUsers += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    pmtilesProtocolUsers -= 1;
    if (pmtilesProtocolUsers === 0) {
      maplibregl.removeProtocol("pmtiles");
      pmtilesProtocol = null;
    }
  };
}

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

export function isTrustedProtomapsBasemap(
  basemap: BasemapInfo | null | undefined,
): basemap is TrustedProtomapsBasemap {
  if (
    !basemap?.enabled ||
    basemap.providerId !== "protomaps" ||
    basemap.mode !== "same-origin-pmtiles" ||
    basemap.resourcePath !== PROTOMAPS_RESOURCE_PATH ||
    basemap.maxZoom !== PROTOMAPS_MAX_ZOOM ||
    basemap.archiveBytes !== PROTOMAPS_ARCHIVE_BYTES ||
    basemap.bounds?.length !== PROTOMAPS_BOUNDS.length ||
    !basemap.bounds.every((value, index) => value === PROTOMAPS_BOUNDS[index]) ||
    basemap.attribution !== PROTOMAPS_ATTRIBUTION
  ) {
    return false;
  }
  const layerIds = basemap.layers.map(({ id }) => id);
  return (
    layerIds.length === PROTOMAPS_SOURCE_LAYERS.length &&
    new Set(layerIds).size === PROTOMAPS_SOURCE_LAYERS.length &&
    PROTOMAPS_SOURCE_LAYERS.every((layerId) => layerIds.includes(layerId))
  );
}

export function isTrustedBasemap(
  basemap: BasemapInfo | null | undefined,
): basemap is BasemapInfo {
  return isTrustedProtomapsBasemap(basemap) || isTrustedTiandituBasemap(basemap);
}

function tilePath(layer: "vec" | "cva"): string {
  return TIANDITU_TILE_PATH_TEMPLATE.replace("{layer}", layer);
}

function removeTiandituBasemap(map: MapLibreMap): void {
  for (const layerId of [TIANDITU_LABEL_LAYER_ID, TIANDITU_VECTOR_LAYER_ID]) {
    if (map.getLayer(layerId)) map.removeLayer(layerId);
  }
  for (const sourceId of [TIANDITU_LABEL_SOURCE_ID, TIANDITU_VECTOR_SOURCE_ID]) {
    if (map.getSource(sourceId)) map.removeSource(sourceId);
  }
}

function removeProtomapsBasemap(map: MapLibreMap): void {
  for (const layerId of [...PROTOMAPS_LAYER_IDS].reverse()) {
    if (map.getLayer(layerId)) map.removeLayer(layerId);
  }
  if (map.getSource(PROTOMAPS_SOURCE_ID)) map.removeSource(PROTOMAPS_SOURCE_ID);
}

function addLayerBeforeGraticule(map: MapLibreMap, layer: LayerSpecification): void {
  map.addLayer(layer, map.getLayer("graticule-lines") ? "graticule-lines" : undefined);
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
      map.getLayer("selected-point-halo") ? "selected-point-halo" : undefined,
    );
  }
}

function protomapsLayers(theme: ResolvedTheme): LayerSpecification[] {
  const palette = PROTOMAPS_THEME_PALETTES[theme];
  return [
    {
      id: PROTOMAPS_LAYER_IDS[0],
      type: "fill",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "earth",
      paint: { "fill-color": palette.earth, "fill-opacity": 1 },
    },
    {
      id: PROTOMAPS_LAYER_IDS[1],
      type: "fill",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "landcover",
      paint: { "fill-color": palette.landcover, "fill-opacity": 0.72 },
    },
    {
      id: PROTOMAPS_LAYER_IDS[2],
      type: "fill",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "landuse",
      paint: { "fill-color": palette.landuse, "fill-opacity": 0.48 },
    },
    {
      id: PROTOMAPS_LAYER_IDS[3],
      type: "fill",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "water",
      paint: { "fill-color": palette.water, "fill-opacity": 0.9 },
    },
    {
      id: PROTOMAPS_LAYER_IDS[4],
      type: "line",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "roads",
      minzoom: 4,
      paint: {
        "line-color": palette.roads,
        "line-opacity": 0.72,
        "line-width": ["interpolate", ["linear"], ["zoom"], 4, 0.35, 9, 1.5],
      },
    },
  ];
}

export function applyProtomapsTheme(map: MapLibreMap, theme: ResolvedTheme): void {
  const palette = PROTOMAPS_THEME_PALETTES[theme];
  const colors = [
    [PROTOMAPS_LAYER_IDS[0], "fill-color", palette.earth],
    [PROTOMAPS_LAYER_IDS[1], "fill-color", palette.landcover],
    [PROTOMAPS_LAYER_IDS[2], "fill-color", palette.landuse],
    [PROTOMAPS_LAYER_IDS[3], "fill-color", palette.water],
    [PROTOMAPS_LAYER_IDS[4], "line-color", palette.roads],
  ] as const;
  for (const [layerId, property, color] of colors) {
    if (map.getLayer(layerId)) map.setPaintProperty(layerId, property, color);
  }
}

function synchronizeProtomapsBasemap(
  map: MapLibreMap,
  basemap: TrustedProtomapsBasemap,
  theme: ResolvedTheme,
): void {
  if (!map.getSource(PROTOMAPS_SOURCE_ID)) {
    const archiveUrl = new URL(basemap.resourcePath, window.location.href).href;
    map.addSource(PROTOMAPS_SOURCE_ID, {
      type: "vector",
      url: `pmtiles://${archiveUrl}`,
      attribution: basemap.attribution,
      minzoom: 0,
      maxzoom: basemap.maxZoom,
    });
  }

  for (const layer of protomapsLayers(theme)) {
    if (!map.getLayer(layer.id)) addLayerBeforeGraticule(map, layer);
  }
  applyProtomapsTheme(map, theme);
}

export function firstBasemapLabelLayerId(map: MapLibreMap): string | undefined {
  return map.getLayer(TIANDITU_LABEL_LAYER_ID) ? TIANDITU_LABEL_LAYER_ID : undefined;
}

export function synchronizeBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo | null | undefined,
  theme: ResolvedTheme,
): void {
  if (isTrustedProtomapsBasemap(basemap)) {
    removeTiandituBasemap(map);
    synchronizeProtomapsBasemap(map, basemap, theme);
    return;
  }
  if (isTrustedTiandituBasemap(basemap)) {
    removeProtomapsBasemap(map);
    synchronizeTiandituBasemap(map, basemap);
    return;
  }
  removeTiandituBasemap(map);
  removeProtomapsBasemap(map);
}
