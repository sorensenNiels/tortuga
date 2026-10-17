#!/usr/bin/env python3
"""Renders the app icon (crates/tortuga-app/icons/tortuga.svg) to the PNG and ICO files next to it.

Needs `resvg` (`cargo binstall resvg`). Run `make icons` after changing the SVG and commit the results.
"""

import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ICONS = Path(__file__).resolve().parent.parent / "crates" / "tortuga-app" / "icons"
SVG = ICONS / "tortuga.svg"
PNG_SIZES = [32, 64, 128, 256, 512]
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


def render(size: int, out: Path) -> None:
    subprocess.run(["resvg", "-w", str(size), "-h", str(size), str(SVG), str(out)], check=True)


def write_ico(pngs: list[tuple[int, bytes]], out: Path) -> None:
    """An ICO file whose images are PNGs (supported since Windows Vista)."""
    header = struct.pack("<HHH", 0, 1, len(pngs))
    offset = len(header) + 16 * len(pngs)
    entries, data = b"", b""
    for size, png in pngs:
        dim = 0 if size >= 256 else size  # 0 means 256
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    out.write_bytes(header + entries + data)


def main() -> int:
    for size in PNG_SIZES:
        render(size, ICONS / f"{size}.png")
    with tempfile.TemporaryDirectory() as tmp:
        pngs = []
        for size in ICO_SIZES:
            path = Path(tmp) / f"{size}.png"
            render(size, path)
            pngs.append((size, path.read_bytes()))
    write_ico(pngs, ICONS / "tortuga.ico")
    print(f"Wrote {len(PNG_SIZES)} PNGs and tortuga.ico to {ICONS}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
