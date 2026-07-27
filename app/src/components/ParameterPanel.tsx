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
          aria-label={`${label}单位`}
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
          <span className="eyebrow">传播参数</span>
          <h2>配置电台</h2>
        </div>
        <span className="model-pill">ITM P2P</span>
      </div>

      <Segmented<ScenarioPreset>
        label="场景预设"
        value={parameters.preset}
        disabled={disabled}
        options={[
          { value: "base-to-handheld", label: "基地台 → 手台", detail: "25 W · 20 m" },
          { value: "handheld-to-base", label: "手台 → 基地台", detail: "5 W · 1.5 m" },
        ]}
        onChange={(preset) => onChange(applyPreset(parameters, preset))}
      />

      <div className="panel-section">
        <div className="section-title">
          <span>01</span>
          <strong>频率与极化</strong>
        </div>
        <Segmented<Band>
          label="频段"
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
          label="具体频率"
          value={parameters.frequencyMhz}
          min={parameters.band === "vhf144" ? 144 : 430}
          max={parameters.band === "vhf144" ? 148 : 440}
          step={0.01}
          suffix="MHz"
          disabled={disabled}
          onChange={(frequencyMhz) => onChange({ ...parameters, frequencyMhz })}
        />
        <Segmented
          label="极化方式"
          value={parameters.polarization}
          disabled={disabled}
          options={[
            { value: "vertical", label: "垂直" },
            { value: "horizontal", label: "水平" },
          ]}
          onChange={(polarization) => onChange({ ...parameters, polarization })}
        />
      </div>

      <div className="panel-section">
        <div className="section-title">
          <span>02</span>
          <strong>发射站</strong>
        </div>
        <label className="field" htmlFor="power">
          <span>发射功率</span>
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
              aria-label="发射功率单位"
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
          label="发射天线增益"
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
          label="发射天线高度 AGL"
          value={parameters.txHeightM}
          min={0.5}
          max={500}
          step={0.1}
          suffix="m"
          disabled={disabled}
          onChange={(txHeightM) => onChange({ ...parameters, txHeightM })}
        />
        <Segmented<"dem" | "manual">
          label="发射点地面高程来源"
          value={groundElevationMode}
          disabled={disabled || (groundElevationMode === "dem" && elevationM === null)}
          options={[
            { value: "dem", label: "DEM 自动" },
            { value: "manual", label: "手动覆盖" },
          ]}
          onChange={(mode) => {
            if (mode === groundElevationMode) return;
            if (mode === "manual" && elevationM === null) return;
            onChange({
              ...parameters,
              txGroundElevationOverrideM:
                mode === "manual" ? (elevationM ?? 0) : null,
            });
          }}
        />
        {groundElevationMode === "manual" && (
          <NumberField
            id="tx-ground-elevation"
            label="手动地面海拔 AMSL"
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
          <span>DEM 参考海拔</span>
          <strong>{elevationM === null ? "选择点后读取" : `${elevationM.toFixed(1)} m AMSL`}</strong>
        </div>
        <div className="readonly-field">
          <span>有效发射天线海拔</span>
          <strong>
            {effectiveAntennaElevationM === null
              ? "选择点后读取"
              : `${effectiveAntennaElevationM.toFixed(1)} m AMSL`}
          </strong>
        </div>
        <p className="field-help">
          手动覆盖只替换发射点地面高程；发射天线高度始终按离地高度 AGL 计算。
        </p>
      </div>

      <div className="panel-section">
        <div className="section-title">
          <span>03</span>
          <strong>接收端</strong>
        </div>
        <GainField
          id="rx-gain"
          label="接收天线增益"
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
          label="接收天线高度 AGL"
          value={parameters.rxHeightM}
          min={0.5}
          max={500}
          step={0.1}
          suffix="m"
          disabled={disabled}
          onChange={(rxHeightM) => onChange({ ...parameters, rxHeightM })}
        />
        <div className="readonly-field">
          <span>接收点海拔</span>
          <strong>逐像素读取 DEM</strong>
        </div>
      </div>

      {validation && <p className="validation-message">{validation}</p>}
    </div>
  );
}
