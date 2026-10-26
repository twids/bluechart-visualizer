// SPDX-License-Identifier: GPL-3.0-only
// Coordinate decoding adapted from QMapShack and GPXSee; see THIRD_PARTY.md.
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
pub const UNIT: f64 = 360.0 / (1u64 << 24) as f64;
pub type Bounds = [f64; 4];
pub fn intersects(a: Bounds, b: Bounds) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}
pub fn contains(a: Bounds, b: Bounds) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && a[2] >= b[2] && a[3] >= b[3]
}
pub fn uint(d: &[u8], p: usize, n: usize) -> Result<u32> {
    ensure!(
        n <= 4 && p.checked_add(n).is_some_and(|e| e <= d.len()),
        "Truncated field at {p}, need {n} bytes"
    );
    Ok(d[p..p + n]
        .iter()
        .enumerate()
        .fold(0, |a, (i, b)| a | (*b as u32) << (i * 8)))
}
pub fn sint(d: &[u8], p: usize, n: usize) -> Result<i64> {
    let v = uint(d, p, n)? as i64;
    Ok(if v & (1 << (8 * n - 1)) != 0 {
        v - (1i64 << (8 * n))
    } else {
        v
    })
}
pub fn var(d: &[u8], p: &mut usize) -> Result<u32> {
    let f = uint(d, *p, 1)?;
    let (n, s) = if f & 1 != 0 {
        (1, 1)
    } else if f & 2 != 0 {
        (2, 2)
    } else if f & 4 != 0 {
        (3, 3)
    } else {
        (4, 3)
    };
    let v = uint(d, *p, n)? >> s;
    *p += n;
    Ok(v)
}
pub struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    register: u64,
    available: usize,
    byte: usize,
}
impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            register: 0,
            available: 0,
            byte: 0,
        }
    }
    pub fn remaining(&self) -> usize {
        self.data.len() * 8 - self.pos
    }
    pub fn get(&mut self, n: usize) -> Result<u32> {
        ensure!(
            n <= 32 && n <= self.remaining(),
            "Truncated coordinate bitstream at bit {}, need {n}, remaining {}",
            self.pos,
            self.remaining()
        );
        while self.available < n {
            self.register |= (self.data[self.byte] as u64) << self.available;
            self.available += 8;
            self.byte += 1;
        }
        let v = (self.register & ((1u64 << n) - 1)) as u32;
        self.register >>= n;
        self.available -= n;
        self.pos += n;
        Ok(v)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feature {
    pub kind: String,
    #[serde(rename = "type")]
    pub typ: u32,
    pub coords: Vec<[f64; 2]>,
    pub label: String,
    pub bbox: Bounds,
    pub tile: String,
    #[serde(default)]
    pub label_ref: u32,
    #[serde(default)]
    pub label_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_detail: Option<String>,
}
impl Feature {
    pub fn cost(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.coords.capacity() * 16
            + self.label.capacity()
            + self.tile.capacity()
            + self.kind.capacity()
            + self.label_mode.capacity()
            + self.symbol_detail.as_ref().map_or(0, |s| s.capacity())
            + 64
    }
}
#[derive(Clone, Copy, Default)]
pub struct Config {
    pub global: u32,
    pub local: [u32; 3],
}
#[derive(Clone, Debug)]
pub struct Sub {
    pub index: usize,
    pub bits: u8,
    pub shift: u8,
    pub cx: i64,
    pub cy: i64,
    pub flags: u8,
    pub start: usize,
    pub end: usize,
    pub bbox: Bounds,
    pub ext: [(usize, usize); 3],
}
fn attributes(d: &[u8], p: &mut usize, subtype: u32, c: Config, kind: u32) -> Result<Option<f64>> {
    let mut depth = None;
    if subtype & 128 != 0 {
        let flags = uint(d, *p, 1)?;
        *p += 1;
        let marker = flags >> 5;
        let size = if marker == 7 {
            var(d, p)? as usize
        } else if (4..=6).contains(&marker) {
            (marker - 3) as usize
        } else {
            0
        };
        ensure!(
            p.checked_add(size).is_some_and(|e| e <= d.len()),
            "Truncated class attributes"
        );
        if (kind == 0x10301 || kind == 0x10302) && size <= 3 {
            let (mut units, mut value) = ((flags >> 3) & 3, uint(d, *p, size)?);
            if size == 0 {
                value = flags & 63;
                units = (flags >> 5) & 2;
            } else {
                value |= (flags & (if size == 1 { 7 } else { 3 })) << (8 * size);
            }
            depth = Some(value as f64 / if units & 1 != 0 { 10.0 } else { 1.0 });
        }
        *p += size;
    }
    if subtype & 64 != 0 {
        let mut bf = if c.local[0] & 0x20000000 != 0 {
            var(d, p)?
        } else {
            u32::MAX
        };
        let mut j = 0;
        for i in 0..29 {
            if c.local[0] & (1 << i) != 0 {
                if bf & 1 != 0 {
                    let mode = (c.local[(j >> 4) + 1] >> ((j * 2) & 30)) & 3;
                    let size = if mode == 3 {
                        var(d, p)? as usize
                    } else {
                        mode as usize + 1
                    };
                    *p = p
                        .checked_add(size)
                        .ok_or_else(|| anyhow::anyhow!("Attribute overflow"))?;
                }
                bf >>= 1;
                j += 1;
            }
        }
    }
    let mut global = c.global;
    while global != 0 {
        *p += (global & 3) as usize;
        global >>= 2;
    }
    ensure!(*p <= d.len(), "Truncated object attributes");
    Ok(depth)
}
pub fn decode(
    d: &[u8],
    p: &mut usize,
    s: &Sub,
    kind: &str,
    extended: bool,
    c: Config,
) -> Result<Feature> {
    let raw = uint(d, *p, 1)?;
    let mut label = 0;
    let subtype;
    let typ;
    let dx;
    let dy;
    let mut depth = None;
    let point = kind == "point" || kind == "indexed";
    if extended {
        subtype = uint(d, *p + 1, 1)?;
        typ = 0x10000 + (raw << 8) + (subtype & 31);
        dx = sint(d, *p + 2, 2)?;
        dy = sint(d, *p + 4, 2)?;
        *p += 6;
    } else {
        subtype = 0;
        label = uint(d, *p + 1, 3)?;
        dx = sint(d, *p + 4, 2)?;
        dy = sint(d, *p + 6, 2)?;
        *p += 8;
        typ = if point {
            (raw << 8)
                + if label & 0x800000 != 0 {
                    let st = uint(d, *p, 1)?;
                    *p += 1;
                    st
                } else {
                    0
                }
        } else {
            raw & if kind == "line" { 63 } else { 127 }
        };
    }
    let scale = 1i64 << s.shift;
    let (mut x, mut y) = (s.cx + dx * scale, s.cy + dy * scale);
    let mut coords = vec![[x as f64 * UNIT, y as f64 * UNIT]];
    if !point {
        let length = if extended {
            var(d, p)?
                .checked_sub(1)
                .ok_or_else(|| anyhow::anyhow!("Invalid polygon length"))? as usize
        } else {
            let n = if raw & 128 != 0 { 2 } else { 1 };
            let v = uint(d, *p, n)? as usize;
            *p += n;
            v
        };
        let base = uint(d, *p, 1)?;
        *p += 1;
        ensure!(
            length > 0 && p.checked_add(length).is_some_and(|e| e <= d.len()),
            "Invalid polygon bitstream"
        );
        let mut bits = Bits::new(&d[*p..*p + length]);
        let mut axes = Vec::new();
        for nibble in [base & 15, base >> 4] {
            let fixed = bits.get(1)? != 0;
            let negative = if fixed { bits.get(1)? != 0 } else { false };
            let width = 2 + if nibble <= 9 { nibble } else { 2 * nibble - 9 } + u32::from(!fixed);
            axes.push((width as usize, fixed, negative));
        }
        if extended {
            bits.get(1)?;
        }
        let extra = !extended && label & 0x400000 != 0;
        let pair = axes.iter().map(|a| a.0).sum::<usize>() + usize::from(extra);
        // Escapes can consume more than the minimum pair width. As in the
        // reference decoder, stop at an incomplete trailing pair; publish only
        // complete coordinates. The declared byte length was checked above.
        'coordinates: while bits.remaining() >= pair {
            if extra {
                bits.get(1)?;
            }
            let mut delta = [0i64; 2];
            for (i, &(width, fixed, negative)) in axes.iter().enumerate() {
                if bits.remaining() < width {
                    break 'coordinates;
                }
                let mut v = bits.get(width)? as i64;
                if fixed {
                    if negative {
                        v = -v;
                    }
                } else {
                    let sign = 1i64 << (width - 1);
                    let mut extension = 0i64;
                    while v == sign {
                        extension = extension
                            .checked_add(sign - 1)
                            .ok_or_else(|| anyhow::anyhow!("Delta overflow"))?;
                        if bits.remaining() < width {
                            break 'coordinates;
                        }
                        v = bits.get(width)? as i64;
                    }
                    v = if v < sign {
                        extension + v
                    } else {
                        v - 2 * sign - extension
                    };
                }
                delta[i] = v;
            }
            x = x
                .checked_add(delta[0] * scale)
                .ok_or_else(|| anyhow::anyhow!("Coordinate overflow"))?;
            y = y
                .checked_add(delta[1] * scale)
                .ok_or_else(|| anyhow::anyhow!("Coordinate overflow"))?;
            coords.push([x as f64 * UNIT, y as f64 * UNIT]);
        }
        *p += length;
    }
    if extended && subtype & 32 != 0 {
        label = uint(d, *p, 3)?;
        *p += 3;
    }
    if extended {
        let value = attributes(d, p, subtype, c, typ)?;
        if point {
            depth = value;
        }
    }
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for v in &coords {
        bbox[0] = bbox[0].min(v[0]);
        bbox[1] = bbox[1].min(v[1]);
        bbox[2] = bbox[2].max(v[0]);
        bbox[3] = bbox[3].max(v[1]);
    }
    if !intersects(bbox, s.bbox) {
        bail!("Object outside subdivision bounds");
    }
    Ok(Feature {
        kind: if point { "point".into() } else { kind.into() },
        typ,
        coords,
        label: String::new(),
        bbox,
        tile: String::new(),
        label_ref: label & 0x3fffff,
        label_mode: if point && label & 0x400000 != 0 {
            "poi"
        } else if !point && label & 0x800000 != 0 {
            "net"
        } else {
            "normal"
        }
        .into(),
        depth_m: depth,
        symbol_detail: if point
            && extended
            && subtype & 128 != 0
            && (0x10200..0x10300).contains(&typ)
        {
            Some("buoy".into())
        } else if point && extended && subtype & 128 != 0 && (0x10100..0x10200).contains(&typ) {
            Some("light".into())
        } else {
            None
        },
    })
}
