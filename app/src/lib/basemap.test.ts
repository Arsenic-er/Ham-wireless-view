import { describe, expect, it, vi } from "vitest";

import {
  PROTOMAPS_GEOMETRY_LAYER_IDS,
  PROTOMAPS_LABEL_LAYER_IDS,
  PROTOMAPS_LAYER_IDS,
  PROTOMAPS_RESOURCE_PATH,
  PROTOMAPS_SOURCE_ID,
  SATELLITE_ATTRIBUTION,
  SATELLITE_LAYER_ID,
  SATELLITE_SOURCE_ID,
  SATELLITE_TILE_PATH_TEMPLATE,
  TAURI_TIANDITU_IMAGERY_LABEL_TEMPLATE,
  TAURI_TIANDITU_IMAGERY_TEMPLATE,
  TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE,
  TAURI_TIANDITU_VECTOR_TEMPLATE,
  TIANDITU_IMAGERY_LABEL_LAYER_ID,
  TIANDITU_IMAGERY_LABEL_SOURCE_ID,
  TIANDITU_IMAGERY_LAYER_ID,
  TIANDITU_IMAGERY_SOURCE_ID,
  TIANDITU_LABEL_LAYER_ID,
  TIANDITU_LABEL_SOURCE_ID,
  TIANDITU_TILE_PATH_TEMPLATE,
  TIANDITU_VECTOR_LAYER_ID,
  TIANDITU_VECTOR_SOURCE_ID,
  firstBasemapLabelLayerId,
  isTrustedProtomapsBasemap,
  isTrustedOnlineBasemap,
  isTrustedSatelliteBasemap,
  isTrustedTiandituBasemap,
  synchronizeBasemap,
} from "./basemap";
import type { BasemapInfo, OnlineBasemapInfo } from "./types";

const configuredSatellite = {
  enabled: true,
  providerId: "eoxcloudless",
  displayName: "Sentinel-2 2025",
  attribution: SATELLITE_ATTRIBUTION,
  mode: "same-origin-proxy",
  maxZoom: 14,
  tilePathTemplate: SATELLITE_TILE_PATH_TEMPLATE,
};

const configuredTianditu: BasemapInfo = {
  enabled: true,
  providerId: "tianditu",
  displayName: "天地图",
  attribution: "天地图",
  mode: "same-origin-proxy",
  maxZoom: 18,
  layers: [
    { id: "vec", displayName: "矢量底图" },
    { id: "cva", displayName: "中文注记" },
  ],
  tilePathTemplate: TIANDITU_TILE_PATH_TEMPLATE,
  satellite: configuredSatellite,
};

const configuredProtomaps: BasemapInfo = {
  enabled: true,
  providerId: "protomaps",
  displayName: "四省区域底图",
  attribution: "© OpenStreetMap contributors",
  mode: "same-origin-pmtiles",
  maxZoom: 9,
  layers: [
    { id: "earth", displayName: "陆地" },
    { id: "landcover", displayName: "地表覆盖" },
    { id: "landuse", displayName: "土地利用" },
    { id: "water", displayName: "水体" },
    { id: "roads", displayName: "道路" },
    { id: "places", displayName: "地名" },
  ],
  resourcePath: PROTOMAPS_RESOURCE_PATH,
  bounds: [107.5, 18, 125.5, 33.5],
  archiveBytes: 33_044_072,
  satellite: configuredSatellite,
};


const configuredOnlineBasemap: OnlineBasemapInfo = {
  configured: true,
  provider: "Tianditu",
  protocolScheme: "tianditu",
  vectorTemplate: TAURI_TIANDITU_VECTOR_TEMPLATE,
  vectorLabelTemplate: TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE,
  imageryTemplate: TAURI_TIANDITU_IMAGERY_TEMPLATE,
  imageryLabelTemplate: TAURI_TIANDITU_IMAGERY_LABEL_TEMPLATE,
  attribution: "天地图",
  minZoom: 1,
  maxZoom: 18,
};
function mapDouble() {
  const sources = new Set<string>();
  const layers = new Set<string>(["graticule-lines", "completed-point-halo", "selected-point-halo"]);
  const map = {
    addSource: vi.fn((id: string) => sources.add(id)),
    getSource: vi.fn((id: string) => (sources.has(id) ? { id } : undefined)),
    removeSource: vi.fn((id: string) => sources.delete(id)),
    addLayer: vi.fn(
      (layer: { id: string; "source-layer"?: string }, _before?: string) =>
        layers.add(layer.id),
    ),
    getLayer: vi.fn((id: string) => (layers.has(id) ? { id } : undefined)),
    removeLayer: vi.fn((id: string) => layers.delete(id)),
    setPaintProperty: vi.fn(),
    setLayoutProperty: vi.fn(),
  };
  return map;
}

describe("trusted basemap contracts", () => {
  it("keeps the TianDiTu same-origin raster contract", () => {
    expect(isTrustedTiandituBasemap(configuredTianditu)).toBe(true);
    expect(
      isTrustedTiandituBasemap({
        ...configuredTianditu,
        tilePathTemplate: "https://example.invalid/{z}/{x}/{y}",
      }),
    ).toBe(false);
    expect(
      isTrustedTiandituBasemap({
        ...configuredTianditu,
        layers: [{ id: "vec", displayName: "矢量底图" }],
      }),
    ).toBe(false);
  });

  it("accepts only the fixed same-origin satellite contract", () => {
    expect(isTrustedSatelliteBasemap(configuredProtomaps)).toBe(true);
    expect(
      isTrustedSatelliteBasemap({
        ...configuredProtomaps,
        satellite: { ...configuredSatellite, maxZoom: 15 },
      }),
    ).toBe(false);
  });

  it("accepts only the fixed local PMTiles archive and safe source layers", () => {
    expect(isTrustedProtomapsBasemap(configuredProtomaps)).toBe(true);
    for (const untrusted of [
      { ...configuredProtomaps, resourcePath: "/assets/other.pmtiles" },
      { ...configuredProtomaps, maxZoom: 10 },
      { ...configuredProtomaps, archiveBytes: 33_044_071 },
      { ...configuredProtomaps, bounds: [107.5, 18, 125.5, 34] },
      {
        ...configuredProtomaps,
        layers: [...configuredProtomaps.layers].reverse(),
      },
      {
        ...configuredProtomaps,
        layers: [
          ...configuredProtomaps.layers.slice(0, -1),
          { id: "boundaries" as never, displayName: "边界" },
        ],
      },
      {
        ...configuredProtomaps,
        layers: [...configuredProtomaps.layers, { id: "pois" as never, displayName: "兴趣点" }],
      },
    ]) {
      expect(isTrustedProtomapsBasemap(untrusted as BasemapInfo)).toBe(false);
    }
  });

  it("adds approved geometry plus scalable local place labels", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, configuredProtomaps, "light");

    expect(map.addSource).toHaveBeenCalledWith(
      PROTOMAPS_SOURCE_ID,
      expect.objectContaining({
        type: "vector",
        url: expect.stringMatching(
          /^pmtiles:\/\/http:\/\/localhost:3000\/api\/basemap\/pmtiles\/four-provinces\.pmtiles$/,
        ),
        maxzoom: 9,
      }),
    );
    const styleLayers = map.addLayer.mock.calls.map(([layer]) => layer);
    expect(styleLayers.map(({ id }) => id)).toEqual(PROTOMAPS_LAYER_IDS);
    expect(styleLayers.map((layer) => layer["source-layer"])).toEqual([
      "earth",
      "landcover",
      "landuse",
      "water",
      "roads",
      "places",
      "places",
      "places",
      "places",
    ]);
    const labelStyleJson = JSON.stringify(styleLayers.slice(5));
    expect(labelStyleJson).toContain('"name:zh-Hans"');
    expect(labelStyleJson).toContain('"name:en"');
    expect(labelStyleJson.indexOf('"name:zh-Hans"')).toBeLessThan(
      labelStyleJson.indexOf('"name"'),
    );
    expect(labelStyleJson.indexOf('"name"')).toBeLessThan(
      labelStyleJson.indexOf('"name:en"'),
    );
    expect(JSON.stringify(styleLayers)).not.toMatch(/boundaries|pois|buildings/);
    expect(
      map.addLayer.mock.calls
        .slice(0, 5)
        .every(([, before]) => before === "graticule-lines"),
    ).toBe(true);
    expect(
      map.addLayer.mock.calls
        .slice(5)
        .every(([, before]) => before === "completed-point-halo"),
    ).toBe(true);
    expect(firstBasemapLabelLayerId(map as never)).toBe(
      PROTOMAPS_LABEL_LAYER_IDS[0],
    );

    map.setPaintProperty.mockClear();
    synchronizeBasemap(map as never, configuredProtomaps, "dark");
    expect(map.addSource.mock.calls.filter(([id]) => id === PROTOMAPS_SOURCE_ID)).toHaveLength(1);
    expect(map.addLayer).toHaveBeenCalledTimes(PROTOMAPS_LAYER_IDS.length);
    expect(map.setPaintProperty.mock.calls).toEqual(
      expect.arrayContaining([
        [PROTOMAPS_LAYER_IDS[0], "fill-color", "#17242b"],
        [PROTOMAPS_LAYER_IDS[1], "fill-color", "#1f3a32"],
        [PROTOMAPS_LAYER_IDS[2], "fill-color", "#3b3327"],
        [PROTOMAPS_LAYER_IDS[3], "fill-color", "#123b4c"],
        [PROTOMAPS_LAYER_IDS[4], "line-color", "#7e898e"],
        [PROTOMAPS_LABEL_LAYER_IDS[1], "text-color", "#edf5f3"],
        [PROTOMAPS_LABEL_LAYER_IDS[1], "text-halo-color", "#101820"],
      ]),
    );

    synchronizeBasemap(map as never, configuredTianditu, "light");
    for (const layerId of [...PROTOMAPS_LAYER_IDS].reverse()) {
      expect(map.removeLayer).toHaveBeenCalledWith(layerId);
    }
    expect(map.removeSource).toHaveBeenCalledWith(PROTOMAPS_SOURCE_ID);
    expect(firstBasemapLabelLayerId(map as never)).toBe(TIANDITU_LABEL_LAYER_ID);
  });

  it("switches to online satellite imagery while retaining local place labels", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, configuredProtomaps, "dark", "satellite");

    expect(map.addSource).toHaveBeenCalledWith(
      SATELLITE_SOURCE_ID,
      expect.objectContaining({
        type: "raster",
        tiles: [SATELLITE_TILE_PATH_TEMPLATE],
        maxzoom: 14,
        attribution: SATELLITE_ATTRIBUTION,
      }),
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: SATELLITE_LAYER_ID }),
      "graticule-lines",
    );
    for (const layerId of PROTOMAPS_GEOMETRY_LAYER_IDS) {
      expect(map.setLayoutProperty).toHaveBeenCalledWith(
        layerId,
        "visibility",
        "none",
      );
    }
    expect(firstBasemapLabelLayerId(map as never)).toBe(
      PROTOMAPS_LABEL_LAYER_IDS[0],
    );

    synchronizeBasemap(map as never, configuredProtomaps, "dark", "map");
    expect(map.removeLayer).toHaveBeenCalledWith(SATELLITE_LAYER_ID);
    expect(map.removeSource).toHaveBeenCalledWith(SATELLITE_SOURCE_ID);
  });

  it("adds and removes both TianDiTu raster layers through fixed same-origin paths", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, configuredTianditu, "light");

    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_VECTOR_SOURCE_ID,
      expect.objectContaining({
        tiles: ["/api/basemap/tianditu/vec/{z}/{x}/{y}"],
        maxzoom: 18,
      }),
    );
    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_LABEL_SOURCE_ID,
      expect.objectContaining({
        tiles: ["/api/basemap/tianditu/cva/{z}/{x}/{y}"],
        maxzoom: 18,
      }),
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: TIANDITU_VECTOR_LAYER_ID }),
      "graticule-lines",
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: TIANDITU_LABEL_LAYER_ID }),
      "completed-point-halo",
    );

    synchronizeBasemap(map as never, { ...configuredTianditu, enabled: false }, "light");
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_LABEL_LAYER_ID);
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_VECTOR_LAYER_ID);
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_LABEL_SOURCE_ID);
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_VECTOR_SOURCE_ID);
  });

  it("accepts only the fixed Tauri TianDiTu custom-protocol metadata", () => {
    expect(isTrustedOnlineBasemap(configuredOnlineBasemap)).toBe(true);
    for (const untrusted of [
      { ...configuredOnlineBasemap, configured: false },
      { ...configuredOnlineBasemap, provider: "Other" },
      { ...configuredOnlineBasemap, protocolScheme: "https" },
      { ...configuredOnlineBasemap, vectorTemplate: "https://example.invalid/{z}/{x}/{y}" },
      { ...configuredOnlineBasemap, vectorLabelTemplate: TAURI_TIANDITU_VECTOR_TEMPLATE },
      { ...configuredOnlineBasemap, imageryTemplate: TAURI_TIANDITU_VECTOR_TEMPLATE },
      { ...configuredOnlineBasemap, imageryLabelTemplate: TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE },
      { ...configuredOnlineBasemap, minZoom: 0 },
      { ...configuredOnlineBasemap, maxZoom: 19 },
    ]) {
      expect(isTrustedOnlineBasemap(untrusted as OnlineBasemapInfo)).toBe(false);
    }
  });

  it("switches fixed desktop vector and imagery pairs with labels above heatmaps and below transmitters", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, null, "light", "map", configuredOnlineBasemap);

    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_VECTOR_SOURCE_ID,
      expect.objectContaining({
        tiles: [TAURI_TIANDITU_VECTOR_TEMPLATE],
        minzoom: 1,
        maxzoom: 18,
      }),
    );
    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_LABEL_SOURCE_ID,
      expect.objectContaining({ tiles: [TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE] }),
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: TIANDITU_VECTOR_LAYER_ID }),
      "graticule-lines",
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: TIANDITU_LABEL_LAYER_ID }),
      "completed-point-halo",
    );
    expect(firstBasemapLabelLayerId(map as never)).toBe(TIANDITU_LABEL_LAYER_ID);

    synchronizeBasemap(map as never, null, "dark", "satellite", configuredOnlineBasemap);
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_LABEL_LAYER_ID);
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_VECTOR_LAYER_ID);
    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_IMAGERY_SOURCE_ID,
      expect.objectContaining({ tiles: [TAURI_TIANDITU_IMAGERY_TEMPLATE] }),
    );
    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_IMAGERY_LABEL_SOURCE_ID,
      expect.objectContaining({ tiles: [TAURI_TIANDITU_IMAGERY_LABEL_TEMPLATE] }),
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: TIANDITU_IMAGERY_LAYER_ID }),
      "graticule-lines",
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: TIANDITU_IMAGERY_LABEL_LAYER_ID }),
      "completed-point-halo",
    );
    expect(firstBasemapLabelLayerId(map as never)).toBe(
      TIANDITU_IMAGERY_LABEL_LAYER_ID,
    );

    synchronizeBasemap(
      map as never,
      null,
      "dark",
      "satellite",
      { ...configuredOnlineBasemap, imageryTemplate: "https://example.invalid" },
    );
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_IMAGERY_SOURCE_ID);
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_IMAGERY_LABEL_SOURCE_ID);
  });
});
