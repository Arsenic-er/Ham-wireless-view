import { describe, expect, it, vi } from "vitest";

import {
  PROTOMAPS_LAYER_IDS,
  PROTOMAPS_RESOURCE_PATH,
  PROTOMAPS_SOURCE_ID,
  TIANDITU_LABEL_LAYER_ID,
  TIANDITU_LABEL_SOURCE_ID,
  TIANDITU_TILE_PATH_TEMPLATE,
  TIANDITU_VECTOR_LAYER_ID,
  TIANDITU_VECTOR_SOURCE_ID,
  firstBasemapLabelLayerId,
  isTrustedProtomapsBasemap,
  isTrustedTiandituBasemap,
  synchronizeBasemap,
} from "./basemap";
import type { BasemapInfo } from "./types";

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
  ],
  resourcePath: PROTOMAPS_RESOURCE_PATH,
  bounds: [107.5, 18, 125.5, 33.5],
  archiveBytes: 33_044_072,
};

function mapDouble() {
  const sources = new Set<string>();
  const layers = new Set<string>(["graticule-lines", "selected-point-halo"]);
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

  it("accepts only the fixed local PMTiles archive and safe source layers", () => {
    expect(isTrustedProtomapsBasemap(configuredProtomaps)).toBe(true);
    for (const untrusted of [
      { ...configuredProtomaps, resourcePath: "/assets/other.pmtiles" },
      { ...configuredProtomaps, maxZoom: 10 },
      { ...configuredProtomaps, archiveBytes: 33_044_071 },
      { ...configuredProtomaps, bounds: [107.5, 18, 125.5, 34] },
      {
        ...configuredProtomaps,
        layers: [
          ...configuredProtomaps.layers.slice(0, -1),
          { id: "boundaries" as never, displayName: "边界" },
        ],
      },
      {
        ...configuredProtomaps,
        layers: [...configuredProtomaps.layers, { id: "places" as never, displayName: "地名" }],
      },
    ]) {
      expect(isTrustedProtomapsBasemap(untrusted as BasemapInfo)).toBe(false);
    }
  });

  it("adds the local vector source and only the five approved non-label layers", () => {
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
    ]);
    expect(JSON.stringify(styleLayers)).not.toMatch(/boundaries|places|pois|buildings/);
    expect(map.addLayer.mock.calls.every(([, before]) => before === "graticule-lines")).toBe(true);
    expect(firstBasemapLabelLayerId(map as never)).toBeUndefined();

    map.setPaintProperty.mockClear();
    synchronizeBasemap(map as never, configuredProtomaps, "dark");
    expect(map.addSource.mock.calls.filter(([id]) => id === PROTOMAPS_SOURCE_ID)).toHaveLength(1);
    expect(map.addLayer).toHaveBeenCalledTimes(PROTOMAPS_LAYER_IDS.length);
    expect(map.setPaintProperty.mock.calls).toEqual([
      [PROTOMAPS_LAYER_IDS[0], "fill-color", "#17242b"],
      [PROTOMAPS_LAYER_IDS[1], "fill-color", "#1f3a32"],
      [PROTOMAPS_LAYER_IDS[2], "fill-color", "#3b3327"],
      [PROTOMAPS_LAYER_IDS[3], "fill-color", "#123b4c"],
      [PROTOMAPS_LAYER_IDS[4], "line-color", "#7e898e"],
    ]);

    synchronizeBasemap(map as never, configuredTianditu, "light");
    for (const layerId of [...PROTOMAPS_LAYER_IDS].reverse()) {
      expect(map.removeLayer).toHaveBeenCalledWith(layerId);
    }
    expect(map.removeSource).toHaveBeenCalledWith(PROTOMAPS_SOURCE_ID);
    expect(firstBasemapLabelLayerId(map as never)).toBe(TIANDITU_LABEL_LAYER_ID);
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
      "selected-point-halo",
    );

    synchronizeBasemap(map as never, { ...configuredTianditu, enabled: false }, "light");
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_LABEL_LAYER_ID);
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_VECTOR_LAYER_ID);
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_LABEL_SOURCE_ID);
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_VECTOR_SOURCE_ID);
  });
});
