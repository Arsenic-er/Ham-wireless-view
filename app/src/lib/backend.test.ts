import { afterEach, describe, expect, it, vi } from "vitest";

import {
  backendCapabilities,
  backendMode,
  bootstrap,
  cancelCalculation,
  exportReport,
  inspectPoint,
} from "./backend";

function removeTauriInternals(): void {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
}

afterEach(() => {
  removeTauriInternals();
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe("backend mode", () => {
  it("keeps an ordinary browser in interface-only preview mode", () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "0");

    expect(backendMode()).toBe("preview");
    expect(backendCapabilities()).toEqual({
      mode: "preview",
      canDownload: false,
      canDeleteCache: false,
      canCalculate: false,
      canExport: false,
    });
  });

  it("enables server operations but never server-side file export", () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");

    expect(backendCapabilities()).toEqual({
      mode: "validation-server",
      canDownload: true,
      canDeleteCache: true,
      canCalculate: true,
      canExport: false,
    });
  });

  it("gives Tauri precedence over the validation build flag", () => {
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });

    expect(backendMode()).toBe("tauri");
    expect(backendCapabilities().canExport).toBe(true);
  });
});

describe("validation server adapter", () => {
  it("uses same-origin JSON endpoints and wrapped Tauri-shaped request bodies", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            schemaVersion: 2,
            modelName: "NTIA ITM Point-to-Point",
            modelVersion: "land-water-v1",
            coverageRadiusKm: 200,
            gridSize: 401,
            cacheUsage: {
              totalBytes: 0,
              demBytes: 0,
              waterBytes: 0,
              partialBytes: 0,
              metadataBytes: 0,
              remainingBytes: 2_500_000_000,
              capBytes: 2_500_000_000,
            },
            internalBuildWarning: "internal",
          }),
          { headers: { "content-type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            point: { lat: 30.5, lon: 103.5 },
            regionId: "region",
            tileCount: 25,
            readyDemCount: 25,
            readyWaterCount: 25,
            missingAssetCount: 0,
            dataReady: true,
            elevationM: 512,
            cacheUsage: {
              totalBytes: 1,
              demBytes: 1,
              waterBytes: 0,
              partialBytes: 0,
              metadataBytes: 0,
              remainingBytes: 2_499_999_999,
              capBytes: 2_500_000_000,
            },
          }),
          { headers: { "content-type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);

    await bootstrap();
    await inspectPoint({ lat: 30.5, lon: 103.5 });
    await cancelCalculation();

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "/api/bootstrap",
      expect.objectContaining({ method: "GET" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "/api/inspect-point",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ point: { lat: 30.5, lon: 103.5 } }),
      }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "/api/cancel-calculation",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({ "Content-Type": "application/json" }),
        body: undefined,
      }),
    );
  });

  it("surfaces JSON API errors and refuses server-side export", async () => {
    removeTauriInternals();
    vi.stubEnv("VITE_VALIDATION_SERVER", "1");
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>().mockResolvedValue(
        new Response(JSON.stringify({ message: "busy" }), {
          status: 409,
          headers: { "content-type": "application/json" },
        }),
      ),
    );

    await expect(bootstrap()).rejects.toThrow("busy");
    await expect(
      exportReport({ format: "png", suggestedFileName: "x.png", reportPngDataUrl: "data:" }),
    ).rejects.toThrow("Tauri Windows");
  });
});
