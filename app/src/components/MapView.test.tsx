import { act, fireEvent, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BasemapInfo, CalculationResult, SessionCoverageResult } from "../lib/types";

const objectUrlMocks = vi.hoisted(() => ({
  create: vi.fn(() => "blob:coverage-heatmap"),
  revoke: vi.fn(),
}));

const maplibreMocks = vi.hoisted(() => {
  type Handler = (...args: unknown[]) => void;
  const handlers = new Map<string, Set<Handler>>();
  const sources = new Map<
    string,
    {
      setData?: ReturnType<typeof vi.fn>;
      updateImage?: ReturnType<typeof vi.fn>;
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
        source.type === "image" ? { updateImage: vi.fn() } : { setData: vi.fn() },
      );
    }),
    getLayer: vi.fn((id: string) => (layers.has(id) ? { id } : undefined)),
    fitBounds: vi.fn(),
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
    resetState() {
      handlers.clear();
      sources.clear();
      layers.clear();
    },
    addProtocol: vi.fn(),
    removeProtocol: vi.fn(),
    Map: vi.fn(function Map() {
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
  resourcePath: "/api/basemap/pmtiles/four-provinces.pmtiles",
  bounds: [107.5, 18, 125.5, 33.5],
  archiveBytes: 33_044_072,
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

describe("MapView controls and desired-state replay", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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

    expect(maplibreMocks.addProtocol).toHaveBeenCalledWith(
      "pmtiles",
      expect.any(Function),
    );
    expect(maplibreMocks.Map).toHaveBeenCalledWith(
      expect.objectContaining({
        maxZoom: 12,
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
    expect(maplibreMocks.removeProtocol).toHaveBeenCalledWith("pmtiles");
  });

  it("fits the trusted regional archive once without resetting later state replays", () => {
    const props = {
      theme: "dark" as const,
      point: null,
      heatmaps: [sampleCoverage],
      activeHeatmapId: sampleCoverage.id,
      preview: null,
      heatmapStale: false,
      onPointSelect: vi.fn(),
    };
    const { getByText, rerender, unmount } = render(
      <MapView {...props} basemap={configuredProtomaps} />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    expect(maplibreMocks.map.fitBounds).toHaveBeenCalledWith(
      [
        [107.5, 18],
        [125.5, 33.5],
      ],
      { padding: 48, duration: 0, maxZoom: 4.5 },
    );
    expect(getByText("区域离线底图 · 内部验证")).toBeDefined();
    expect(getByText("© OpenStreetMap contributors · 本地区域底图")).toBeDefined();
    expect(maplibreMocks.map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: "coverage-heatmap-layer-coverage-1" }),
      "basemap-protomaps-place-province",
    );
    expect(maplibreMocks.map.setPaintProperty).toHaveBeenCalledWith(
      "basemap-protomaps-earth",
      "fill-color",
      "#17242b",
    );

    maplibreMocks.map.setPaintProperty.mockClear();
    rerender(
      <MapView
        {...{ ...props, theme: "light" as const }}
        basemap={{ ...configuredProtomaps }}
      />,
    );
    expect(maplibreMocks.map.fitBounds).toHaveBeenCalledOnce();
    expect(
      maplibreMocks.map.addSource.mock.calls.filter(
        ([id]) => id === "basemap-protomaps",
      ),
    ).toHaveLength(1);
    expect(maplibreMocks.addProtocol).toHaveBeenCalledOnce();
    expect(maplibreMocks.removeProtocol).not.toHaveBeenCalled();
    expect(maplibreMocks.map.setPaintProperty).toHaveBeenCalledWith(
      "basemap-protomaps-earth",
      "fill-color",
      "#d9ddd7",
    );
    expect(maplibreMocks.map.setPaintProperty).toHaveBeenCalledWith(
      "basemap-protomaps-roads",
      "line-color",
      "#8b8174",
    );

    unmount();
  });

  it("switches between the offline map and online satellite imagery", () => {
    const { getByRole, getByText, unmount } = render(
      <MapView
        theme="dark"
        point={null}
        heatmaps={[]}
        activeHeatmapId={null}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
        basemap={configuredProtomaps}
      />,
    );

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    fireEvent.click(getByRole("button", { name: /卫星/ }));

    expect(getByText("Sentinel-2 卫星影像（联网）· 中文地名 · 内部验证")).toBeDefined();
    expect(maplibreMocks.map.addSource).toHaveBeenCalledWith(
      "basemap-satellite",
      expect.objectContaining({
        type: "raster",
        tiles: ["/api/basemap/satellite/{z}/{x}/{y}"],
        maxzoom: 14,
      }),
    );
    expect(maplibreMocks.map.addLayer).toHaveBeenCalledWith(
      expect.objectContaining({ id: "basemap-satellite-layer" }),
      "graticule-lines",
    );
    expect(maplibreMocks.map.setLayoutProperty).toHaveBeenCalledWith(
      "basemap-protomaps-earth",
      "visibility",
      "none",
    );

    fireEvent.click(getByRole("button", { name: "地图" }));
    expect(maplibreMocks.map.removeLayer).toHaveBeenCalledWith(
      "basemap-satellite-layer",
    );
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith("basemap-satellite");

    fireEvent.click(getByRole("button", { name: /卫星/ }));
    act(() => {
      maplibreMocks.emit("error", { sourceId: "basemap-satellite" });
    });
    expect(getByText("卫星影像不可用，已切回区域离线地图")).toBeDefined();

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
    expect(objectUrlMocks.create).toHaveBeenCalledTimes(2);

    unmount();
    expect(objectUrlMocks.revoke).toHaveBeenCalledTimes(2);
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
    expect(objectUrlMocks.create).toHaveBeenCalledOnce();

    maplibreMocks.map.isStyleLoaded.mockReturnValue(false);
    rerender(<MapView {...sharedProps} heatmaps={[]} activeHeatmapId={null} />);
    expect(maplibreMocks.map.removeLayer).not.toHaveBeenCalled();
    expect(objectUrlMocks.revoke).not.toHaveBeenCalled();

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("idle");
    expect(maplibreMocks.map.removeLayer).toHaveBeenCalledWith("coverage-heatmap-layer-coverage-1");
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith("coverage-heatmap-coverage-1");
    expect(objectUrlMocks.revoke).toHaveBeenCalledWith("blob:coverage-heatmap");

    unmount();
    expect(objectUrlMocks.revoke).toHaveBeenCalledOnce();
  });
});
