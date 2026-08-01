// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DEFAULT_PARAMETERS } from "../lib/parameters";
import type { RadioParameters } from "../lib/types";
import { ParameterPanel } from "./ParameterPanel";

afterEach(cleanup);

function Harness() {
  const [parameters, setParameters] = useState<RadioParameters>(DEFAULT_PARAMETERS);
  return (
    <ParameterPanel
      parameters={parameters}
      disabled={false}
      elevationM={512}
      onChange={setParameters}
    />
  );
}

describe("transmitter ground elevation controls", () => {
  it("keeps DEM visible while switching between automatic and manual ground elevation", () => {
    render(<Harness />);

    expect(screen.getByText("512.0 m AMSL")).toBeTruthy();
    expect(
      screen.queryByRole("spinbutton", { name: /手动地面海拔 AMSL/ }),
    ).toBeNull();
    expect(screen.getByText("532.0 m AMSL")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "手动覆盖" }));
    const input = screen.getByRole("spinbutton", {
      name: /手动地面海拔 AMSL/,
    }) as HTMLInputElement;
    expect(input.value).toBe("512");

    fireEvent.change(input, { target: { value: "-500" } });
    expect(screen.getByText("-480.0 m AMSL")).toBeTruthy();
    expect(screen.getByText(/发射天线高度始终按离地高度 AGL/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "DEM 自动" }));
    expect(
      screen.queryByRole("spinbutton", { name: /手动地面海拔 AMSL/ }),
    ).toBeNull();
    expect(screen.getByText("532.0 m AMSL")).toBeTruthy();
  });

  it("does not materialize a zero-metre override before DEM inspection completes", () => {
    const onChange = vi.fn();
    const { getByRole } = render(
      <ParameterPanel
        parameters={DEFAULT_PARAMETERS}
        disabled={false}
        elevationM={null}
        onChange={onChange}
      />,
    );

    const manualButton = getByRole("button", {
      name: /\u624b\u52a8\u8986\u76d6/,
    }) as HTMLButtonElement;
    const sourceFieldset = manualButton.closest(
      "fieldset",
    ) as HTMLFieldSetElement;
    expect(sourceFieldset.disabled).toBe(true);
    fireEvent.click(manualButton);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("preserves a real zero-metre DEM value when manual mode is selected", () => {
    const onChange = vi.fn();
    render(
      <ParameterPanel
        parameters={DEFAULT_PARAMETERS}
        disabled={false}
        elevationM={0}
        onChange={onChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "手动覆盖" }));

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        txGroundElevationOverrideM: 0,
      }),
    );
  });
});
