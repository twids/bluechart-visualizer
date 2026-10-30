# SPDX-License-Identifier: GPL-3.0-only
"""Synthetic format checks; no chart payloads or fixtures."""
import importlib.util
import struct
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("inspect_vision", Path(__file__).resolve().parents[1] / "tools/inspect_vision.py")
vision = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vision)


class VisionTests(unittest.TestCase):
    def test_marine_directory_structure_and_invalid_range(self):
        data = bytearray(92)
        offset = 1000
        for at in (8, 20, 32, 44, 56):
            struct.pack_into("<III", data, at, offset, 3, 6)
            offset += 6
        struct.pack_into("<II", data, 68, offset, 7)
        offset += 7
        struct.pack_into("<III", data, 76, offset, 4, 8)
        report = vision.marine_directory(data, offset + 8)
        self.assertTrue(report["structurally_valid"])
        self.assertEqual(report["sections"][0]["records"], 2)
        self.assertFalse(vision.marine_directory(data, offset)["structurally_valid"])
        struct.pack_into("<I", data, 16, 5)
        self.assertFalse(vision.marine_directory(data, offset + 8)["structurally_valid"])
        self.assertIsNone(vision.marine_directory(data[:91], 10000))

    def test_jpeg_false_positives_and_frame_dimensions(self):
        self.assertIsNone(vision.jpeg_dimensions(bytes.fromhex("ff d8 ff db 12 a0 01 71")))
        # Synthetic grayscale SOF marker: precision, height, width, 1 component.
        jpeg = bytes.fromhex("ff d8 ff c0 00 0b 08 00 20 00 40 01 01 11 00")
        self.assertEqual(vision.jpeg_dimensions(jpeg), [64, 32])
        self.assertIsNone(vision.jpeg_dimensions(jpeg[:-1]))

    def test_position_hypothesis_rejects_outside_and_unaligned_records(self):
        tre = bytearray(60)
        for at in (0x18, 0x15):
            tre[at:at + 3] = (2**20).to_bytes(3, "little")
        positions = struct.pack("<IiiIii", 3, 2**23, 2**23, 1, -2**23, -2**23)
        class Synthetic:
            def header(self, *args):
                return tre
            def part(self, key, offset, size):
                return positions[offset:offset + size]
        directory = {"structurally_valid": True, "sections": [{}, {"bytes": 12, "record_bytes": 3}, {"bytes": 24, "record_bytes": 12, "offset": 0}]}
        result = vision.probe_position_index(Synthetic(), None, 0, directory)
        self.assertEqual(result["records"], 2)
        self.assertEqual(result["coordinates_within_TRE_bounds"], 1)
        self.assertEqual(result["aligned_offsets_within_section_2"], 1)
        self.assertFalse(result["depth_semantics_verified"])

    def test_fragmented_reads_and_continuation_validation(self):
        data = bytearray(14 * 512)
        data[16:23] = b"DSKIMG\0"
        data[65:72] = b"GARMIN\0"
        data[0x61] = 9
        data[0x600] = 1
        data[0x601:0x60c] = b" " * 11
        struct.pack_into("<I", data, 0x60c, 0xc00)
        for off, index, block in [(0x800, 0, 10), (0xa00, 1, 12)]:
            data[off] = 1
            data[off + 1:off + 12] = b"TEST0001MAR"
            struct.pack_into("<I", data, off + 12, 800 if index == 0 else 0)
            data[off + 17] = index
            data[off + 32:off + 512] = b"\xff" * 480
            struct.pack_into("<H", data, off + 32, block)
        data[10 * 512:11 * 512] = b"A" * 512
        data[12 * 512:13 * 512] = b"B" * 512
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "synthetic.img"
            path.write_bytes(data)
            img = vision.Img(path)
            try:
                self.assertEqual(img.part(("TEST0001", "MAR"), 500, 25), b"A" * 12 + b"B" * 13)
                with self.assertRaises(ValueError):
                    img.part(("TEST0001", "MAR"), 799, 2)
            finally:
                img.file.close()
            data[0xa11] = 2
            path.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "continuation"):
                vision.Img(path)


if __name__ == "__main__":
    unittest.main()
