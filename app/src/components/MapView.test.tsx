import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BasemapInfo, CalculationResult } from "../lib/types";

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
  mapOverlayCorners: [
    [101, 31],
    [105, 31],
    [105, 29],
    [101, 29],
  ],
  mapOverlayPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
} as CalculationResult;

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
  resourcePath: "/api/basemap/pmtiles/four-provinces.pmtiles",
  bounds: [107.5, 18, 125.5, 33.5],
  archiveBytes: 33_044_072,
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
        heatmap={null}
        preview={null}
        heatmapStale={false}
        onPointSelect={vi.fn()}
      />,
    );

    expect(maplibreMocks.addProtocol).toHaveBeenCalledWith(
      "pmtiles",
      expect.any(Function),
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
      heatmap: sampleHeatmap,
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
      expect.objectContaining({ id: "coverage-heatmap-layer" }),
      "coverage-circle-fill",
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

  it("replays a deferred clear after the style becomes ready", () => {
    const sharedProps = {
      theme: "dark" as const,
      point: { lat: 30, lon: 103 },
      preview: null,
      heatmapStale: false,
      onPointSelect: vi.fn(),
    };
    const { rerender, unmount } = render(<MapView {...sharedProps} heatmap={sampleHeatmap} />);

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("load");
    expect(maplibreMocks.map.getLayer("coverage-heatmap-layer")).toBeDefined();
    expect(objectUrlMocks.create).toHaveBeenCalledOnce();

    maplibreMocks.map.isStyleLoaded.mockReturnValue(false);
    rerender(<MapView {...sharedProps} heatmap={null} />);
    expect(maplibreMocks.map.removeLayer).not.toHaveBeenCalled();
    expect(objectUrlMocks.revoke).not.toHaveBeenCalled();

    maplibreMocks.map.isStyleLoaded.mockReturnValue(true);
    maplibreMocks.emit("idle");
    expect(maplibreMocks.map.removeLayer).toHaveBeenCalledWith("coverage-heatmap-layer");
    expect(maplibreMocks.map.removeSource).toHaveBeenCalledWith("coverage-heatmap");
    expect(objectUrlMocks.revoke).toHaveBeenCalledWith("blob:coverage-heatmap");

    unmount();
    expect(objectUrlMocks.revoke).toHaveBeenCalledOnce();
  });
});
