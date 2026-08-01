// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from "react-i18next";

import {
  linkParameterValidationMessage,
  switchLinkBand,
} from "../lib/linkParameters";
import { convertGainUnit, convertPowerUnit } from "../lib/parameters";
import type {
  Band,
  GainUnit,
  LinkEndpointParameters,
  LinkParameters,
  Polarization,
  PowerUnit,
} from "../lib/types";

interface LinkParameterPanelProps {
  parameters: LinkParameters;
  disabled: boolean;
  onChange: (parameters: LinkParameters) => void;
}

function NumberField({
  id,
  label,
  value,
  min,
  max,
  step,
  suffix,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  suffix?: string;
  disabled: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <label className="field" htmlFor={id}>
      <span>{label}</span>
      <span className="number-input-wrap">
        <input
          id={id}
          type="number"
          value={Number.isFinite(value) ? value : ""}
          min={min}
          max={max}
          step={step}
          disabled={disabled}
          onChange={(event) => onChange(Number(event.currentTarget.value))}
        />
        {suffix && <em>{suffix}</em>}
      </span>
    </label>
  );
}

function Segmented<T extends string>({
  label,
  value,
  options,
  disabled,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string; detail?: string }[];
  disabled: boolean;
  onChange: (value: T) => void;
}) {
  return (
    <fieldset className="segmented-field" disabled={disabled}>
      <legend>{label}</legend>
      <div className="segmented">
        {options.map((option) => (
          <button
            type="button"
            key={option.value}
            className={value === option.value ? "active" : ""}
            aria-pressed={value === option.value}
            onClick={() => onChange(option.value)}
          >
            <span>{option.label}</span>
            {option.detail && <small>{option.detail}</small>}
          </button>
        ))}
      </div>
    </fieldset>
  );
}

function EndpointFields({
  prefix,
  title,
  values,
  disabled,
  onChange,
}: {
  prefix: "tx" | "rx";
  title: string;
  values: LinkEndpointParameters;
  disabled: boolean;
  onChange: (values: LinkEndpointParameters) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="panel-section">
      <div className="section-title">
        <span>{prefix === "tx" ? "02" : "03"}</span>
        <strong>{title}</strong>
      </div>
      <NumberField
        id={`link-${prefix}-height`}
        label={t("linkAntennaHeight")}
        value={values.antennaHeightM}
        min={0.5}
        max={500}
        step={0.1}
        suffix="m"
        disabled={disabled}
        onChange={(antennaHeightM) => onChange({ ...values, antennaHeightM })}
      />
      <label className="field" htmlFor={`link-${prefix}-gain`}>
        <span>{t("linkAntennaGain")}</span>
        <span className="compound-input">
          <input
            id={`link-${prefix}-gain`}
            type="number"
            min={-22.15}
            max={30}
            step={0.1}
            value={Number.isFinite(values.antennaGainValue) ? values.antennaGainValue : ""}
            disabled={disabled}
            onChange={(event) =>
              onChange({ ...values, antennaGainValue: Number(event.currentTarget.value) })
            }
          />
          <select
            aria-label={t("unitLabel", { label: t("linkAntennaGain") })}
            value={values.antennaGainUnit}
            disabled={disabled}
            onChange={(event) => {
              const antennaGainUnit = event.currentTarget.value as GainUnit;
              onChange({
                ...values,
                antennaGainUnit,
                antennaGainValue: Number(
                  convertGainUnit(
                    values.antennaGainValue,
                    values.antennaGainUnit,
                    antennaGainUnit,
                  ).toFixed(2),
                ),
              });
            }}
          >
            <option value="dbi">dBi</option>
            <option value="dbd">dBd</option>
          </select>
        </span>
      </label>
      <Segmented<Polarization>
        label={t("polarization")}
        value={values.polarization}
        disabled={disabled}
        options={[
          { value: "vertical", label: t("vertical") },
          { value: "horizontal", label: t("horizontal") },
        ]}
        onChange={(polarization) => onChange({ ...values, polarization })}
      />
    </div>
  );
}

export function LinkParameterPanel({
  parameters,
  disabled,
  onChange,
}: LinkParameterPanelProps) {
  const { t } = useTranslation();
  const validation = linkParameterValidationMessage(parameters);
  return (
    <div className="parameter-panel link-parameter-panel">
      <div className="panel-heading">
        <div>
          <span className="eyebrow">{t("linkAnalysis")}</span>
          <h2>{t("configureLink")}</h2>
        </div>
        <span className="model-pill">ITM P2P</span>
      </div>

      <div className="panel-section">
        <div className="section-title">
          <span>01</span>
          <strong>{t("linkRadioBudget")}</strong>
        </div>
        <Segmented<Band>
          label={t("band")}
          value={parameters.band}
          disabled={disabled}
          options={[
            { value: "vhf144", label: "144 MHz", detail: "VHF" },
            { value: "uhf430", label: "430 MHz", detail: "UHF" },
          ]}
          onChange={(band) => onChange(switchLinkBand(parameters, band))}
        />
        <NumberField
          id="link-frequency"
          label={t("exactFrequency")}
          value={parameters.frequencyMhz}
          min={parameters.band === "vhf144" ? 144 : 430}
          max={parameters.band === "vhf144" ? 148 : 440}
          step={0.01}
          suffix="MHz"
          disabled={disabled}
          onChange={(frequencyMhz) => onChange({ ...parameters, frequencyMhz })}
        />
        <label className="field" htmlFor="link-tx-power">
          <span>{t("transmitPower")}</span>
          <span className="compound-input">
            <input
              id="link-tx-power"
              type="number"
              min={parameters.txPowerUnit === "watt" ? 0.1 : 20}
              max={parameters.txPowerUnit === "watt" ? 1000 : 60}
              step={parameters.txPowerUnit === "watt" ? 0.1 : 0.01}
              value={Number.isFinite(parameters.txPowerValue) ? parameters.txPowerValue : ""}
              disabled={disabled}
              onChange={(event) =>
                onChange({ ...parameters, txPowerValue: Number(event.currentTarget.value) })
              }
            />
            <select
              aria-label={t("unitLabel", { label: t("transmitPower") })}
              value={parameters.txPowerUnit}
              disabled={disabled}
              onChange={(event) => {
                const txPowerUnit = event.currentTarget.value as PowerUnit;
                onChange({
                  ...parameters,
                  txPowerUnit,
                  txPowerValue: Number(
                    convertPowerUnit(
                      parameters.txPowerValue,
                      parameters.txPowerUnit,
                      txPowerUnit,
                    ).toFixed(txPowerUnit === "watt" ? 3 : 2),
                  ),
                });
              }}
            >
              <option value="watt">W</option>
              <option value="dbm">dBm</option>
            </select>
          </span>
        </label>
        <NumberField
          id="link-receiver-threshold"
          label={t("receiverThreshold")}
          value={parameters.receiverThresholdDbm}
          min={-160}
          max={-40}
          step={1}
          suffix="dBm"
          disabled={disabled}
          onChange={(receiverThresholdDbm) =>
            onChange({ ...parameters, receiverThresholdDbm })
          }
        />
        <p className="field-help">{t("receiverThresholdHelp")}</p>
      </div>

      <EndpointFields
        prefix="tx"
        title={t("linkTx")}
        values={parameters.tx}
        disabled={disabled}
        onChange={(tx) => onChange({ ...parameters, tx })}
      />
      <EndpointFields
        prefix="rx"
        title={t("linkRx")}
        values={parameters.rx}
        disabled={disabled}
        onChange={(rx) => onChange({ ...parameters, rx })}
      />

      {parameters.tx.polarization !== parameters.rx.polarization && (
        <p className="validation-message">{t("polarizationMismatchNotice")}</p>
      )}
      {validation && <p className="validation-message">{validation}</p>}
    </div>
  );
}
