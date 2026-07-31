import maplibregl, {
  type LayerSpecification,
  type Map as MapLibreMap,
} from "maplibre-gl";
import { Protocol } from "pmtiles";

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

export const PROTOMAPS_RESOURCE_PATH =
  "/api/basemap/pmtiles/four-provinces.pmtiles";
export const PROTOMAPS_SOURCE_ID = "basemap-protomaps";
export const PROTOMAPS_GEOMETRY_LAYER_IDS = [
  "basemap-protomaps-earth",
  "basemap-protomaps-landcover",
  "basemap-protomaps-landuse",
  "basemap-protomaps-water",
  "basemap-protomaps-roads",
] as const;
export const PROTOMAPS_LABEL_LAYER_IDS = [
  "basemap-protomaps-place-province",
  "basemap-protomaps-place-major-city",
  "basemap-protomaps-place-county",
  "basemap-protomaps-place-town",
] as const;
export const PROTOMAPS_LAYER_IDS = [
  ...PROTOMAPS_GEOMETRY_LAYER_IDS,
  ...PROTOMAPS_LABEL_LAYER_IDS,
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
  "places",
] as const;

const PROTOMAPS_THEME_PALETTES = {
  light: {
    earth: "#d9ddd7",
    landcover: "#c7d9c0",
    landuse: "#d9d2bd",
    water: "#9fc9d8",
    roads: "#8b8174",
    provinceText: "#53656c",
    placeText: "#24343b",
    secondaryPlaceText: "#42545b",
    placeHalo: "#f7f4eb",
  },
  dark: {
    earth: "#17242b",
    landcover: "#1f3a32",
    landuse: "#3b3327",
    water: "#123b4c",
    roads: "#7e898e",
    provinceText: "#b9d6d2",
    placeText: "#edf5f3",
    secondaryPlaceText: "#c5d4d2",
    placeHalo: "#101820",
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
    basemap?.enabled === true &&
    satellite?.enabled === true &&
    satellite.providerId === "eoxcloudless" &&
    satellite.mode === "same-origin-proxy" &&
    satellite.tilePathTemplate === SATELLITE_TILE_PATH_TEMPLATE &&
    satellite.maxZoom === 14 &&
    satellite.attribution === SATELLITE_ATTRIBUTION
  );
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
    PROTOMAPS_SOURCE_LAYERS.every((layerId, index) => layerIds[index] === layerId)
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

function removeProtomapsBasemap(map: MapLibreMap): void {
  for (const layerId of [...PROTOMAPS_LAYER_IDS].reverse()) {
    if (map.getLayer(layerId)) map.removeLayer(layerId);
  }
  if (map.getSource(PROTOMAPS_SOURCE_ID)) map.removeSource(PROTOMAPS_SOURCE_ID);
}
function removeSatelliteBasemap(map: MapLibreMap): void {
  if (map.getLayer(SATELLITE_LAYER_ID)) map.removeLayer(SATELLITE_LAYER_ID);
  if (map.getSource(SATELLITE_SOURCE_ID)) map.removeSource(SATELLITE_SOURCE_ID);
}

function setBaseGeometryVisibility(map: MapLibreMap, visible: boolean): void {
  const visibility = visible ? "visible" : "none";
  for (const layerId of [
    ...PROTOMAPS_GEOMETRY_LAYER_IDS,
    TIANDITU_VECTOR_LAYER_ID,
  ]) {
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
    {
      id: PROTOMAPS_LABEL_LAYER_IDS[0],
      type: "symbol",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "places",
      minzoom: 3.5,
      maxzoom: 8.5,
      filter: [
        "in",
        ["get", "kind_detail"],
        ["literal", ["state", "province"]],
      ],
      layout: {
        "text-field": [
          "coalesce",
          ["get", "name:zh-Hans"],
          ["get", "name"],
          ["get", "name:en"],
          "",
        ],
        "text-size": ["interpolate", ["linear"], ["zoom"], 3.5, 12, 7, 15],
        "symbol-sort-key": ["to-number", ["get", "sort_key"], 999999],
        "text-allow-overlap": false,
        "text-ignore-placement": false,
        "text-padding": 8,
      },
      paint: {
        "text-color": palette.provinceText,
        "text-halo-color": palette.placeHalo,
        "text-halo-width": 1.6,
        "text-halo-blur": 0.2,
      },
    },
    {
      id: PROTOMAPS_LABEL_LAYER_IDS[1],
      type: "symbol",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "places",
      minzoom: 3.5,
      filter: [
        "all",
        ["==", ["get", "kind_detail"], "city"],
        ["<=", ["to-number", ["get", "min_zoom"], 99], 7],
      ],
      layout: {
        "text-field": [
          "coalesce",
          ["get", "name:zh-Hans"],
          ["get", "name"],
          ["get", "name:en"],
          "",
        ],
        "text-size": ["interpolate", ["linear"], ["zoom"], 3.5, 12, 8, 14],
        "symbol-sort-key": ["to-number", ["get", "sort_key"], 999999],
        "text-variable-anchor": ["top", "bottom", "left", "right"],
        "text-radial-offset": 0.35,
        "text-allow-overlap": false,
        "text-ignore-placement": false,
        "text-padding": 5,
      },
      paint: {
        "text-color": palette.placeText,
        "text-halo-color": palette.placeHalo,
        "text-halo-width": 1.7,
        "text-halo-blur": 0.2,
      },
    },
    {
      id: PROTOMAPS_LABEL_LAYER_IDS[2],
      type: "symbol",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "places",
      minzoom: 7,
      filter: [
        "all",
        ["==", ["get", "kind_detail"], "city"],
        [">=", ["to-number", ["get", "min_zoom"], 99], 8],
        ["<=", ["to-number", ["get", "min_zoom"], 99], 9],
      ],
      layout: {
        "text-field": [
          "coalesce",
          ["get", "name:zh-Hans"],
          ["get", "name"],
          ["get", "name:en"],
          "",
        ],
        "text-size": ["interpolate", ["linear"], ["zoom"], 7, 10.5, 11, 12.5],
        "symbol-sort-key": ["to-number", ["get", "sort_key"], 999999],
        "text-variable-anchor": ["top", "bottom", "left", "right"],
        "text-radial-offset": 0.3,
        "text-allow-overlap": false,
        "text-ignore-placement": false,
        "text-padding": 4,
      },
      paint: {
        "text-color": palette.secondaryPlaceText,
        "text-halo-color": palette.placeHalo,
        "text-halo-width": 1.45,
        "text-halo-blur": 0.2,
      },
    },
    {
      id: PROTOMAPS_LABEL_LAYER_IDS[3],
      type: "symbol",
      source: PROTOMAPS_SOURCE_ID,
      "source-layer": "places",
      minzoom: 9.5,
      filter: ["==", ["get", "kind_detail"], "town"],
      layout: {
        "text-field": [
          "coalesce",
          ["get", "name:zh-Hans"],
          ["get", "name"],
          ["get", "name:en"],
          "",
        ],
        "text-size": ["interpolate", ["linear"], ["zoom"], 9.5, 10, 12, 11.5],
        "symbol-sort-key": ["to-number", ["get", "sort_key"], 999999],
        "text-variable-anchor": ["top", "bottom", "left", "right"],
        "text-radial-offset": 0.25,
        "text-allow-overlap": false,
        "text-ignore-placement": false,
        "text-padding": 3,
      },
      paint: {
        "text-color": palette.secondaryPlaceText,
        "text-halo-color": palette.placeHalo,
        "text-halo-width": 1.35,
        "text-halo-blur": 0.2,
      },
    },
  ];
}

export function applyProtomapsTheme(
  map: MapLibreMap,
  theme: ResolvedTheme,
  presentation: BasemapPresentation = "map",
): void {
  const palette = PROTOMAPS_THEME_PALETTES[theme];
  const geometryColors = [
    [PROTOMAPS_GEOMETRY_LAYER_IDS[0], "fill-color", palette.earth],
    [PROTOMAPS_GEOMETRY_LAYER_IDS[1], "fill-color", palette.landcover],
    [PROTOMAPS_GEOMETRY_LAYER_IDS[2], "fill-color", palette.landuse],
    [PROTOMAPS_GEOMETRY_LAYER_IDS[3], "fill-color", palette.water],
    [PROTOMAPS_GEOMETRY_LAYER_IDS[4], "line-color", palette.roads],
  ] as const;
  for (const [layerId, property, color] of geometryColors) {
    if (map.getLayer(layerId)) map.setPaintProperty(layerId, property, color);
  }
  const satellite = presentation === "satellite";
  const labelColors = [
    [PROTOMAPS_LABEL_LAYER_IDS[0], satellite ? "#f3fbff" : palette.provinceText],
    [PROTOMAPS_LABEL_LAYER_IDS[1], satellite ? "#ffffff" : palette.placeText],
    [PROTOMAPS_LABEL_LAYER_IDS[2], satellite ? "#f3f8f7" : palette.secondaryPlaceText],
    [PROTOMAPS_LABEL_LAYER_IDS[3], satellite ? "#eef6f4" : palette.secondaryPlaceText],
  ] as const;
  const haloColor = satellite ? "#101619" : palette.placeHalo;
  for (const [layerId, color] of labelColors) {
    if (!map.getLayer(layerId)) continue;
    map.setPaintProperty(layerId, "text-color", color);
    map.setPaintProperty(layerId, "text-halo-color", haloColor);
  }
}

function synchronizeProtomapsBasemap(
  map: MapLibreMap,
  basemap: TrustedProtomapsBasemap,
  theme: ResolvedTheme,
  presentation: BasemapPresentation,
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
    if (map.getLayer(layer.id)) continue;
    if (PROTOMAPS_LABEL_LAYER_IDS.some((layerId) => layerId === layer.id)) {
      map.addLayer(
        layer,
        firstTransmitterLayerId(map),
      );
    } else {
      addLayerBeforeGraticule(map, layer);
    }
  }
  applyProtomapsTheme(map, theme, presentation);
}

export function firstBasemapLabelLayerId(map: MapLibreMap): string | undefined {
  for (const layerId of PROTOMAPS_LABEL_LAYER_IDS) {
    if (map.getLayer(layerId)) return layerId;
  }
  for (const layerId of [TIANDITU_LABEL_LAYER_ID, TIANDITU_IMAGERY_LABEL_LAYER_ID]) {
    if (map.getLayer(layerId)) return layerId;
  }
  return undefined;
}

export function synchronizeBasemap(
  map: MapLibreMap,
  basemap: BasemapInfo | null | undefined,
  theme: ResolvedTheme,
  presentation: BasemapPresentation = "map",
  onlineBasemap?: OnlineBasemapInfo | null,
): void {
  if (isTrustedOnlineBasemap(onlineBasemap)) {
    removeSatelliteBasemap(map);
    removeProtomapsBasemap(map);
    synchronizeOnlineBasemap(map, onlineBasemap, presentation);
    return;
  }

  if (isTrustedProtomapsBasemap(basemap)) {
    removeTiandituBasemap(map);
    synchronizeProtomapsBasemap(map, basemap, theme, presentation);
    synchronizeSatelliteBasemap(map, basemap, presentation);
    return;
  }
  if (isTrustedTiandituBasemap(basemap)) {
    removeProtomapsBasemap(map);
    synchronizeTiandituBasemap(map, basemap);
    synchronizeSatelliteBasemap(map, basemap, presentation);
    return;
  }
  removeSatelliteBasemap(map);
  removeTiandituBasemap(map);
  removeProtomapsBasemap(map);
}
