#!/usr/bin/env python3
# Ham Wireless View
# Project creator and lead developer: Arsenic-er
# SPDX-FileCopyrightText: 2026 Arsenic-er
# SPDX-License-Identifier: Apache-2.0
"""Fail closed when tracked or pending repository files lack attribution policy."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROJECT = "Ham Wireless View"
CREATOR = "Project creator and lead developer: Arsenic-er"
COPYRIGHT = "SPDX-FileCopyrightText: 2026 Arsenic-er"
LICENSE = "SPDX-License-Identifier: Apache-2.0"
REQUIRED = (PROJECT, CREATOR, COPYRIGHT, LICENSE)

PROTECTED_EXACT = {
    "Cargo.lock",
    "app/src-tauri/Cargo.lock",
    "app/package-lock.json",
    "LICENSE",
    "THIRD_PARTY_LICENSES.md",
}
PROTECTED_PREFIXES = ("third_party/", "app/src-tauri/gen/")

INLINE_EXACT = {
    ".github/CODEOWNERS",
    ".github/workflows/core.yml",
    "Cargo.toml",
    "app/index.html",
    "app/src-tauri/Cargo.toml",
    "app/src-tauri/app-icon.svg",
    "app/src-tauri/build.rs",
    "app/vite.config.ts",
    "rust-toolchain.toml",
}
INLINE_SUFFIXES = {".rs", ".cpp", ".h", ".ts", ".tsx", ".css", ".sh", ".ps1", ".py", ".mjs", ".toml", ".yml", ".yaml", ".html", ".svg"}

EXTERNAL_EXACT = {
    ".gitignore",
    ".node-version",
    "AGENTS.md",
    "AUTHORS.md",
    "NOTICE",
    "app/package.json",
    "app/src-tauri/tauri.conf.json",
    "app/tsconfig.app.json",
    "app/tsconfig.json",
    "app/tsconfig.node.json",
    "docs/readme-facts.json",
}
EXTERNAL_PREFIXES = (
    "README",
    "docs/",
    "app/src-tauri/capabilities/",
    "app/src-tauri/icons/",
)


def repository_files() -> list[str]:
    completed = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    return sorted(
        value.decode("utf-8")
        for value in completed.stdout.split(b"\0")
        if value
    )


def is_inline(path: str) -> bool:
    # Classify by commentable source syntax before documentation/asset prefixes.
    # Protected third-party/generated paths are handled first by classify().
    return path in INLINE_EXACT or Path(path).suffix in INLINE_SUFFIXES


def classify(path: str) -> str:
    if path in PROTECTED_EXACT or path.startswith(PROTECTED_PREFIXES):
        return "protected"
    if is_inline(path):
        return "inline"
    if path in EXTERNAL_EXACT or path.startswith(EXTERNAL_PREFIXES):
        return "external"
    return "unclassified"


def comment_style(path: str) -> str:
    suffix = Path(path).suffix
    if suffix in {".rs", ".cpp", ".h", ".ts", ".tsx", ".mjs"}:
        return "slash"
    if suffix == ".css":
        return "css"
    if suffix in {".html", ".svg"}:
        return "markup"
    return "hash"


def header_lines(path: str) -> list[str]:
    style = comment_style(path)
    if style == "slash":
        return [f"// {line}" for line in REQUIRED]
    if style == "css":
        return ["/*", *(f" * {line}" for line in REQUIRED), " */"]
    if style == "markup":
        return ["<!--", *(f"  {line}" for line in REQUIRED), "-->"]
    return [f"# {line}" for line in REQUIRED]


def insertion_index(path: str, lines: list[str]) -> int:
    if not lines:
        return 0
    first = lines[0].lower()
    if first.startswith("#!"):
        return 1
    if path.endswith(".html") and first.startswith("<!doctype"):
        return 1
    if path.endswith(".svg") and first.startswith("<?xml"):
        return 1
    return 0


def has_header(path: Path) -> bool:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError):
        return False
    relative = path.relative_to(ROOT).as_posix()
    index = insertion_index(relative, lines)
    expected = header_lines(relative)
    return lines[index : index + len(expected)] == expected


def add_header(relative: str) -> bool:
    path = ROOT / relative
    if has_header(path):
        return False
    text = path.read_text(encoding="utf-8")
    if COPYRIGHT in text or LICENSE in text:
        raise ValueError(f"{relative}: partial SPDX header requires manual repair")
    newline = "\r\n" if "\r\n" in text else "\n"
    lines = text.splitlines()
    index = insertion_index(relative, lines)
    block = header_lines(relative)
    updated = [*lines[:index], *block, "", *lines[index:]]
    path.write_text(newline.join(updated) + newline, encoding="utf-8", newline="")
    return True


def central_errors() -> list[str]:
    errors: list[str] = []
    required_central = {
        "AUTHORS.md": (PROJECT, CREATOR, "https://github.com/Arsenic-er"),
        "NOTICE": (PROJECT, "Copyright 2026 Arsenic-er", CREATOR),
        ".github/CODEOWNERS": (PROJECT, CREATOR, "* @Arsenic-er"),
    }
    for relative, values in required_central.items():
        path = ROOT / relative
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as error:
            errors.append(f"{relative}: cannot read: {error}")
            continue
        for value in values:
            if value not in text:
                errors.append(f"{relative}: missing {value!r}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--fix",
        action="store_true",
        help="insert missing headers in known first-party commentable files",
    )
    args = parser.parse_args()

    files = repository_files()
    groups = {name: [] for name in ("inline", "external", "protected", "unclassified")}
    for relative in files:
        groups[classify(relative)].append(relative)

    fix_errors: list[str] = []
    changed = 0
    if args.fix:
        for relative in groups["inline"]:
            try:
                changed += int(add_header(relative))
            except (OSError, UnicodeError, ValueError) as error:
                fix_errors.append(str(error))

    errors = [*fix_errors, *central_errors()]
    missing = [
        relative
        for relative in groups["inline"]
        if not has_header(ROOT / relative)
    ]
    errors.extend(f"{relative}: missing complete attribution header" for relative in missing)

    protected_markers = tuple(value.encode("utf-8") for value in REQUIRED)
    wrongly_stamped: list[str] = []
    for relative in groups["protected"]:
        try:
            data = (ROOT / relative).read_bytes()
            if any(marker in data for marker in protected_markers):
                wrongly_stamped.append(relative)
        except OSError as error:
            errors.append(f"{relative}: cannot inspect protected file: {error}")
    errors.extend(
        f"{relative}: protected/generated/third-party file was stamped"
        for relative in wrongly_stamped
    )
    errors.extend(
        f"{relative}: tracked or pending file is not classified"
        for relative in groups["unclassified"]
    )

    print(f"inline first-party coverage: {len(groups['inline']) - len(missing)}/{len(groups['inline'])}")
    print(f"external/metadata classification: {len(groups['external'])}")
    print(f"protected/generated/third-party exclusions: {len(groups['protected'])}")
    print(f"unclassified repository files: {len(groups['unclassified'])}")
    if args.fix:
        print(f"headers inserted: {changed}")

    if errors:
        print("source attribution check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("source attribution check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
