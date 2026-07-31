import { afterEach, describe, expect, it, vi } from "vitest";

import { buildPdfFromJpeg, exportReportInBrowser } from "./browserExport";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("browser report export", () => {
  it("builds a self-contained one-page PDF around the report image", () => {
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xd9]);
    const pdf = buildPdfFromJpeg(jpeg, 1600, 1100);
    const text = new TextDecoder().decode(pdf);

    expect(Array.from(pdf.slice(0, 8))).toEqual([
      0x25,
      0x50,
      0x44,
      0x46,
      0x2d,
      0x31,
      0x2e,
      0x34,
    ]);
    expect(text).toContain("/Type /Pages /Kids [3 0 R] /Count 1");
    expect(text).toContain("/Filter /DCTDecode");
    expect(text).toContain("xref\n0 6");
    expect(text).toMatch(/startxref\n\d+\n%%EOF\n$/);
  });

  it("rejects invalid JPEG input and dimensions", () => {
    expect(() => buildPdfFromJpeg(new Uint8Array([1, 2, 3, 4]), 1, 1)).toThrow(
      "JPEG",
    );
    expect(() =>
      buildPdfFromJpeg(new Uint8Array([0xff, 0xd8, 0xff, 0xd9]), 0, 1),
    ).toThrow("尺寸");
  });

  it("downloads a PNG locally without using a server export route", async () => {
    vi.useFakeTimers();
    const createObjectURL = vi.fn(() => "blob:report");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);

    const result = await exportReportInBrowser({
      format: "png",
      suggestedFileName: "coverage.png",
      reportPngDataUrl: "data:image/png;base64,iVBORw0KGgo=",
    });

    expect(result).toEqual({ cancelled: false, path: null, bytesWritten: 8 });
    expect(createObjectURL).toHaveBeenCalledOnce();
    expect(click).toHaveBeenCalledOnce();
    expect(document.querySelector('a[download="coverage.png"]')).toBeNull();
    vi.runAllTimers();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:report");
  });
});
