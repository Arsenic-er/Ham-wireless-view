import { describe, expect, it, vi } from "vitest";

import {
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
  const layers = new Set<string>([
    "graticule-lines",
    "coverage-heatmap-layer-example",
    "completed-point-halo",
    "selected-point-halo",
  ]);
  return {
    addSource: vi.fn((id: string) => sources.add(id)),
    getSource: vi.fn((id: string) => (sources.has(id) ? { id } : undefined)),
    removeSource: vi.fn((id: string) => sources.delete(id)),
    addLayer: vi.fn((layer: { id: string }) => layers.add(layer.id)),
    getLayer: vi.fn((id: string) => (layers.has(id) ? { id } : undefined)),
    removeLayer: vi.fn((id: string) => layers.delete(id)),
    setPaintProperty: vi.fn(),
    setLayoutProperty: vi.fn(),
  };
}

describe("trusted online basemap contracts", () => {
  it("accepts only the fixed same-origin TianDiTu and satellite templates", () => {
    expect(isTrustedTiandituBasemap(configuredTianditu)).toBe(true);
    expect(isTrustedSatelliteBasemap(configuredTianditu)).toBe(true);
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
    expect(
      isTrustedSatelliteBasemap({
        ...configuredTianditu,
        satellite: { ...configuredSatellite, tilePathTemplate: "https://example.invalid" },
      }),
    ).toBe(false);
  });

  it("switches same-origin vec/cva and EOX satellite while retaining labels", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, configuredTianditu, "light", "map");

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
    expect(firstBasemapLabelLayerId(map as never)).toBe(TIANDITU_LABEL_LAYER_ID);

    synchronizeBasemap(map as never, configuredTianditu, "dark", "satellite");
    expect(map.addSource).toHaveBeenCalledWith(
      SATELLITE_SOURCE_ID,
      expect.objectContaining({
        tiles: [SATELLITE_TILE_PATH_TEMPLATE],
        maxzoom: 14,
        attribution: SATELLITE_ATTRIBUTION,
      }),
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: SATELLITE_LAYER_ID }),
      "graticule-lines",
    );
    expect(map.setLayoutProperty).toHaveBeenCalledWith(
      TIANDITU_VECTOR_LAYER_ID,
      "visibility",
      "none",
    );
    expect(firstBasemapLabelLayerId(map as never)).toBe(TIANDITU_LABEL_LAYER_ID);

    synchronizeBasemap(map as never, configuredTianditu, "dark", "map");
    expect(map.removeLayer).toHaveBeenCalledWith(SATELLITE_LAYER_ID);
    expect(map.removeSource).toHaveBeenCalledWith(SATELLITE_SOURCE_ID);
    expect(map.setLayoutProperty).toHaveBeenCalledWith(
      TIANDITU_VECTOR_LAYER_ID,
      "visibility",
      "visible",
    );
  });

  it("removes every online layer when metadata is no longer trusted", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, configuredTianditu, "light", "satellite");
    synchronizeBasemap(map as never, { ...configuredTianditu, enabled: false }, "light");

    expect(map.removeLayer).toHaveBeenCalledWith(SATELLITE_LAYER_ID);
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_LABEL_LAYER_ID);
    expect(map.removeLayer).toHaveBeenCalledWith(TIANDITU_VECTOR_LAYER_ID);
    expect(map.removeSource).toHaveBeenCalledWith(SATELLITE_SOURCE_ID);
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

  it("switches fixed desktop vector and imagery pairs with labels below transmitters", () => {
    const map = mapDouble();
    synchronizeBasemap(map as never, null, "light", "map", configuredOnlineBasemap);

    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_VECTOR_SOURCE_ID,
      expect.objectContaining({ tiles: [TAURI_TIANDITU_VECTOR_TEMPLATE] }),
    );
    expect(map.addSource).toHaveBeenCalledWith(
      TIANDITU_LABEL_SOURCE_ID,
      expect.objectContaining({ tiles: [TAURI_TIANDITU_VECTOR_LABEL_TEMPLATE] }),
    );
    expect(map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: TIANDITU_LABEL_LAYER_ID }),
      "completed-point-halo",
    );

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
  });
});
