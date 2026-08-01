// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import i18n from "../i18n";
import { convertGainUnit, convertPowerUnit } from "./parameters";
import type {
  LinkAnalysisRequest,
  LinkParameters,
  MapPoint,
} from "./types";

export const DEFAULT_LINK_PARAMETERS: LinkParameters = {
  band: "vhf144",
  frequencyMhz: 145,
  txPowerValue: 25,
  txPowerUnit: "watt",
  receiverThresholdDbm: -120,
  tx: {
    antennaHeightM: 20,
    antennaGainValue: 6,
    antennaGainUnit: "dbi",
    polarization: "vertical",
  },
  rx: {
    antennaHeightM: 1.5,
    antennaGainValue: -3,
    antennaGainUnit: "dbi",
    polarization: "vertical",
  },
};

export function switchLinkBand(
  parameters: LinkParameters,
  band: LinkParameters["band"],
): LinkParameters {
  return {
    ...parameters,
    band,
    frequencyMhz: band === "vhf144" ? 145 : 435,
  };
}

function gainDbi(value: number, unit: LinkParameters["tx"]["antennaGainUnit"]): number {
  return unit === "dbi" ? value : convertGainUnit(value, "dbd", "dbi");
}

export function linkParameterValidationMessage(
  parameters: LinkParameters,
): string | null {
  const range = parameters.band === "vhf144" ? [144, 148] : [430, 440];
  if (
    !Number.isFinite(parameters.frequencyMhz) ||
    parameters.frequencyMhz < range[0] ||
    parameters.frequencyMhz > range[1] ||
    Math.abs(parameters.frequencyMhz * 100 - Math.round(parameters.frequencyMhz * 100)) > 1e-8
  ) {
    return i18n.t("validationFrequency", {
      min: range[0].toFixed(2),
      max: range[1].toFixed(2),
    });
  }
  const watts =
    parameters.txPowerUnit === "watt"
      ? parameters.txPowerValue
      : convertPowerUnit(parameters.txPowerValue, "dbm", "watt");
  if (!Number.isFinite(watts) || watts < 0.1 || watts > 1000) {
    return i18n.t("validationPower");
  }
  if (
    !Number.isFinite(parameters.receiverThresholdDbm) ||
    parameters.receiverThresholdDbm < -160 ||
    parameters.receiverThresholdDbm > -40
  ) {
    return i18n.t("validationReceiverThreshold");
  }
  for (const [label, endpoint] of [
    [i18n.t("linkTx"), parameters.tx],
    [i18n.t("linkRx"), parameters.rx],
  ] as const) {
    const gain = gainDbi(endpoint.antennaGainValue, endpoint.antennaGainUnit);
    if (!Number.isFinite(gain) || gain < -20 || gain > 30) {
      return i18n.t("validationGain", { label });
    }
    if (
      !Number.isFinite(endpoint.antennaHeightM) ||
      endpoint.antennaHeightM < 0.5 ||
      endpoint.antennaHeightM > 500
    ) {
      return i18n.t("validationHeight", { label });
    }
  }
  return null;
}

export function buildLinkAnalysisRequest(
  txPoint: MapPoint,
  rxPoint: MapPoint,
  parameters: LinkParameters,
): LinkAnalysisRequest {
  const endpoint = (
    point: MapPoint,
    values: LinkParameters["tx"],
  ): LinkAnalysisRequest["tx"] => ({
    point,
    antennaHeightM: values.antennaHeightM,
    antennaGainValue: values.antennaGainValue,
    antennaGainUnit: values.antennaGainUnit,
    polarization: values.polarization,
  });
  return {
    tx: endpoint(txPoint, parameters.tx),
    rx: endpoint(rxPoint, parameters.rx),
    band: parameters.band === "vhf144" ? "vhf-144" : "uhf-430",
    frequencyMhz: parameters.frequencyMhz,
    txPowerValue: parameters.txPowerValue,
    txPowerUnit: parameters.txPowerUnit,
    receiverThresholdDbm: parameters.receiverThresholdDbm,
  };
}
