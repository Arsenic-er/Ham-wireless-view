// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";

import { DEFAULT_LINK_PARAMETERS } from "../lib/linkParameters";
import type { LinkParameters } from "../lib/types";
import { LinkParameterPanel } from "./LinkParameterPanel";

afterEach(cleanup);

function Harness() {
  const [parameters, setParameters] =
    useState<LinkParameters>(DEFAULT_LINK_PARAMETERS);
  return (
    <>
      <LinkParameterPanel
        parameters={parameters}
        disabled={false}
        onChange={setParameters}
      />
      <output data-testid="link-values">
        {parameters.frequencyMhz}|{parameters.tx.polarization}|{parameters.rx.polarization}|
        {parameters.receiverThresholdDbm}
      </output>
    </>
  );
}

describe("LinkParameterPanel", () => {
  it("keeps endpoint polarization independent and switches band frequency", () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /430 MHz/ }));
    expect(screen.getByTestId("link-values").textContent).toContain("435|vertical|vertical");

    const horizontal = screen.getAllByRole("button", { name: "水平" });
    fireEvent.click(horizontal[0]);
    expect(screen.getByTestId("link-values").textContent).toContain("435|horizontal|vertical");
  });

  it("edits the receiver planning threshold", () => {
    render(<Harness />);
    const threshold = screen
      .getAllByRole<HTMLInputElement>("spinbutton")
      .find((input) => input.value === "-120");
    if (!threshold) throw new Error("missing receiver threshold input");
    fireEvent.change(threshold, { target: { value: "-110" } });
    expect(screen.getByTestId("link-values").textContent).toContain("|-110");
  });
});
