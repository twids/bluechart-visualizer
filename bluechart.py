"""Read-only Garmin IMG explorer. SPDX-License-Identifier: GPL-3.0-only.

Geometry decoding adapted from QMapShack, Copyright (C) 2006-2008
Oliver Eichler. See THIRD_PARTY.md. No chart data is bundled with this code.
"""
from __future__ import annotations

import argparse
import collections
import json
import math
import struct
import sqlite3
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parent
DEFAULT_CHART = ROOT / 'charts' / 'chart.img'
UNIT = 360 / (1 << 24)


class FormatError(ValueError):
    pass


def uint(data, offset, size=4):
    if offset < 0 or offset + size > len(data):
        raise FormatError(f'Truncated field at {offset}, need {size} bytes')
    if size == 1:
        return data[offset]
    if size == 2:
        return struct.unpack_from('<H',data,offset)[0]
    if size == 3:
        return data[offset] | data[offset+1] << 8 | data[offset+2] << 16
    if size == 4:
        return struct.unpack_from('<I',data,offset)[0]
    return int.from_bytes(data[offset:offset + size], 'little')


def sint(data, offset, size=3):
    value = uint(data, offset, size)
    return value - (1 << (size * 8)) if value & (1 << (size * 8 - 1)) else value


def intersects(a, b):
    return a[0] <= b[2] and a[2] >= b[0] and a[1] <= b[3] and a[3] >= b[1]


@dataclass
class Part:
    name: str
    kind: str
    size: int
    blocks: list[int]


class Img:
    """Bounded random access, including fragmented and multipart FAT entries."""
    def __init__(self, path):
        self.path = Path(path)
        self.size = self.path.stat().st_size
        self.stream = self.path.open('rb')
        self.mutex = threading.RLock()
        self.bytes_read = 0
        self.read_calls = 0
        self.max_read = 0
        try:
            h = self.read(0, 512)
            if h[0] or h[16:23] != b'DSKIMG\0' or h[65:72] != b'GARMIN\0':
                raise FormatError('Unsupported IMG signature or XOR encoding')
            exponent = h[0x61] + h[0x62]
            if not 9 <= exponent <= 20:
                raise FormatError('Invalid IMG block size')
            self.block_size = 1 << exponent
            self.title = (h[0x49:0x5d] + h[0x65:0x83]).decode('cp1252').strip()
            # Locate the blank-name directory allocation entry, not a fixed FAT offset.
            directory = None
            for offset in range(0x600, min(self.size, 65536), 512):
                entry = self.read(offset, 512)
                if entry[0] == 1 and entry[1:12] == b' ' * 11:
                    directory = offset
                    directory_end = uint(entry, 12)
                    break
            if directory is None or not directory < directory_end <= min(self.size, 16*1024*1024):
                raise FormatError('Cannot locate a bounded IMG allocation directory')
            records = collections.defaultdict(dict)
            for offset in range(directory + 512, directory_end, 512):
                entry = self.read(offset, 512)
                if entry[0] != 1:
                    continue
                name = entry[1:9].decode('ascii').strip()
                kind = entry[9:12].decode('ascii').strip()
                if not name:
                    continue
                # Byte 0x10 is a flag; the continuation index is at 0x11.
                part = uint(entry, 17, 1)
                key = name, kind
                if part in records[key]:
                    raise FormatError(f'Duplicate FAT record {key}/{part}')
                blocks = [b for b in struct.unpack_from('<240H', entry, 32) if b != 65535]
                records[key][part] = (uint(entry, 12), blocks)
            self.parts = {}
            for (name, kind), chunks in records.items():
                if sorted(chunks) != list(range(len(chunks))):
                    raise FormatError(f'Missing FAT continuation: {name}.{kind}')
                size = chunks[0][0]
                blocks = [block for i in range(len(chunks)) for block in chunks[i][1]]
                if size > len(blocks) * self.block_size:
                    raise FormatError(f'Too few blocks: {name}.{kind}')
                for block in blocks:
                    if block*self.block_size >= self.size:
                        raise FormatError(f'Block outside IMG: {name}.{kind}')
                self.parts[name, kind] = Part(name, kind, size, blocks)
        except Exception:
            self.close()
            raise

    def close(self):
        self.stream.close()

    def read(self, offset, length):
        if offset < 0 or length < 0 or offset + length > self.size or length > 16*1024*1024:
            raise FormatError(f'Invalid physical read {offset}+{length}')
        with self.mutex:
            self.stream.seek(offset)
            data = self.stream.read(length)
            self.bytes_read += len(data)
            self.read_calls += 1
            self.max_read = max(self.max_read, length)
        if len(data) != length:
            raise FormatError('Short physical read')
        return data

    def read_part(self, part, offset=0, length=None):
        length = part.size - offset if length is None else length
        if offset < 0 or length < 0 or offset + length > part.size or length > 16*1024*1024:
            raise FormatError(f'Invalid read of {part.name}.{part.kind}: {offset}+{length}')
        result = bytearray()
        while length:
            index, within = divmod(offset, self.block_size)
            count = min(length, self.block_size - within)
            result.extend(self.read(part.blocks[index]*self.block_size + within, count))
            offset += count
            length -= count
        return bytes(result)


class Bits:
    def __init__(self, data):
        self.data = data
        self.pos = 0
        self.total = len(data)*8
        self.register = 0
        self.available = 0
        self.byte_pos = 0

    @property
    def remaining(self):
        return self.total - self.pos

    def get(self, count):
        if count < 0 or count > self.remaining:
            raise FormatError('Truncated coordinate bitstream')
        while self.available < count:
            if self.byte_pos+4 <= len(self.data):
                self.register |= struct.unpack_from('<I',self.data,self.byte_pos)[0] << self.available
                self.byte_pos += 4
                self.available += 32
            else:
                self.register |= self.data[self.byte_pos] << self.available
                self.byte_pos += 1
                self.available += 8
        value = self.register & ((1 << count)-1)
        self.register >>= count
        self.available -= count
        self.pos += count
        return value


def deltas(data, base, extended=False, extra=False):
    bits = Bits(data)
    axes = []
    for nibble in (base & 15, base >> 4):
        fixed = bits.get(1)
        negative = bits.get(1) if fixed else False
        width = 2 + (nibble if nibble <= 9 else 2*nibble-9) + (not fixed)
        axes.append([width, fixed, negative])
    if extended:
        bits.get(1)  # Reserved bit, not part of either coordinate width.
    pair_width = axes[0][0]+axes[1][0]+extra
    while bits.remaining >= pair_width:
        if extra:
            bits.get(1)
        pair = []
        for width, fixed, negative in axes:
            value = bits.get(width)
            if fixed:
                value = -value if negative else value
            else:
                sign = 1 << (width-1)
                extension = 0
                while value == sign:
                    extension += sign-1
                    value = bits.get(width)
                value = extension+value if value < sign else value-2*sign-extension
            pair.append(value)
        yield pair


def variable_uint(data, pos, bitfield=False):
    first = uint(data,pos,1)
    if first & 1:
        size,shift = 1,1
    elif first & 2:
        size,shift = 2,2
    elif bitfield:
        size,shift = (3 if first & 4 else 4),3
    else:
        size,shift = (3 if first & 4 else 4),3
    return uint(data,pos,size) >> shift, pos+size


def attributes(data,pos,subtype,config,kind):
    details = {}
    if subtype & 128:
        flags = uint(data,pos,1)
        pos += 1
        marker = flags >> 5
        size = marker-3 if 4 <= marker <= 6 else 0
        if marker == 7:
            size,pos = variable_uint(data,pos)
        if pos+size > len(data):
            raise FormatError('Truncated class attributes')
        value = uint(data,pos,size) if size else 0
        if kind in (0x10301,0x10302) and size <= 3:
            units = (flags >> 3) & 3
            if not size:
                depth = flags & 63
                units = (flags >> 5) & 2
            else:
                depth = value | ((flags & (7 if size == 1 else 3)) << (8*size))
            details['depth_m'] = depth / 10 if units & 1 else depth
        if 0x10200 <= kind < 0x10300:
            details['symbol_detail'] = 'buoy'
        if 0x10100 <= kind < 0x10200:
            details['symbol_detail'] = 'light'
        pos += size
    global_flags, local_flags = config
    if subtype & 64:
        bitfield = 0xffffffff
        if local_flags[0] & 0x20000000:
            bitfield,pos = variable_uint(data,pos,True)
        j = 0
        for i in range(29):
            if local_flags[0] & (1 << i):
                if bitfield & 1:
                    mode = (local_flags[(j >> 4)+1] >> ((j*2) & 30)) & 3
                    size = mode+1
                    if mode == 3:
                        size,pos = variable_uint(data,pos)
                    pos += size
                bitfield >>= 1
                j += 1
    while global_flags:
        pos += global_flags & 3
        global_flags >>= 2
    if pos > len(data):
        raise FormatError('Truncated object attributes')
    return pos,details


def decode_poly(data, pos, sub, line=False, extended=False, config=(0,(0,0,0))):
    start = pos
    raw_type = uint(data, pos, 1)
    if extended:
        subtype = uint(data, pos+1, 1)
        kind = 0x10000 + (raw_type << 8) + (subtype & 31)
        dx, dy = sint(data, pos+2, 2), sint(data, pos+4, 2)
        pos += 6
        length,pos = variable_uint(data,pos)
        length -= 1  # Includes the coordinate-width byte.
        label = 0
        extra = False
    else:
        kind = raw_type & (63 if line else 127)
        label = uint(data, pos+1, 3)
        extra = bool(label & 0x400000)
        dx, dy = sint(data, pos+4, 2), sint(data, pos+6, 2)
        pos += 8
        width = 2 if raw_type & 128 else 1
        length = uint(data, pos, width)
        pos += width
    base = uint(data, pos, 1)
    pos += 1
    if length <= 0 or pos + length > len(data):
        raise FormatError(f'Invalid polygon bitstream at {start}')
    scale = 1 << sub['shift']
    x, y = sub['cx'] + dx*scale, sub['cy'] + dy*scale
    coords = [[x*UNIT, y*UNIT]]
    for dx, dy in deltas(data[pos:pos+length], base, extended, extra):
        x += dx*scale
        y += dy*scale
        coords.append([x*UNIT, y*UNIT])
    pos += length
    if extended and subtype & 32:
        label = uint(data, pos, 3)
        pos += 3
    if extended:
        pos,_ = attributes(data,pos,subtype,config,kind)
    return {'kind': 'line' if line else 'polygon', 'type': kind,
            'label_ref': label & 0x3fffff, 'label_mode': 'net' if label & 0x800000 else 'normal',
            'coords': coords}, pos


def decode_point(data, pos, sub, extended=False, config=(0,(0,0,0))):
    details = {}
    kind = uint(data, pos, 1) << 8
    if extended:
        subtype = uint(data, pos+1, 1)
        kind += 0x10000 + (subtype & 31)
        dx, dy = sint(data, pos+2, 2), sint(data, pos+4, 2)
        pos += 6
        label = 0
        if subtype & 32:
            label = uint(data, pos, 3)
            pos += 3
        pos,details = attributes(data,pos,subtype,config,kind)
    else:
        label = uint(data, pos+1, 3)
        dx, dy = sint(data, pos+4, 2), sint(data, pos+6, 2)
        pos += 8
        if label & 0x800000:
            kind += uint(data, pos, 1)
            pos += 1
    scale = 1 << sub['shift']
    return {'kind': 'point', 'type': kind, 'label_ref': label & 0x3fffff,
            'label_mode': 'poi' if label & 0x400000 else 'normal',
            'coords': [[(sub['cx']+dx*scale)*UNIT, (sub['cy']+dy*scale)*UNIT]], **details}, pos


class Chart:
    def __init__(self, path=DEFAULT_CHART, overview_path=None):
        self.img = Img(path)
        self.tiles = []
        self.warnings = []
        self.cache = collections.OrderedDict()
        self.cache_bytes = 0
        self.cache_limit = 64*1024*1024
        self.cache_hits = 0
        self.cache_misses = 0
        self.labels = collections.OrderedDict()
        self.loaded_tiles = collections.OrderedDict()
        self.mutex = threading.RLock()
        for (name, kind), part in self.img.parts.items():
            if kind != 'TRE':
                continue
            try:
                tile = self.tile(part)
                self.tiles.append(tile)
            except (FormatError, KeyError) as exc:
                self.warnings.append(f'{name}: {exc}')
        if not self.tiles:
            raise FormatError('No readable TRE/RGN map tiles: ' + '; '.join(self.warnings[:4]))
        self.overview = None
        overview_path=Path(overview_path) if overview_path else ROOT/'.cache'/'overview.sqlite3'
        if overview_path.is_file():
            try:
                absolute=overview_path.absolute()
                uri=absolute.as_uri()
                if absolute.as_posix().startswith('//'):
                    uri='file://'+quote(absolute.as_posix(),safe='/:')
                overview=sqlite3.connect(uri+'?mode=ro',uri=True,check_same_thread=False)
                overview.execute('PRAGMA cache_size=-8192')
                overview.execute('PRAGMA query_only=ON')
                expected={'format':1,'size':self.img.size,'mtime':int(self.img.path.stat().st_mtime),'title':self.img.title}
                if json.loads(overview.execute('SELECT payload FROM metadata').fetchone()[0])!=expected:
                    overview.close()
                    raise FormatError('Overview cache belongs to another chart revision')
                self.overview=overview
            except (OSError,ValueError,KeyError,sqlite3.Error) as exc:
                self.warnings.append(f'Overview: {exc}')

    def close(self):
        if self.overview:
            self.overview.close()
        self.img.close()

    def tile(self, part, load=False):
        h = self.img.read_part(part, 0, min(part.size, 0x121))
        if h[2:12] != b'GARMIN TRE' or h[13] & 128:
            raise FormatError('Unsupported or locked TRE')
        rgn = self.img.parts[part.name, 'RGN']
        rh = self.img.read_part(rgn, 0, min(rgn.size, 0x7d))
        if rh[2:12] != b'GARMIN RGN':
            raise FormatError('Invalid RGN header')
        if len(rh) >= 125 and uint(rh,0x75) and uint(rh,0x79) & 30:
            raise FormatError('Huffman coordinate encoding is not supported')
        configs = {}
        for kind,at in [('polygon',0x29),('line',0x45),('point',0x61)]:
            configs[kind] = (uint(rh,at),tuple(uint(rh,at+4+i*4) for i in range(3))) if len(rh)>=at+16 else (0,(0,0,0))
        rgn_offset, rgn_length = uint(rh, 0x15), uint(rh, 0x19)
        levels_raw = self.img.read_part(part, uint(h, 0x21), uint(h, 0x25))
        if len(levels_raw) % 4:
            raise FormatError('Misaligned map levels')
        levels = [{'level': z & 15, 'bits': bits, 'count': count, 'inherited': bool(z & 128)}
                  for z, bits, count in struct.iter_unpack('<BBH', levels_raw)]
        if not levels or any(not 1 <= l['bits'] <= 24 for l in levels):
            raise FormatError('Invalid coordinate resolution')
        metadata = {'id': part.name, 'bbox': [sint(h,0x1e)*UNIT,sint(h,0x1b)*UNIT,
                                             sint(h,0x18)*UNIT,sint(h,0x15)*UNIT],
                    'levels':levels, 'subs':None, 'rgn':rgn, 'configs':configs}
        if not load:
            return metadata
        raw = self.img.read_part(part, uint(h, 0x29), uint(h, 0x2d))
        subs = []
        pos = 0
        for li, level in enumerate(levels):
            for _ in range(level['count']):
                shift = 24-level['bits']
                cx, cy = sint(raw, pos+4), sint(raw, pos+7)
                width = (uint(raw, pos+10, 2) & 32767) << shift
                height = (uint(raw, pos+12, 2) & 32767) << shift
                subs.append({'index': len(subs), 'bits': level['bits'], 'shift': shift,
                             'cx': cx, 'cy': cy, 'flags': uint(raw, pos+3, 1),
                             'start': rgn_offset+(uint(raw, pos, 4) & 0x0fffffff),
                             'bbox': [(cx-width)*UNIT, (cy-height)*UNIT,
                                      (cx+width+1)*UNIT, (cy+height+1)*UNIT]})
                pos += 14 if li == len(levels)-1 else 16
        for i, sub in enumerate(subs):
            sub['end'] = subs[i+1]['start'] if i+1 < len(subs) else rgn_offset+rgn_length
            if not rgn_offset <= sub['start'] <= sub['end'] <= rgn_offset+rgn_length <= rgn.size:
                raise FormatError('Invalid subdivision RGN range')
        if len(h) >= 0x9a and uint(h, 0x80):
            size = uint(h, 0x84, 2)
            if not 4 <= size <= 64:
                raise FormatError(f'Unsupported TRE7 record size {size}')
            ext = self.img.read_part(part, uint(h, 0x7c), uint(h, 0x80))
            if len(ext) % size:
                raise FormatError('Misaligned TRE7 table')
            first = len(subs) - len(ext)//size + 1
            if first < 0:
                raise FormatError('Too many TRE7 records')
            flags = uint(h,0x86)
            at = 0
            for bit, field, hoff in [(1,'polygon',0x1d), (2,'line',0x39), (4,'point',0x55)]:
                if not flags & bit:
                    continue
                base, length = uint(rh, hoff), uint(rh, hoff+4)
                if base+length > rgn.size:
                    raise FormatError('Extended RGN outside subfile')
                if at+4 > size:
                    raise FormatError('TRE7 flags exceed record size')
                for i in range(first,len(subs)):
                    sub = subs[i]
                    start = uint(ext, (i-first)*size+at)
                    end = uint(ext, (i-first+1)*size+at)
                    if not 0 <= start <= end <= length:
                        raise FormatError('Invalid TRE7 range')
                    sub[field+'2'] = (base+start, end-start)
                at += 4
        metadata['subs'] = subs
        return metadata

    def load_tile(self,tile):
        key = tile['id']
        if tile['subs'] is None:
            tile.update(self.tile(self.img.parts[key,'TRE'],True))
            self.loaded_tiles[key] = tile
            while len(self.loaded_tiles) > 4:
                _,old = self.loaded_tiles.popitem(last=False)
                old['subs'] = None
        else:
            self.loaded_tiles.move_to_end(key)

    def label(self, tile, ref, mode):
        if not ref or mode == 'net':
            return ''
        key = tile['id'], ref, mode
        if key in self.labels:
            self.labels.move_to_end(key)
            return self.labels[key]
        part = self.img.parts.get((tile['id'], 'LBL'))
        if not part:
            return ''
        h = self.img.read_part(part, 0, min(0xac, part.size))
        if h[2:12] != b'GARMIN LBL':
            raise FormatError('Invalid LBL header')
        if mode == 'poi':
            offset = uint(h,0x57) + ref
            if offset+3 > uint(h,0x57)+uint(h,0x5b):
                return ''
            ref = uint(self.img.read_part(part,offset,3),0,3) & 0x3fffff
        offset = ref << h[0x1d]
        length = uint(h,0x19)
        if offset >= length:
            return ''
        data = self.img.read_part(part, uint(h,0x15)+offset, min(256,length-offset))
        coding = h[0x1e]
        if coding in (9,10):
            cp = uint(h,0xaa,2) if len(h) >= 0xac else 1252
            encoding = 'utf-8' if coding == 10 or cp == 65001 else f'cp{cp or 1252}'
            try:
                text = data.split(b'\0')[0].decode(encoding, errors='replace')
            except LookupError:
                text = data.split(b'\0')[0].decode('cp1252', errors='replace')
        elif coding == 6:
            bit_string = ''.join(f'{byte:08b}' for byte in data)
            chars = [int(bit_string[i:i+6],2) for i in range(0,len(bit_string)-5,6)]
            table = ' ABCDEFGHIJKLMNOPQRSTUVWXYZ' + '\0'*5 + '0123456789' + '\0'*6
            text = ''
            escaped = None
            for c in chars:
                if c > 47:
                    break
                if escaped is not None:
                    if escaped == 27 and c <= 26:
                        text += chr(96+c)
                    elif escaped == 28:
                        text += chr(64+c) if 42 <= c <= 46 else chr(32+c) if c <= 15 else chr(32+c) if 26 <= c <= 31 else ''
                    escaped = None
                elif c in (27,28):
                    escaped = c
                elif 29 <= c <= 31:
                    text += ' '
                else:
                    text += table[c]
        else:
            return ''
        text = ''.join(c if c.isprintable() else ' ' for c in text).strip()
        self.labels[key] = text
        if len(self.labels) > 8192:
            self.labels.popitem(last=False)
        return text

    def decode_sub(self, tile, sub):
        cache_key = tile['id'], sub['index']
        if cache_key in self.cache:
            self.cache_hits += 1
            self.cache.move_to_end(cache_key)
            return self.cache[cache_key][0]
        self.cache_misses += 1
        result, errors = [], []
        raw = self.img.read_part(tile['rgn'],sub['start'],sub['end']-sub['start'])
        kinds = [k for flag,k in [(16,'point'),(32,'indexed'),(64,'line'),(128,'polygon')] if sub['flags'] & flag]
        sections = []
        if raw and kinds:
            offsets = [2*(len(kinds)-1)] + [uint(raw,i*2,2) for i in range(len(kinds)-1)] + [len(raw)]
            if offsets != sorted(offsets) or offsets[-1] > len(raw):
                raise FormatError('Invalid subdivision section offsets')
            sections.extend((kind,raw[a:b],False) for kind,a,b in zip(kinds,offsets,offsets[1:]))
        for kind in ('polygon','line','point'):
            offset,length = sub.get(kind+'2',(0,0))
            if length:
                sections.append((kind,self.img.read_part(tile['rgn'],offset,length),True))
        for kind, data, extended in sections:
            pos = 0
            while pos < len(data):
                try:
                    if kind in ('point','indexed'):
                        feature, pos = decode_point(data,pos,sub,extended,tile['configs']['point'])
                    else:
                        feature, pos = decode_poly(data,pos,sub,kind == 'line',extended,tile['configs'][kind])
                    feature['label'] = self.label(tile,feature['label_ref'],feature['label_mode'])
                    xs,ys = zip(*feature['coords'])
                    feature['bbox'] = [min(xs),min(ys),max(xs),max(ys)]
                    if not intersects(feature['bbox'],sub['bbox']):
                        raise FormatError('Object outside subdivision bounds')
                    feature['tile'] = tile['id']
                    result.append(feature)
                except FormatError as exc:
                    errors.append(f"{tile['id']}/{sub['index']} {kind}: {exc}")
                    break
        payload = {'features': result, 'warnings': errors}
        # Estimate retained Python objects without serializing every coordinate.
        cost = 256 + sum(1024+160*len(f['coords'])+4*len(f['label']) for f in result)
        if cost <= self.cache_limit:
            while self.cache and self.cache_bytes+cost > self.cache_limit:
                _, (_, old_cost) = self.cache.popitem(last=False)
                self.cache_bytes -= old_cost
            self.cache[cache_key] = payload, cost
            self.cache_bytes += cost
        return payload

    def select_subdivisions(self,bbox,bits,include_fishing,cancelled):
        selected,errors = [],[]
        base_available = any(t['id'].startswith('B') for t in self.tiles)
        for tile in self.tiles:
            if cancelled and cancelled():
                raise InterruptedError('Superseded map request')
            if not intersects(tile['bbox'],bbox):
                continue
            is_base = tile['id'].startswith('B')
            if base_available and is_base != (bits<=17):
                continue
            if not include_fishing and tile['id'].startswith('F'):
                continue
            try:
                self.load_tile(tile)
            except FormatError as exc:
                errors.append(f"{tile['id']}: {exc}")
                continue
            available=sorted({l['bits'] for l in tile['levels'] if not l['inherited'] and l['count']})
            if not available:
                continue
            chosen=min(available,key=lambda b:abs(b-bits))
            for sub in tile['subs']:
                if sub['bits']==chosen and intersects(sub['bbox'],bbox):
                    selected.append((tile,sub))
                    if len(selected)>180:
                        return selected,errors
        return selected,errors

    def features(self, bbox, bits=24, include_fishing=True, cancelled=None):
        started = time.perf_counter()
        with self.mutex:
            before = self.img.bytes_read
            hits_before,misses_before = self.cache_hits,self.cache_misses
            result, errors = [], []
            span=max(bbox[2]-bbox[0],bbox[3]-bbox[1])
            overview_bits=13 if span>20 else 15 if span>5 else 17 if span>2 else bits
            effective=min(bits,overview_bits)
            while True:
                selected,errors = self.select_subdivisions(bbox,effective,include_fishing,cancelled)
                if len(selected)<=180 or effective<=17:
                    break
                effective-=1
            if effective<=17 and self.overview:
                level=min((13,15,17),key=lambda value:abs(value-effective))
                result=[json.loads(row[0]) for row in self.overview.execute(
                    'SELECT payload FROM overview WHERE level=? AND id IN (SELECT id FROM bounds WHERE west<=? AND east>=? AND south<=? AND north>=?)',
                    (level,bbox[2],bbox[0],bbox[3],bbox[1]))]
                selected=[]
            elif effective<=17:
                errors.append('Overview cache missing: run build_overview.py to display coastlines in broad views.')
            if len(selected)>180:
                raise FormatError('No bounded overview level is available for this map')
            for tile,sub in selected:
                if cancelled and cancelled():
                    raise InterruptedError('Superseded map request')
                try:
                    payload = self.decode_sub(tile,sub)
                    errors.extend(payload['warnings'])
                    result.extend(f for f in payload['features'] if intersects(f['bbox'],bbox))
                except FormatError as exc:
                    errors.append(f"{tile['id']}/{sub['index']}: {exc}")
            return {'features':result,'warnings':errors,'stats':{
                'features':len(result),'subdivisions':len(selected),'effective_bits':effective,
                'overview_cache':bool(effective<=17 and self.overview),
                'bytes_read':self.img.bytes_read-before,'total_bytes_read':self.img.bytes_read,
                'max_read_bytes':self.img.max_read,'cache_estimate_bytes':self.cache_bytes,
                'cache_hits':self.cache_hits-hits_before,'cache_misses':self.cache_misses-misses_before,
                'seconds':round(time.perf_counter()-started,3)}}

    def metadata(self):
        return {'title':self.img.title,'file_size':self.img.size,'block_size':self.img.block_size,
                'parts':len(self.img.parts),'tiles':[{'id':t['id'],'bbox':t['bbox'],'levels':t['levels'],
                'subdivisions':sum(l['count'] for l in t['levels'])} for t in self.tiles], 'warnings':self.warnings,
                'initial_bbox':list(self.tiles[0]['bbox']),
                'coverage_bbox':[min(t['bbox'][0] for t in self.tiles),min(t['bbox'][1] for t in self.tiles),
                                 max(t['bbox'][2] for t in self.tiles),max(t['bbox'][3] for t in self.tiles)],
                'overview_available':self.overview is not None,
                'limitations':['Experimental type and symbol decoding; not for navigation.',
                               'Vision, routes, and general g3 compatibility are unverified.',
                               'NET labels and some extra attributes are not yet supported.']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--chart',type=Path,default=DEFAULT_CHART)
    parser.add_argument('--bbox',type=float,nargs=4,required=True)
    parser.add_argument('--bits',type=int,default=24)
    args = parser.parse_args()
    started = time.perf_counter()
    chart = Chart(args.chart)
    try:
        payload = chart.features(args.bbox,args.bits)
        print(json.dumps({'metadata':chart.metadata(),'summary':payload['stats'],
                          'types':dict(collections.Counter(f"{f['kind']}:0x{f['type']:x}" for f in payload['features'])),
                          'labels':[f['label'] for f in payload['features'] if f['label']][:60],
                          'warnings':payload['warnings'], 'total_seconds':time.perf_counter()-started},ensure_ascii=False,indent=2))
    finally:
        chart.img.close()


if __name__ == '__main__':
    main()
