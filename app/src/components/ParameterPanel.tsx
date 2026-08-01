// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from "react-i18next";

import {
  applyPreset,
  convertGainUnit,
  convertPowerUnit,
  parameterValidationMessage,
  switchBand,
} from "../lib/parameters";
import type {
  Band,
  GainUnit,
  PowerUnit,
  RadioParameters,
  ScenarioPreset,
} from "../lib/types";

interface ParameterPanelProps {
  parameters: RadioParameters;
  disabled: boolean;
  elevationM: number | null;
  onChange: (parameters: RadioParameters) => void;
}

interface NumberFieldProps {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  suffix?: string;
  disabled: boolean;
  onChange: (value: number) => void;
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
}: NumberFieldProps) {
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
          onChange={(event) => onChange(Number(event.target.value))}
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

function GainField({
  id,
  label,
  value,
  unit,
  disabled,
  onValueChange,
  onUnitChange,
}: {
  id: string;
  label: string;
  value: number;
  unit: GainUnit;
  disabled: boolean;
  onValueChange: (value: number) => void;
  onUnitChange: (unit: GainUnit, converted: number) => void;
}) {
  const { t } = useTranslation();
  return (
    <label className="field" htmlFor={id}>
      <span>{label}</span>
      <span className="compound-input">
        <input
          id={id}
          type="number"
          min={-22.15}
          max={30}
          step={0.1}
          value={Number.isFinite(value) ? value : ""}
          disabled={disabled}
          onChange={(event) => onValueChange(Number(event.target.value))}
        />
        <select
          aria-label={t("unitLabel", { label })}
          value={unit}
          disabled={disabled}
          onChange={(event) => {
            const next = event.target.value as GainUnit;
            onUnitChange(next, Number(convertGainUnit(value, unit, next).toFixed(2)));
          }}
        >
          <option value="dbi">dBi</option>
          <option value="dbd">dBd</option>
        </select>
      </span>
    </label>
  );
}

export function ParameterPanel({
  parameters,
  disabled,
  elevationM,
  onChange,
}: ParameterPanelProps) {
  const { t } = useTranslation();
  const validation = parameterValidationMessage(parameters);
  const groundElevationMode = parameters.txGroundElevationOverrideM === null ? "dem" : "manual";
  const effectiveGroundElevationM = parameters.txGroundElevationOverrideM ?? elevationM;
  const effectiveAntennaElevationM =
    effectiveGroundElevationM !== null && Number.isFinite(effectiveGroundElevationM)
      ? effectiveGroundElevationM + parameters.txHeightM
      : null;
  return (
    <div className="parameter-panel">
      <div className="panel-heading">
        <div>
          <span className="eyebrow">{t("propagationParameters")}</span>
          <h2>{t("configureStation")}</h2>
        </div>
        <span className="model-pill">ITM P2P</span>
      </div>

      <Segmented<ScenarioPreset>
        label={t("scenarioPreset")}
        value={parameters.preset}
        disabled={disabled}
        options={[
          { value: "base-to-handheld", label: t("baseToHandheld"), detail: "25 W · 20 m" },
          { value: "handheld-to-base", label: t("handheldToBase"), detail: "5 W · 1.5 m" },
        ]}
        onChange={(preset) => onChange(applyPreset(parameters, preset))}
      />

      <div className="panel-section">
        <div className="section-title">
          <span>01</span>
          <strong>{t("frequencyPolarization")}</strong>
        </div>
        <Segmented<Band>
          label={t("band")}
          value={parameters.band}
          disabled={disabled}
          options={[
            { value: "vhf144", label: "144 MHz", detail: "VHF" },
            { value: "uhf430", label: "430 MHz", detail: "UHF" },
          ]}
          onChange={(band) => onChange(switchBand(parameters, band))}
        />
        <NumberField
          id="frequency"
          label={t("exactFrequency")}
          value={parameters.frequencyMhz}
          min={parameters.band === "vhf144" ? 144 : 430}
          max={parameters.band === "vhf144" ? 148 : 440}
          step={0.01}
          suffix="MHz"
          disabled={disabled}
          onChange={(frequencyMhz) => onChange({ ...parameters, frequencyMhz })}
        />
        <Segmented
          label={t("polarization")}
          value={parameters.polarization}
          disabled={disabled}
          options={[
            { value: "vertical", label: t("vertical") },
            { value: "horizontal", label: t("horizontal") },
          ]}
          onChange={(polarization) => onChange({ ...parameters, polarization })}
        />
      </div>

      <div className="panel-section">
        <div className="section-title">
          <span>02</span>
          <strong>{t("transmitterSection")}</strong>
        </div>
        <label className="field" htmlFor="power">
          <span>{t("transmitPower")}</span>
          <span className="compound-input">
            <input
              id="power"
              type="number"
              min={parameters.powerUnit === "watt" ? 0.1 : 20}
              max={parameters.powerUnit === "watt" ? 1000 : 60}
              step={parameters.powerUnit === "watt" ? 0.1 : 0.01}
              value={Number.isFinite(parameters.powerValue) ? parameters.powerValue : ""}
              disabled={disabled}
              onChange={(event) =>
                onChange({ ...parameters, powerValue: Number(event.target.value) })
              }
            />
            <select
              aria-label={t("unitLabel", { label: t("transmitPower") })}
              value={parameters.powerUnit}
              disabled={disabled}
              onChange={(event) => {
                const powerUnit = event.target.value as PowerUnit;
                onChange({
                  ...parameters,
                  powerUnit,
                  powerValue: Number(
                    convertPowerUnit(
                      parameters.powerValue,
                      parameters.powerUnit,
                      powerUnit,
                    ).toFixed(powerUnit === "watt" ? 3 : 2),
                  ),
                });
              }}
            >
              <option value="watt">W</option>
              <option value="dbm">dBm</option>
            </select>
          </span>
        </label>
        <GainField
          id="tx-gain"
          label={t("transmitGain")}
          value={parameters.txGainValue}
          unit={parameters.txGainUnit}
          disabled={disabled}
          onValueChange={(txGainValue) => onChange({ ...parameters, txGainValue })}
          onUnitChange={(txGainUnit, txGainValue) =>
            onChange({ ...parameters, txGainUnit, txGainValue })
          }
        />
        <NumberField
          id="tx-height"
          label={t("transmitHeight")}
          value={parameters.txHeightM}
          min={0.5}
          max={500}
          step={0.1}
          suffix="m"
          disabled={disabled}
          onChange={(txHeightM) => onChange({ ...parameters, txHeightM })}
        />
        <Segmented<"dem" | "manual">
          label={t("groundSource")}
          value={groundElevationMode}
          disabled={disabled || (groundElevationMode === "dem" && elevationM === null)}
          options={[
            { value: "dem", label: t("demAutomatic") },
            { value: "manual", label: t("manualOverride") },
          ]}
          onChange={(mode) => {
            if (mode === groundElevationMode) return;
            if (mode === "manual" && elevationM === null) return;
            onChange({
              ...parameters,
              txGroundElevationOverrideM:
                mode === "manual" ? elevationM : null,
            });
          }}
        />
        {groundElevationMode === "manual" && (
          <NumberField
            id="tx-ground-elevation"
            label={t("manualGround")}
            value={parameters.txGroundElevationOverrideM ?? 0}
            min={-500}
            max={9000}
            step={0.1}
            suffix="m"
            disabled={disabled}
            onChange={(txGroundElevationOverrideM) =>
              onChange({ ...parameters, txGroundElevationOverrideM })
            }
          />
        )}
        <div className="readonly-field">
          <span>{t("demReference")}</span>
          <strong>{elevationM === null ? t("selectPointFirst") : `${elevationM.toFixed(1)} m AMSL`}</strong>
        </div>
        <div className="readonly-field">
          <span>{t("effectiveTxElevation")}</span>
          <strong>
            {effectiveAntennaElevationM === null
              ? t("selectPointFirst")
              : `${effectiveAntennaElevationM.toFixed(1)} m AMSL`}
          </strong>
        </div>
        <p className="field-help">
          {t("groundHelp")}
        </p>
      </div>

      <div className="panel-section">
        <div className="section-title">
          <span>03</span>
          <strong>{t("receiverSection")}</strong>
        </div>
        <GainField
          id="rx-gain"
          label={t("receiveGain")}
          value={parameters.rxGainValue}
          unit={parameters.rxGainUnit}
          disabled={disabled}
          onValueChange={(rxGainValue) => onChange({ ...parameters, rxGainValue })}
          onUnitChange={(rxGainUnit, rxGainValue) =>
            onChange({ ...parameters, rxGainUnit, rxGainValue })
          }
        />
        <NumberField
          id="rx-height"
          label={t("receiveHeight")}
          value={parameters.rxHeightM}
          min={0.5}
          max={500}
          step={0.1}
          suffix="m"
          disabled={disabled}
          onChange={(rxHeightM) => onChange({ ...parameters, rxHeightM })}
        />
        <div className="readonly-field">
          <span>{t("receiverElevation")}</span>
          <strong>{t("perPixelDem")}</strong>
        </div>
      </div>

      {validation && <p className="validation-message">{validation}</p>}
    </div>
  );
}
