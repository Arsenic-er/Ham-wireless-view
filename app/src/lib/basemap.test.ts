import { describe, expect, it, vi } from "vitest";

import {
  TIANDITU_LABEL_LAYER_ID,
  TIANDITU_LABEL_SOURCE_ID,
  TIANDITU_TILE_PATH_TEMPLATE,
  TIANDITU_VECTOR_LAYER_ID,
  TIANDITU_VECTOR_SOURCE_ID,
  isTrustedTiandituBasemap,
  synchronizeBasemap,
} from "./basemap";
import type { BasemapInfo } from "./types";

const configuredBasemap: BasemapInfo = {
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

function mapDouble() {
  const sources = new Set<string>();
  const layers = new Set<string>(["graticule-lines", "selected-point-halo"]);
  const map = {
    addSource: vi.fn((id: string) => sources.add(id)),
    getSource: vi.fn((id: string) => (sources.has(id) ? { id } : undefined)),
    removeSource: vi.fn((id: string) => sources.delete(id)),
    addLayer: vi.fn((layer: { id: string }) => layers.add(layer.id)),
    getLayer: vi.fn((id: string) => (layers.has(id) ? { id } : undefined)),
    removeLayer: vi.fn((id: string) => layers.delete(id)),
  };
  return map;
}

describe("same-origin TianDiTu basemap contract", () => {
  it("fails closed for untrusted metadata", () => {
    expect(isTrustedTiandituBasemap(configuredBasemap)).toBe(true);
    expect(
      isTrustedTiandituBasemap({
        ...configuredBasemap,
        tilePathTemplate: "https://example.invalid/{z}/{x}/{y}",
      }),
    ).toBe(false);
    expect(
      isTrustedTiandituBasemap({
        ...configuredBasemap,
        layers: [{ id: "vec", displayName: "矢量底图" }],
      }),
    ).toBe(false);
  });

  it("adds both real-map layers through fixed same-origin paths and removes them", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, configuredBasemap);

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

    synchronizeBasemap(map as never, { ...configuredBasemap, enabled: false });
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_LABEL_LAYER_ID);
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_VECTOR_LAYER_ID);
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_LABEL_SOURCE_ID);
    expect(map.removeSource).toHaveBeenCalledWith(TIANDITU_VECTOR_SOURCE_ID);
  });
});
