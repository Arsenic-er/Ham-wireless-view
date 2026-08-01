#!/usr/bin/env python3
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0

"""Validate localized README structure and language-independent project facts."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "docs" / "readme-facts.json"
LINK_RE = re.compile(r"(?<!!)\[[^\]\n]+\]\(([^)\n]+)\)")
COMMAND_BLOCK_RE = re.compile(r"```(bash|powershell)\n(.*?)\n```", re.DOTALL)


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def local_link_target(raw_destination: str) -> str | None:
    destination = raw_destination.strip()
    if destination.startswith("<") and destination.endswith(">"):
        destination = destination[1:-1]
    if " " in destination:
        destination = destination.split(" ", 1)[0]
    parsed = urlsplit(destination)
    if parsed.scheme or parsed.netloc or not parsed.path:
        return None
    return unquote(parsed.path)


def main() -> int:
    errors: list[str] = []
    try:
        manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        print(f"README locale check failed: cannot read {MANIFEST_PATH}: {error}", file=sys.stderr)
        return 1

    if manifest.get("schema_version") != 1:
        fail(errors, "docs/readme-facts.json: unsupported schema_version")

    locale_specs = manifest.get("locales", {})
    expected_files = {spec["file"] for spec in locale_specs.values()}
    actual_files = {path.name for path in ROOT.glob("README*.md")}
    if actual_files != expected_files:
        fail(
            errors,
            "localized README set differs: "
            f"expected {sorted(expected_files)}, found {sorted(actual_files)}",
        )

    canonical = manifest.get("canonical")
    if canonical not in expected_files:
        fail(errors, f"canonical README {canonical!r} is not a configured locale")

    sections = manifest.get("section_markers", [])
    required_literals = manifest.get("required_literals", [])
    forbidden_literals = manifest.get("forbidden_literals", [])
    canonical_commands: list[tuple[str, str]] | None = None

    for locale, spec in locale_specs.items():
        relative_path = spec["file"]
        path = ROOT / relative_path
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as error:
            fail(errors, f"{relative_path}: cannot read as UTF-8: {error}")
            continue

        if not text.startswith("# HamHeatmap\n"):
            fail(errors, f"{relative_path}: first heading must be '# HamHeatmap'")

        navigation = spec["navigation"]
        if text.count(navigation) != 1 or navigation not in text.splitlines()[:12]:
            fail(errors, f"{relative_path}: missing or duplicate top language navigation")

        locale_marker = f"<!-- locale: {locale} -->"
        if text.count(locale_marker) != 1:
            fail(errors, f"{relative_path}: expected exactly one {locale_marker}")
        if text.count("<!-- canonical: README.md -->") != 1:
            fail(errors, f"{relative_path}: canonical README marker is missing or duplicated")

        last_position = -1
        for section in sections:
            marker = f"<!-- section:{section} -->"
            if text.count(marker) != 1:
                fail(errors, f"{relative_path}: expected exactly one {marker}")
                continue
            position = text.index(marker)
            if position <= last_position:
                fail(errors, f"{relative_path}: section marker {marker} is out of order")
            last_position = position

        for literal in required_literals:
            if literal not in text:
                fail(errors, f"{relative_path}: missing required fact {literal!r}")
        for literal in forbidden_literals:
            if literal in text:
                fail(errors, f"{relative_path}: contains obsolete text {literal!r}")

        commands = COMMAND_BLOCK_RE.findall(text)
        if canonical_commands is None:
            canonical_commands = commands
        elif commands != canonical_commands:
            fail(errors, f"{relative_path}: bash/PowerShell command blocks drifted from README.md")

        for match in LINK_RE.finditer(text):
            target = local_link_target(match.group(1))
            if target is None:
                continue
            resolved = (path.parent / target).resolve()
            try:
                resolved.relative_to(ROOT.resolve())
            except ValueError:
                fail(errors, f"{relative_path}: local link escapes repository: {target}")
                continue
            if not resolved.exists():
                fail(errors, f"{relative_path}: broken local link: {target}")

    if canonical_commands is not None and not canonical_commands:
        fail(errors, "README.md: no bash/PowerShell command blocks found")

    if errors:
        print("README locale check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        "README locale check passed: "
        f"{len(locale_specs)} locales, {len(sections)} section markers, "
        f"{len(required_literals)} synchronized facts."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
