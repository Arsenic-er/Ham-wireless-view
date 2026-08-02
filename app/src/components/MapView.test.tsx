// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { act, fireEvent, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "../i18n";
import type { BasemapInfo, CalculationResult, OnlineBasemapInfo, SessionCoverageResult } from "../lib/types";

const objectUrlMocks = vi.hoisted(() => ({
  create: vi.fn(() => "blob:coverage-heatmap"),
  revoke: vi.fn(),
}));
const canvasLeaseMocks = vi.hoisted(() => ({
  instances: [] as Array<{
    dirty: boolean;
    applyThreshold: ReturnType<typeof vi.fn>;
    markUploaded: ReturnType<typeof vi.fn>;
    dispose: ReturnType<typeof vi.fn>;
  }>,
}));


const maplibreMocks = vi.hoisted(() => {
  type Handler = (...args: unknown[]) => void;
  const handlers = new Map<string, Set<Handler>>();
  let viewport = { west: 90, south: 20, east: 120, north: 40 };
  const sources = new Map<
    string,
    {
      tiles?: Record<string, unknown>;
      setData?: ReturnType<typeof vi.fn>;
      updateImage?: ReturnType<typeof vi.fn>;
      play?: ReturnType<typeof vi.fn>;
      pause?: ReturnType<typeof vi.fn>;
      setCoordinates?: ReturnType<typeof vi.fn>;
    }
  >();
  const layers = new Set<string>();
  const map = {
    addControl: vi.fn(),
    addLayer: vi.fn((layer: { id: string }) => {
      layers.add(layer.id);
    }),
    addSource: vi.fn((id: string, source: { type?: string }) => {
      sources.set(
        id,
        source.type === "image"
          ? { updateImage: vi.fn() }
          : source.type === "canvas"
            ? {
                tiles: { visible: {} },
                play: vi.fn(),
                pause: vi.fn(),
                setCoordinates: vi.fn(),
              }
            : { setData: vi.fn() },
      );
    }),
    getLayer: vi.fn((id: string) => (layers.has(id) ? { id } : undefined)),
    fitBounds: vi.fn(),
    getBounds: vi.fn(() => ({
      getWest: () => viewport.west,
      getSouth: () => viewport.south,
      getEast: () => viewport.east,
      getNorth: () => viewport.north,
    })),
    getSource: vi.fn((id: string) => sources.get(id)),
    isStyleLoaded: vi.fn(() => false),
    on: vi.fn((event: string, handler: Handler) => {
      const eventHandlers = handlers.get(event) ?? new Set<Handler>();
      eventHandlers.add(handler);
      handlers.set(event, eventHandlers);
    }),
    remove: vi.fn(),
    removeLayer: vi.fn((id: string) => {
      layers.delete(id);
    }),
    removeSource: vi.fn((id: string) => {
      sources.delete(id);
    }),
    setMaxZoom: vi.fn(),
    setZoom: vi.fn(),
    setLayoutProperty: vi.fn(),
    setPaintProperty: vi.fn(),
  };
  const navigationControl = { kind: "navigation-control" };
  const scaleControl = { kind: "scale-control" };
  return {
    map,
    navigationControl,
    scaleControl,
    emit(event: string, ...args: unknown[]) {
      for (const handler of handlers.get(event) ?? []) handler(...args);
    },
    setViewport(next: typeof viewport) {
      viewport = next;
    },
    resetState() {
      handlers.clear();
      sources.clear();
      layers.clear();
    },
    addProtocol: vi.fn(),
    removeProtocol: vi.fn(),
    Map: vi.fn(function Map(options?: { container?: HTMLElement }) {
      if (options?.container) {
        const zoomIn = document.createElement("button");
        zoomIn.className = "maplibregl-ctrl-zoom-in";
        const zoomOut = document.createElement("button");
        zoomOut.className = "maplibregl-ctrl-zoom-out";
        options.container.append(zoomIn, zoomOut);
      }
      return map;
    }),
    NavigationControl: vi.fn(function NavigationControl() {
      return navigationControl;
    }),
    ScaleControl: vi.fn(function ScaleControl() {
      return scaleControl;
    }),
  };
});

vi.mock("maplibre-gl", () => ({
  default: {
    addProtocol: maplibreMocks.addProtocol,
    removeProtocol: maplibreMocks.removeProtocol,
    Map: maplibreMocks.Map,
    NavigationControl: maplibreMocks.NavigationControl,
    ScaleControl: maplibreMocks.ScaleControl,
  },
}));

vi.mock("../lib/mapOverlayCanvas", () => ({
  MapOverlayCanvasLease: class MapOverlayCanvasLease {
    readonly canvas = document.createElement("canvas");
    readonly ready = true;
    readonly coordinates = [
      [101, 31],
      [105, 31],
      [105, 29],
      [101, 29],
    ];
    dirty = false;
    readonly update = vi.fn(() => true);
    readonly applyThreshold = vi.fn(() => {
      this.dirty = true;
      return true;
    });
    readonly markUploaded = vi.fn(() => {
      this.dirty = false;
    });
    readonly dispose = vi.fn();

    constructor() {
      canvasLeaseMocks.instances.push(this);
    }
  },
}));

import { MapView } from "./MapView";

const sampleHeatmap = {
  mapOverlayProjection: "EPSG:3857",
  center: { lat: 30, lon: 103 },
  mapOverlayCorners: [
    [101, 31],
    [105, 31],
    [105, 29],
    [101, 29],
  ],
  mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
} as CalculationResult;

const sampleCoverage = {
  id: "coverage-1",
  result: sampleHeatmap,
} as SessionCoverageResult;

const sampleCoverage2 = {
  id: "coverage-2",
  result: {
    ...sampleHeatmap,
    center: { lat: 31, lon: 104 },
    mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgoAAA==",
  },
} as SessionCoverageResult;

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
  tilePathTemplate: "/api/basemap/tianditu/{layer}/{z}/{x}/{y}",
  satellite: {
    enabled: true,
    providerId: "eoxcloudless",
    displayName: "Sentinel-2 2025",
    attribution:
      "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)",
    mode: "same-origin-proxy",
    maxZoom: 14,
    tilePathTemplate: "/api/basemap/satellite/{z}/{x}/{y}",
  },
};

const configuredSatelliteOnly: BasemapInfo = {
  ...configuredTianditu,
  enabled: false,
};

const configuredCarto: BasemapInfo = {
  enabled: true,
  providerId: "carto-voyager",
  displayName: "CARTO Voyager / OpenStreetMap",
  attribution: "© OpenStreetMap contributors © CARTO",
  mode: "same-origin-proxy",
  maxZoom: 18,
  layers: [
    { id: "base", displayName: "Map" },
    { id: "labels", displayName: "Place labels" },
  ],
  tilePathTemplate: "/api/basemap/carto/{layer}/{z}/{x}/{y}",
  satellite: configuredTianditu.satellite,
};

const configuredOnlineBasemap: OnlineBasemapInfo = {
  configured: true,
  provider: "Tianditu",
  protocolScheme: "tianditu",
  vectorTemplate: "tianditu://localhost/vec/{z}/{x}/{y}",
  vectorLabelTemplate: "tianditu://localhost/cva/{z}/{x}/{y}",
  imageryTemplate: "tianditu://localhost/img/{z}/{x}/{y}",
  imageryLabelTemplate: "tianditu://localhost/cia/{z}/{x}/{y}",
  attribution: "天地图",
  minZoom: 1,
  maxZoom: 18,
};

describe("MapView controls and desired-state replay", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    canvasLeaseMocks.instances.length = 0;
    maplibreMocks.resetState();
    maplibreMocks.map.isStyleLoaded.mockReturnValue(false);
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: objectUrlMocks.create,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: objectUrlMocks.revoke,
    });
  });

  it("places a dynamic metric scale in the bottom-right corner", () => {
    const { unmount } = render(
      <MapView
        theme="dark"
        point={null}
        heatmaps={[]}
        activeHeatmapId={null}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
      />,
    );

    expect(maplibreMocks.Map).toHaveBeenCalledWith(
      expect.objectContaining({
        maxZoom: 18,
        localIdeographFontFamily: expect.stringContaining("Microsoft YaHei"),
      }),
    );
    expect(maplibreMocks.NavigationControl).toHaveBeenCalledWith({ showCompass: false });
    expect(maplibreMocks.ScaleControl).toHaveBeenCalledWith({
      maxWidth: 120,
      unit: "metric",
    });
    expect(maplibreMocks.map.addControl).toHaveBeenNthCalledWith(
      1,
      maplibreMocks.navigationControl,
      "top-left",
    );
    expect(maplibreMocks.map.addControl).toHaveBeenNthCalledWith(
      2,
      maplibreMocks.scaleControl,
      "bottom-right",
    );

    unmount();
    expect(maplibreMocks.map.remove).toHaveBeenCalledOnce();
    expect(maplibreMocks.addProtocol).not.toHaveBeenCalled();
    expect(maplibreMocks.removeProtocol).not.toHaveBeenCalled();
  });


  it("updates localized controls without reconstructing MapLibre", async () => {
    const { container, unmount } = render(
      <MapView
        theme="dark"
        point={{ lat: 30.5, lon: 103.5 }}
        heatmaps={[sampleCoverage]}
        activeHeatmapId={sampleCoverage.id}
        preview={null}
        heatmapStale={false}
        visibleSignalThresholdDbm={-120}
        onPointSelect={vi.fn()}
      />,
    );

    const zoomIn = container.querySelector(".maplibregl-ctrl-zoom-in");
    expect(zoomIn?.getAttribute("aria-label")).toBe("放大");
    expect(maplibreMocks.Map).toHaveBeenCalledTimes(1);

    try {
      await act(async () => {
        await i18n.changeLanguage("ja-JP");
      });
      expect(zoomIn?.getAttribute("aria-label")).toBe("拡大");
      expect(maplibreMocks.Map).toHaveBeenCalledTimes(1);
      expect(maplibreMocks.map.remove).not.toHaveBeenCalled();
    } finally {
      await act(async () => {
        await i18n.changeLanguage("zh-CN");
      });
      unmount();
    }
  });

  it("uses same-origin online map and satellite and falls back to WGS84", () => {
    const { getByRole, getByText, unmount } = render(
      <MapView
        theme="dark"
        point={null}
        heatmaps={[sampleCoverage]}
        activeHeatmapId={sampleCoverage.id}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
        basemap={configuredTianditu}
      />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    expect(getByText("天地图在线真实底图 · 内部验证")).toBeDefined();
    expect(maplibreMocks.map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: "coverage-heatmap-layer-coverage-1" }),
      "basemap-tianditu-label-layer",
    );

    act(() => window.dispatchEvent(new Event("offline")));
    expect(getByText("在线地图不可用，已回退 WGS84 坐标网格")).toBeDefined();
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith(
      "basemap-tianditu-vector",
    );
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();

    fireEvent.click(getByRole("button", { name: "重试" }));
    fireEvent.click(getByRole("button", { name: /卫星/ }));
    expect(getByText("Sentinel-2 卫星影像（联网）· 中文地名 · 内部验证")).toBeDefined();
    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-satellite" });
    });
    expect(getByText("卫星影像不可用，已切回在线地图")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-tianditu-vector")).toBeDefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith(
      "basemap-satellite",
    );

    fireEvent.click(getByRole("button", { name: /卫星/ }));
    expect(getByText("Sentinel-2 卫星影像（联网）· 中文地名 · 内部验证")).toBeDefined();
    fireEvent.click(getByRole("button", { name: "地图" }));
    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-tianditu-label" });
    });
    expect(
      getByText("地名图层暂不可用；底图与分析仍可使用。"),
    ).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-tianditu-label")).toBeUndefined();
    expect(maplibreMocks.map.getSource("basemap-tianditu-vector")).toBeDefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();

    fireEvent.click(getByRole("button", { name: "重试" }));
    expect(maplibreMocks.map.getSource("basemap-tianditu-label")).toBeDefined();
    expect(getByText("天地图在线真实底图 · 内部验证")).toBeDefined();
    expect(maplibreMocks.map.setMaxZoom).toHaveBeenLastCalledWith(18);
    expect(maplibreMocks.map.setZoom).not.toHaveBeenCalled();

    unmount();
  });


  it("uses CARTO without a TianDiTu token and keeps labels over satellite", () => {
    const { getByRole, getByText, unmount } = render(
      <MapView
        theme="dark"
        point={null}
        heatmaps={[sampleCoverage]}
        activeHeatmapId={sampleCoverage.id}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
        basemap={configuredCarto}
      />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    expect(
      getByText("CARTO Voyager 在线地图 · 地名 · 内部验证"),
    ).toBeDefined();
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-carto-base",
      expect.objectContaining({
        tiles: ["/api/basemap/carto/base/{z}/{x}/{y}"],
      }),
    );
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-carto-labels",
      expect.objectContaining({
        tiles: ["/api/basemap/carto/labels/{z}/{x}/{y}"],
      }),
    );
    expect(maplibreMocks.map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: "coverage-heatmap-layer-coverage-1" }),
      "basemap-carto-labels-layer",
    );

    fireEvent.click(getByRole("button", { name: /卫星/ }));
    expect(
      getByText("Sentinel-2 卫星影像（联网）· CARTO 地名 · 内部验证"),
    ).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-carto-labels")).toBeDefined();

    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-carto-labels" });
    });
    expect(
      getByText("地名图层暂不可用；底图与分析仍可使用。"),
    ).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-carto-labels")).toBeUndefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeDefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();

    fireEvent.click(getByRole("button", { name: "重试" }));
    expect(maplibreMocks.map.getSource("basemap-carto-labels")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeDefined();

    fireEvent.click(getByRole("button", { name: "地图" }));
    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-carto-base" });
    });
    expect(
      getByText("普通在线地图不可用，已切换到卫星影像。"),
    ).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-carto-base")).toBeUndefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-carto-labels")).toBeDefined();

    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-satellite" });
    });
    expect(
      getByText("在线地图不可用，已回退 WGS84 坐标网格"),
    ).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-carto-base")).toBeUndefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeUndefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();

    unmount();
  });

  it("falls back from CARTO satellite to map once, then uses WGS84 if map also fails", () => {
    const { getByRole, getByText, unmount } = render(
      <MapView
        theme="dark"
        point={null}
        heatmaps={[sampleCoverage]}
        activeHeatmapId={sampleCoverage.id}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
        basemap={configuredCarto}
      />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    fireEvent.click(getByRole("button", { name: /卫星/ }));
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeDefined();

    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-satellite" });
    });
    expect(getByText("卫星影像不可用，已切回在线地图")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeUndefined();
    expect(maplibreMocks.map.getSource("basemap-carto-base")).toBeDefined();

    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-carto-base" });
    });
    expect(
      getByText("在线地图不可用，已回退 WGS84 坐标网格"),
    ).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-carto-base")).toBeUndefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeUndefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();

    unmount();
  });

  it("uses a trusted satellite when the ordinary validation map is disabled", () => {
    const { getByRole, getByText, unmount } = render(
      <MapView
        theme="dark"
        point={null}
        heatmaps={[sampleCoverage]}
        activeHeatmapId={sampleCoverage.id}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
        basemap={configuredSatelliteOnly}
      />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    expect(getByText("WGS84 内部测试画布 · 未配置真实底图")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-tianditu-vector")).toBeUndefined();

    fireEvent.click(getByRole("button", { name: /卫星/ }));
    expect(getByText("Sentinel-2 卫星影像（联网）· 内部验证")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeDefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();

    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-satellite" });
    });
    expect(getByText("在线地图不可用，已回退 WGS84 坐标网格")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeUndefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();
    expect(maplibreMocks.map.setMaxZoom).toHaveBeenLastCalledWith(18);
    expect(maplibreMocks.map.setZoom).not.toHaveBeenCalled();

    fireEvent.click(getByRole("button", { name: "重试" }));
    expect(getByText("Sentinel-2 卫星影像（联网）· 内部验证")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-satellite")).toBeDefined();

    unmount();
  });
  it("falls back from desktop imagery to its ordinary vector map", () => {
    const { getByRole, getByText, unmount } = render(
      <MapView
        theme="dark"
        point={null}
        heatmaps={[sampleCoverage]}
        activeHeatmapId={sampleCoverage.id}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
        onlineBasemap={configuredOnlineBasemap}
      />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    expect(getByText("天地图在线矢量底图 · 中文地名")).toBeDefined();
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-tianditu-vector",
      expect.objectContaining({
        tiles: ["tianditu://localhost/vec/{z}/{x}/{y}"],
        minzoom: 1,
        maxzoom: 18,
      }),
    );
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-tianditu-label",
      expect.objectContaining({
        tiles: ["tianditu://localhost/cva/{z}/{x}/{y}"],
      }),
    );
    expect(maplibreMocks.map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: "coverage-heatmap-layer-coverage-1" }),
      "basemap-tianditu-label-layer",
    );
    expect(maplibreMocks.map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: "basemap-tianditu-label-layer" }),
      "completed-point-halo",
    );
    expect(maplibreMocks.map.setMaxZoom).toHaveBeenCalledWith(18);

    fireEvent.click(getByRole("button", { name: /卫星/ }));
    expect(getByText("天地图卫星影像（联网）· 中文地名")).toBeDefined();
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-tianditu-imagery",
      expect.objectContaining({
        tiles: ["tianditu://localhost/img/{z}/{x}/{y}"],
      }),
    );
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-tianditu-imagery-label",
      expect.objectContaining({
        tiles: ["tianditu://localhost/cia/{z}/{x}/{y}"],
      }),
    );

    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-tianditu-imagery" });
    });
    expect(getByText("卫星影像不可用，已切回在线地图")).toBeDefined();
    expect(maplibreMocks.map.getSource("basemap-tianditu-vector")).toBeDefined();
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith(
      "basemap-tianditu-imagery",
    );
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith(
      "basemap-tianditu-imagery-label",
    );
    expect(maplibreMocks.map.setMaxZoom).toHaveBeenLastCalledWith(18);
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();

    fireEvent.click(getByRole("button", { name: /卫星/ }));
    expect(getByText("天地图卫星影像（联网）· 中文地名")).toBeDefined();
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-tianditu-imagery",
      expect.any(Object),
    );
    expect(maplibreMocks.map.setMaxZoom).toHaveBeenLastCalledWith(18);

    unmount();
  });

  it("keeps separate image sources and layers for completed transmitter sites", () => {
    const props = {
      theme: "dark" as const,
      point: { lat: 30, lon: 103 },
      preview: null,
      heatmapStale: false,
      onPointSelect: vi.fn(),
    };
    const { rerender, unmount } = render(
      <MapView
        {...props}
        heatmaps={[sampleCoverage]}
        activeHeatmapId={sampleCoverage.id}
      />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    rerender(
      <MapView
        {...props}
        heatmaps={[sampleCoverage, sampleCoverage2]}
        activeHeatmapId={sampleCoverage2.id}
      />,
    );

    expect(maplibreMocks.map.getLayer("coverage-heatmap-layer-coverage-1")).toBeDefined();
    expect(maplibreMocks.map.getLayer("coverage-heatmap-layer-coverage-2")).toBeDefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-1")).toBeDefined();
    expect(maplibreMocks.map.getSource("coverage-heatmap-coverage-2")).toBeDefined();
    expect(maplibreMocks.map.removeLayer).not.toHaveBeenCalledWith(
      "coverage-heatmap-layer-coverage-1",
    );
    expect(canvasLeaseMocks.instances).toHaveLength(2);

    unmount();
    expect(canvasLeaseMocks.instances.every((lease) => lease.dispose.mock.calls.length === 1)).toBe(true);
  });

  it("replays a deferred clear after the style becomes ready", () => {
    const sharedProps = {
      theme: "dark" as const,
      point: { lat: 30, lon: 103 },
      preview: null,
      heatmapStale: false,
      onPointSelect: vi.fn(),
    };
    const { rerender, unmount } = render(<MapView {...sharedProps} heatmaps={[sampleCoverage]} activeHeatmapId={sampleCoverage.id} />);

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    expect(maplibreMocks.map.getLayer("coverage-heatmap-layer-coverage-1")).toBeDefined();
    expect(canvasLeaseMocks.instances).toHaveLength(1);

    maplibreMocks.map.isStyleLoaded.mockReturnValue(false);
    rerender(<MapView {...sharedProps} heatmaps={[]} activeHeatmapId={null} />);
    expect(maplibreMocks.map.removeLayer).not.toHaveBeenCalled();
    expect(canvasLeaseMocks.instances[0].dispose).not.toHaveBeenCalled();

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("idle");
    expect(maplibreMocks.map.removeLayer).toHaveBeenCalledWith("coverage-heatmap-layer-coverage-1");
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith("coverage-heatmap-coverage-1");
    expect(canvasLeaseMocks.instances[0].dispose).toHaveBeenCalledOnce();

    unmount();
    expect(canvasLeaseMocks.instances[0].dispose).toHaveBeenCalledOnce();
  });

  it("keeps an offscreen canvas dirty and uploads it once after it returns to the viewport", () => {
    vi.useFakeTimers();
    const props = {
      theme: "dark" as const,
      point: { lat: 30, lon: 103 },
      heatmaps: [sampleCoverage],
      activeHeatmapId: sampleCoverage.id,
      preview: null,
      heatmapStale: false,
      onPointSelect: vi.fn(),
    };
    const { rerender, unmount } = render(
      <MapView {...props} visibleSignalThresholdDbm={-140} />,
    );

    try {
      maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
      maplibreMocks.emit("load");
      const source = maplibreMocks.map.getSource(
        "coverage-heatmap-coverage-1",
      ) as {
        play: ReturnType<typeof vi.fn>;
        pause: ReturnType<typeof vi.fn>;
        tiles: Record<string, unknown>;
      };
      source.play.mockClear();
      source.pause.mockClear();
      const lease = canvasLeaseMocks.instances[0];
      lease.markUploaded.mockClear();

      maplibreMocks.setViewport({
        west: 10,
        south: 0,
        east: 20,
        north: 10,
      });
      rerender(<MapView {...props} visibleSignalThresholdDbm={-120} />);
      act(() => {
        vi.advanceTimersByTime(40);
      });

      expect(lease.applyThreshold).toHaveBeenCalledWith(-120);
      expect(lease.dirty).toBe(true);
      expect(source.play).not.toHaveBeenCalled();
      expect(source.pause).not.toHaveBeenCalled();

      maplibreMocks.setViewport({
        west: 100,
        south: 20,
        east: 110,
        north: 40,
      });
      source.tiles = {};
      act(() => {
        maplibreMocks.emit("moveend");
      });
      expect(source.play).not.toHaveBeenCalled();
      expect(lease.dirty).toBe(true);

      source.tiles = { visible: {} };
      act(() => {
        maplibreMocks.emit("idle");
      });
      expect(source.play).toHaveBeenCalledOnce();
      expect(source.pause).toHaveBeenCalledOnce();
      expect(lease.markUploaded).toHaveBeenCalledOnce();
      expect(lease.dirty).toBe(false);

      act(() => {
        maplibreMocks.emit("idle");
      });
      expect(source.play).toHaveBeenCalledOnce();
    } finally {
      unmount();
      vi.useRealTimers();
    }
  });
});
