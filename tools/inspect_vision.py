#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Read-only IMG section inventory. Output is private chart-derived metadata.

Offsets follow GPXSee's LBL raster reader and the 2019-11-16 IMG subfile
reference (see VISION.md). Unknown marine payloads are not treated as depths.
No images or geometry are exported. Python standard library only.
"""
import argparse
import collections
import json
import re
import struct
from pathlib import Path


def uint(data, offset, size=4):
    if offset < 0 or offset + size > len(data):
        raise ValueError("Truncated integer")
    return int.from_bytes(data[offset:offset + size], "little")


class Img:
    def __init__(self, path):
        self.file = open(path, "rb")
        try:
            self.size = self.file.seek(0, 2)
            h = self.read(0, 512)
            if h[0] or h[16:23] != b"DSKIMG\0" or h[65:72] != b"GARMIN\0":
                raise ValueError("Unsupported IMG signature or XOR encoding")
            exp = h[0x61] + h[0x62]
            if not 9 <= exp <= 20:
                raise ValueError("Invalid block size")
            self.block_size = 1 << exp
            self.title = (h[0x49:0x5d] + h[0x65:0x83]).decode("cp1252").strip()
            directory = None
            for off in range(0x600, min(self.size, 65536), 512):
                row = self.read(off, 512)
                if row[0] == 1 and row[1:12] == b" " * 11:
                    directory = (off, uint(row, 12))
                    break
            if directory is None:
                raise ValueError("Missing allocation directory")
            start, end = directory
            if not start < end <= min(self.size, 16 * 1024 * 1024):
                raise ValueError("Invalid directory bounds")
            records = collections.defaultdict(dict)
            for off in range(start + 512, end, 512):
                row = self.read(off, 512)
                if row[0] != 1:
                    continue
                key = (row[1:9].decode("ascii").strip(), row[9:12].decode("ascii").strip())
                if not key[0]:
                    continue
                index = row[17]
                if index in records[key]:
                    raise ValueError("Duplicate FAT record")
                blocks = [b for b, in struct.iter_unpack("<H", row[32:]) if b != 65535]
                records[key][index] = (uint(row, 12), blocks)
            self.parts = {}
            for key, chunks in records.items():
                if sorted(chunks) != list(range(len(chunks))):
                    raise ValueError("Missing FAT continuation")
                size = chunks[0][0]
                blocks = [b for i in sorted(chunks) for b in chunks[i][1]]
                if size > len(blocks) * self.block_size or any(b * self.block_size >= self.size for b in blocks):
                    raise ValueError("Invalid block allocation")
                self.parts[key] = (size, blocks)
        except Exception:
            self.file.close()
            raise

    def read(self, offset, size):
        if offset < 0 or size < 0 or offset + size > self.size or size > 16 * 1024 * 1024:
            raise ValueError("Invalid physical read")
        self.file.seek(offset)
        out = self.file.read(size)
        if len(out) != size:
            raise ValueError("Short physical read")
        return out

    def part(self, key, offset, size):
        length, blocks = self.parts[key]
        if offset < 0 or size < 0 or offset + size > length:
            raise ValueError("Invalid subfile read")
        out = bytearray()
        while size:
            block, within = divmod(offset, self.block_size)
            take = min(size, self.block_size - within)
            out.extend(self.read(blocks[block] * self.block_size + within, take))
            offset += take
            size -= take
        return bytes(out)

    def header(self, key, offset, expected):
        base = self.part(key, offset, 21)
        length = uint(base, 0, 2)
        if base[2:12] != b"GARMIN " + expected.encode("ascii") or length < 21:
            raise ValueError(f"Invalid {expected} header")
        return self.part(key, offset, length)


def section(data, at, part_size):
    offset, length = uint(data, at), uint(data, at + 4)
    return {"offset": offset, "bytes": length,
            "range_valid": offset + length <= part_size}


def marine_directory(data, part_size):
    """Observed 92-byte MAR layout; section semantics remain unknown.

    Keep this conditional: other MAR versions must not be forced into the layout.
    Five offset/record-size/length triples, one variable-length pair, one triple.
    Physical adjacency and whole records provide independent structural checks.
    """
    if len(data) != 92:
        return None
    sections = []
    for number, at, variable in [(1, 8, False), (2, 20, False), (3, 32, False),
                                 (4, 44, False), (5, 56, False), (6, 68, True),
                                 (7, 76, False)]:
        offset = uint(data, at)
        record_bytes = None if variable else uint(data, at + 4)
        length = uint(data, at + (4 if variable else 8))
        sections.append({"section": number, "offset": offset, "bytes": length,
                         "record_bytes": record_bytes,
                         "records": length // record_bytes if record_bytes else None,
                         "range_valid": offset + length <= part_size,
                         "whole_records": length % record_bytes == 0 if record_bytes else length == 0 or variable})
    ordered = sorted(sections, key=lambda s: (s["offset"], s["bytes"]))
    contiguous = all(a["offset"] + a["bytes"] == b["offset"] for a, b in zip(ordered, ordered[1:]))
    return {"layout": "observed_92_bytes_unverified_semantics", "sections": sections,
            "contiguous": contiguous,
            "structurally_valid": contiguous and all(s["range_valid"] and s["whole_records"] for s in sections)}


def probe_position_index(img, key, tre_offset, directory):
    """Test a geographic-index hypothesis without exporting coordinates or depths."""
    if not directory["structurally_valid"]:
        return None
    s2, s3 = directory["sections"][1:3]
    if s3["record_bytes"] != 12:
        return None
    tre = img.header(key, tre_offset, "TRE")
    west, south, east, north = [int.from_bytes(tre[o:o + 3], "little", signed=True) * 360 / 2**24
                                 for o in (0x1e, 0x1b, 0x18, 0x15)]
    count = inside = offsets = 0
    for at in range(0, s3["bytes"], 12 * 4096):
        data = img.part(key, s3["offset"] + at, min(12 * 4096, s3["bytes"] - at))
        for offset, x, y in struct.iter_unpack("<Iii", data):
            lon, lat = x * 360 / 2**28, y * 360 / 2**28
            count += 1
            inside += west - .0001 <= lon <= east + .0001 and south - .0001 <= lat <= north + .0001
            offsets += offset <= s2["bytes"] and (not s2["record_bytes"] or offset % s2["record_bytes"] == 0)
    return {"hypothesis": "section_3_offset_and_two_signed_28_bit_map_coordinates",
            "records": count, "coordinates_within_TRE_bounds": inside,
            "aligned_offsets_within_section_2": offsets,
            "depth_semantics_verified": False}


def inspect(img):
    tiles = []
    for key in sorted(img.parts):
        if key[1] != "GMP":
            continue
        size = img.parts[key][0]
        gmp = img.header(key, 0, "GMP")
        row = {"tile": key[0], "bytes": size, "headers": {}}
        for kind, at in [("TRE", 0x19), ("RGN", 0x1d), ("LBL", 0x21),
                         ("NET", 0x25), ("NOD", 0x29), ("DEM", 0x2d),
                         ("MAR", 0x31), ("MET", 0x39)]:
            if len(gmp) < at + 4 or not (offset := uint(gmp, at)):
                continue
            h = img.header(key, offset, kind)
            info = {"offset": offset, "header_bytes": len(h)}
            if kind == "LBL" and len(h) >= 0x19a:
                info["image_index"] = section(h, 0x184, size)
                info["image_record_bytes"] = uint(h, 0x18c, 2)
                info["image_storage"] = section(h, 0x192, size)
            if kind == "MAR" and len(h) >= 0x1d:
                # Candidate offset/length only; payload meaning is unverified.
                info["candidate_payload"] = section(h, 0x15, size)
                payload = info["candidate_payload"]
                if payload["range_valid"] and payload["bytes"] == 92:
                    info["directory"] = marine_directory(img.part(key, payload["offset"], 92), size)
            if kind == "MET":
                info["sections"] = {str(n): section(h, at, size)
                                    for n, at in [(1, 0x15), (2, 0x29), (3, 0x31),
                                                  (4, 0x98), (6, 0x120), (7, 0x128)]
                                    if at + 8 <= len(h)}
                if len(h) > 0xb6:
                    info["product"] = h[0xb6:0x11b].split(b"\0")[0].decode("cp1252", errors="replace")
            row["headers"][kind] = info
        mar = row["headers"].get("MAR", {})
        if mar.get("directory") and "TRE" in row["headers"]:
            mar["position_index_probe"] = probe_position_index(img, key, row["headers"]["TRE"]["offset"], mar["directory"])
        tiles.append(row)
    counts = collections.Counter(k for t in tiles for k in t["headers"])
    return {"title": img.title, "bytes": img.size, "gmp_tiles": len(tiles),
            "header_counts": dict(counts), "tiles": tiles}


def jpeg_dimensions(data):
    """Reject signature coincidences using a bounded JPEG marker walk to SOF."""
    if data[:2] != b"\xff\xd8":
        return None
    pos = 2
    while pos + 4 <= len(data):
        if data[pos] != 255:
            return None
        while pos < len(data) and data[pos] == 255:
            pos += 1
        if pos + 3 > len(data):
            return None
        marker = data[pos]
        length = int.from_bytes(data[pos + 1:pos + 3], "big")
        if length < 2 or pos + 1 + length > len(data):
            return None
        if marker in (0xc0, 0xc1, 0xc2) and length >= 8:
            h, w = struct.unpack(">HH", data[pos + 4:pos + 8])
            components = data[pos + 8]
            if w and h and components in (1, 3, 4) and length == 8 + 3 * components:
                return [w, h]
            return None
        if marker in (0x00, 0xd9, 0xda):
            return None
        pos += 1 + length
    return None


def scan_jpeg(img):
    candidates = 0
    dimensions = []
    offset = 0
    overlap = b""
    while offset < img.size:
        data = img.read(offset, min(4 * 1024 * 1024, img.size - offset))
        combined = overlap + data
        for match in re.finditer(rb"\xff\xd8\xff[\xdb\xe0\xe1\xc0\xc2]", combined):
            at = offset - len(overlap) + match.start()
            candidates += 1
            dim = jpeg_dimensions(img.read(at, min(65536, img.size - at)))
            if dim:
                dimensions.append(dim)
        overlap = combined[-3:]
        offset += len(data)
    return {"candidates": candidates, "validated_headers": len(dimensions), "dimensions": dimensions,
            "limitation": "Physical scan, at most 64 KiB per header; does not exclude fragmented, compressed or differently encoded images."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("chart", type=Path)
    parser.add_argument("--scan-jpeg", action="store_true", help="Also scan physical bytes for plausible JPEG headers")
    args = parser.parse_args()
    img = Img(args.chart)
    try:
        report = inspect(img)
        if args.scan_jpeg:
            report["jpeg_scan"] = scan_jpeg(img)
        print(json.dumps(report, ensure_ascii=True, indent=2))
    finally:
        img.file.close()


if __name__ == "__main__":
    main()
