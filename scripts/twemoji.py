#!/usr/bin/env python3
"""Build RUNE's color emoji pack from a pinned Twemoji release.

Reads only the upstream 72x72 PNGs, unmodified, and Unicode's emoji-data.txt
for the default presentation of single code points. Writes:

  assets/twemoji/pngs.bin   the PNGs back to back, in index order
  assets/twemoji/index.tsv  sequence, offset, length, presentation
  assets/licenses/Twemoji-CC-BY-4.0.txt  the graphics license, as shipped

Standard library only; the output is byte-for-byte reproducible.

    python3 -I scripts/twemoji.py                       # downloads both inputs
    python3 -I scripts/twemoji.py --archive twemoji.tar.gz --emoji-data emoji-data.txt
"""

import argparse
import hashlib
import io
import pathlib
import struct
import sys
import tarfile
import urllib.request

COMMIT = "b6b55fef1e8636b540a6d016a4729ca8cdf2e60b"  # jdecked/twemoji v17.0.3
ARCHIVE_URL = f"https://github.com/jdecked/twemoji/archive/{COMMIT}.tar.gz"
ARCHIVE_SHA256 = "705d79de1460e5e775f362f0d0f01fbe3ef8d65bf4648c490e4649704584f747"
EMOJI_DATA_URL = "https://www.unicode.org/Public/17.0.0/ucd/emoji/emoji-data.txt"
EMOJI_DATA_SHA256 = "2cb2bb9455cda83e8481541ecf5b6dfda66a3bb89efa3fa7c5297eccf607b72b"
COUNT = 4009
SIDE = 72
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
VS16 = 0xFE0F

ROOT = pathlib.Path(__file__).resolve().parent.parent


def fetch(url: str, path: str | None, sha256: str) -> bytes:
    if path:
        data = pathlib.Path(path).read_bytes()
    else:
        with urllib.request.urlopen(url, timeout=60) as response:
            data = response.read(64 * 1024 * 1024)
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha256:
        sys.exit(f"{url}: sha256 {digest}, expected {sha256}")
    return data


def emoji_presentation(text: str) -> set[int]:
    """Code points whose default presentation is emoji."""
    found = set()
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        points, prop = (part.strip() for part in line.split(";"))
        if prop != "Emoji_Presentation":
            continue
        first, _, last = points.partition("..")
        found.update(range(int(first, 16), int(last or first, 16) + 1))
    assert 0x1F600 in found and 0x2764 not in found, "unexpected emoji-data.txt"
    return found


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--archive", help="a local copy of the pinned archive")
    parser.add_argument("--emoji-data", help="a local copy of emoji-data.txt")
    args = parser.parse_args()
    archive = fetch(ARCHIVE_URL, args.archive, ARCHIVE_SHA256)
    presentation = emoji_presentation(
        fetch(EMOJI_DATA_URL, args.emoji_data, EMOJI_DATA_SHA256).decode()
    )

    prefix = f"twemoji-{COMMIT}/"
    entries = {}
    license_text = None
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
        for member in tar:
            if member.name == prefix + "LICENSE-GRAPHICS" and member.isfile():
                license_text = tar.extractfile(member).read()
            folder, _, name = member.name.rpartition("/")
            if folder != prefix + "assets/72x72" or not name.endswith(".png"):
                continue
            assert member.isfile(), f"{name} is not a regular file"
            png = tar.extractfile(member).read()
            assert png.startswith(PNG_SIGNATURE) and png[12:16] == b"IHDR", name
            assert struct.unpack(">II", png[16:24]) == (SIDE, SIDE), name
            points = [int(part, 16) for part in name[:-4].split("-")]
            # Lookups drop VS16 from the text too; FE0E is never in the set.
            key = tuple(point for point in points if point != VS16)
            assert key and key not in entries, f"{name} repeats a sequence"
            entries[key] = png
    assert len(entries) == COUNT, f"{len(entries)} images, expected {COUNT}"
    assert license_text and b"Attribution 4.0 International" in license_text

    out = ROOT / "assets" / "twemoji"
    out.mkdir(parents=True, exist_ok=True)
    blob = bytearray()
    lines = []
    for key in sorted(entries):
        png = entries[key]
        # A lone code point that is text by default needs VS16 to be drawn.
        kind = "t" if len(key) == 1 and key[0] not in presentation else "e"
        sequence = "-".join(f"{point:x}" for point in key)
        lines.append(f"{sequence}\t{len(blob)}\t{len(png)}\t{kind}\n")
        blob += png
    (out / "pngs.bin").write_bytes(blob)
    (out / "index.tsv").write_text("".join(lines))
    (ROOT / "assets" / "licenses" / "Twemoji-CC-BY-4.0.txt").write_bytes(license_text)
    texts = sum(line.endswith("\tt\n") for line in lines)
    print(f"{len(entries)} PNGs ({texts} text by default), {len(blob)} bytes")
    for path in (out / "pngs.bin", out / "index.tsv"):
        print(f"{path.name}: sha256 {hashlib.sha256(path.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
