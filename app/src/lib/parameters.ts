import type {
  Band,
  GainUnit,
  PowerUnit,
  RadioParameters,
  ScenarioPreset,
} from "./types";

const PRESETS: Record<
  ScenarioPreset,
  Omit<RadioParameters, "preset" | "band" | "frequencyMhz" | "polarization" | "txGroundElevationOverrideM">
> = {
  "base-to-handheld": {
    powerValue: 25,
    powerUnit: "watt",
    txGainValue: 6,
    txGainUnit: "dbi",
    txHeightM: 20,
    rxGainValue: -3,
    rxGainUnit: "dbi",
    rxHeightM: 1.5,
  },
  "handheld-to-base": {
    powerValue: 5,
    powerUnit: "watt",
    txGainValue: -3,
    txGainUnit: "dbi",
    txHeightM: 1.5,
    rxGainValue: 6,
    rxGainUnit: "dbi",
    rxHeightM: 20,
  },
};

export const DEFAULT_PARAMETERS: RadioParameters = {
  preset: "base-to-handheld",
  band: "vhf144",
  frequencyMhz: 145,
  polarization: "vertical",
  txGroundElevationOverrideM: null,
  ...PRESETS["base-to-handheld"],
};

export function applyPreset(
  current: RadioParameters,
  preset: ScenarioPreset,
): RadioParameters {
  return { ...current, ...PRESETS[preset], preset };
}

export function switchBand(current: RadioParameters, band: Band): RadioParameters {
  return {
    ...current,
    band,
    frequencyMhz: band === "vhf144" ? 145 : 435,
  };
}

export function convertPowerUnit(
  value: number,
  from: PowerUnit,
  to: PowerUnit,
): number {
  if (from === to || !Number.isFinite(value)) return value;
  if (to === "dbm") return 10 * Math.log10(value * 1000);
  return 10 ** (value / 10) / 1000;
}

export function convertGainUnit(
  value: number,
  from: GainUnit,
  to: GainUnit,
): number {
  if (from === to || !Number.isFinite(value)) return value;
  return to === "dbi" ? value + 2.15 : value - 2.15;
}

function powerWatts(parameters: RadioParameters): number {
  return parameters.powerUnit === "watt"
    ? parameters.powerValue
    : convertPowerUnit(parameters.powerValue, "dbm", "watt");
}

function gainDbi(value: number, unit: GainUnit): number {
  return unit === "dbi" ? value : convertGainUnit(value, "dbd", "dbi");
}

export function parameterValidationMessage(parameters: RadioParameters): string | null {
  const frequencyRange = parameters.band === "vhf144" ? [144, 148] : [430, 440];
  if (
    !Number.isFinite(parameters.frequencyMhz) ||
    parameters.frequencyMhz < frequencyRange[0] ||
    parameters.frequencyMhz > frequencyRange[1] ||
    Math.abs(parameters.frequencyMhz * 100 - Math.round(parameters.frequencyMhz * 100)) >
      1e-8
  ) {
    return `频率应为 ${frequencyRange[0].toFixed(2)}–${frequencyRange[1].toFixed(2)} MHz，最多两位小数`;
  }
  if (!Number.isFinite(powerWatts(parameters)) || powerWatts(parameters) < 0.1 || powerWatts(parameters) > 1000) {
    return "发射功率应等效于 0.1–1000 W";
  }
  for (const [label, value] of [
    ["发射天线增益", gainDbi(parameters.txGainValue, parameters.txGainUnit)],
    ["接收天线增益", gainDbi(parameters.rxGainValue, parameters.rxGainUnit)],
  ] as const) {
    if (!Number.isFinite(value) || value < -20 || value > 30) {
      return `${label}换算后应为 -20–30 dBi`;
    }
  }
  for (const [label, value] of [
    ["发射天线高度", parameters.txHeightM],
    ["接收天线高度", parameters.rxHeightM],
  ] as const) {
    if (!Number.isFinite(value) || value < 0.5 || value > 500) {
      return `${label}应为 0.5–500 m`;
    }
  }
  if (
    parameters.txGroundElevationOverrideM !== null &&
    (!Number.isFinite(parameters.txGroundElevationOverrideM) ||
      parameters.txGroundElevationOverrideM < -500 ||
      parameters.txGroundElevationOverrideM > 9000)
  ) {
    return "发射点地面海拔覆盖应为 -500–9000 m AMSL";
  }
  return null;
}

