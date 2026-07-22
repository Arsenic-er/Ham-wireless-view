import { describe, expect, it } from "vitest";

import {
  DEFAULT_PARAMETERS,
  applyPreset,
  convertGainUnit,
  convertPowerUnit,
  parameterValidationMessage,
  switchBand,
} from "./parameters";

describe("radio parameters", () => {
  it("applies both new-user presets without changing the band", () => {
    const handheld = applyPreset(DEFAULT_PARAMETERS, "handheld-to-base");
    expect(handheld.band).toBe("vhf144");
    expect(handheld.powerValue).toBe(5);
    expect(handheld.txHeightM).toBe(1.5);
    expect(handheld.rxHeightM).toBe(20);
  });

  it("selects a deterministic default frequency for each band", () => {
    expect(switchBand(DEFAULT_PARAMETERS, "uhf430").frequencyMhz).toBe(435);
    expect(switchBand(DEFAULT_PARAMETERS, "vhf144").frequencyMhz).toBe(145);
  });

  it("round-trips supported display units", () => {
    const dbm = convertPowerUnit(25, "watt", "dbm");
    expect(convertPowerUnit(dbm, "dbm", "watt")).toBeCloseTo(25, 10);
    expect(convertGainUnit(convertGainUnit(6, "dbi", "dbd"), "dbd", "dbi")).toBeCloseTo(
      6,
      12,
    );
  });

  it("rejects a frequency outside its selected band", () => {
    expect(
      parameterValidationMessage({ ...DEFAULT_PARAMETERS, frequencyMhz: 435 }),
    ).toContain("144.00–148.00");
    expect(parameterValidationMessage(DEFAULT_PARAMETERS)).toBeNull();
  });
});

