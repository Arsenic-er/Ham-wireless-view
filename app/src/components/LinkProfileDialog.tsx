// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import type { LinkAnalysisResult } from "../lib/types";
import { LinkProfileChart } from "./LinkProfileChart";

interface LinkProfileDialogProps {
  result: LinkAnalysisResult;
  dimmed: boolean;
  onActivate: () => void;
  onInteractOutside: () => void;
  onClose: () => void;
}

export function LinkProfileDialog({
  result,
  dimmed,
  onActivate,
  onInteractOutside,
  onClose,
}: LinkProfileDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    dialogRef.current?.focus({ preventScroll: true });
  }, [result]);

  useEffect(() => {
    function handlePointerDown(event: PointerEvent) {
      const dialog = dialogRef.current;
      const target = event.target;
      if (!dialog || !(target instanceof Node)) return;
      if (dialog.contains(target)) onActivate();
      else onInteractOutside();
    }

    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => document.removeEventListener("pointerdown", handlePointerDown, true);
  }, [onActivate, onInteractOutside]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <section
      ref={dialogRef}
      className={`link-profile-dialog${dimmed ? " is-dimmed" : ""}`}
      role="dialog"
      aria-modal="false"
      aria-labelledby="link-profile-dialog-title"
      aria-describedby="link-profile-dialog-hint"
      tabIndex={-1}
    >
      <header className="link-profile-dialog-heading">
        <div>
          <span className="eyebrow">{t("linkProfileDialogEyebrow")}</span>
          <h2 id="link-profile-dialog-title">{t("linkProfileDialogTitle")}</h2>
          <p id="link-profile-dialog-hint">{t("linkProfileDialogHint")}</p>
        </div>
        <button type="button" aria-label={t("closeLinkProfile")} onClick={onClose}>
          ×
        </button>
      </header>
      <div className="link-profile-dialog-body">
        <LinkProfileChart result={result} />
      </div>
    </section>
  );
}
