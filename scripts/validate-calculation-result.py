#!/usr/bin/env python3
"""Fail closed when an HTTP calculation result violates schema v4."""

from __future__ import annotations

import base64
import binascii
import json
import struct
import sys
from pathlib import Path
from typing import NoReturn

GRID_SIZE = 401
PIXEL_COUNT = GRID_SIZE * GRID_SIZE
PNG_PREFIX = "data:image/png;base64,"
FILTER_ENCODING = "u8-dbm-floor-v1"


def fail(message: str) -> NoReturn:
    raise SystemExit(f"calculation result contract failed: {message}")


def decode_base64(value: object, label: str) -> bytes:
    if not isinstance(value, str):
        fail(f"{label} was not a string")
    try:
        return base64.b64decode(value, validate=True)
    except (binascii.Error, ValueError) as error:
        fail(f"{label} was not valid base64: {error}")


def validate_png(value: object, label: str) -> int:
    if not isinstance(value, str) or not value.startswith(PNG_PREFIX):
        fail(f"{label} was not a PNG data URL")
    png = decode_base64(value[len(PNG_PREFIX) :], label)
    if (
        len(png) < 24
        or png[:8] != b"\x89PNG\r\n\x1a\n"
        or png[12:16] != b"IHDR"
    ):
        fail(f"{label} did not contain a PNG header")
    if struct.unpack(">II", png[16:24]) != (GRID_SIZE, GRID_SIZE):
        fail(f"{label} was not {GRID_SIZE}x{GRID_SIZE}")
    return len(png)


def object_without_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    value: dict[str, object] = {}
    for key, item in pairs:
        if key in value:
            fail(f"duplicate JSON field {key}")
        value[key] = item
    return value


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: validate-calculation-result.py RESULT.json")
    try:
        result = json.loads(
            Path(sys.argv[1]).read_text(encoding="utf-8"),
            object_pairs_hook=object_without_duplicate_keys,
        )
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"could not read JSON: {error}")
    if not isinstance(result, dict):
        fail("top-level value was not an object")
    if result.get("schemaVersion") != 4:
        fail("schemaVersion was not 4")

    for field in ("imageWidth", "imageHeight", "mapOverlayWidth", "mapOverlayHeight"):
        if result.get(field) != GRID_SIZE:
            fail(f"{field} was not {GRID_SIZE}")

    heatmap_bytes = validate_png(result.get("heatmapPngDataUrl"), "heatmapPngDataUrl")
    overlay_bytes = validate_png(
        result.get("mapOverlayPngDataUrl"), "mapOverlayPngDataUrl"
    )
    if result.get("mapOverlayFilterEncoding") != FILTER_ENCODING:
        fail(f"mapOverlayFilterEncoding was not {FILTER_ENCODING}")
    bins = decode_base64(
        result.get("mapOverlayFilterBase64"), "mapOverlayFilterBase64"
    )
    if len(bins) != PIXEL_COUNT:
        fail(
            "mapOverlayFilterBase64 decoded to "
            f"{len(bins)} bytes, expected {PIXEL_COUNT}"
        )
    invalid = next((index for index, value in enumerate(bins) if value > 81), None)
    if invalid is not None:
        fail(f"filter bin {invalid} was {bins[invalid]}, outside 0..81")
    print(
        "schema=4 "
        f"heatmapPngBytes={heatmap_bytes} overlayPngBytes={overlay_bytes} "
        f"filterEncoding={FILTER_ENCODING} filterBytes={len(bins)} "
        f"filterRange={min(bins)}..{max(bins)}"
    )


if __name__ == "__main__":
    main()
