// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    format::*,
    img::{Img, Part, ReadCache},
};
use anyhow::{ensure, Context, Result};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    num::NonZeroUsize,
    path::Path,
    sync::{atomic::Ordering, Arc, Mutex},
    time::Instant,
};
#[derive(Clone, Serialize)]
pub struct Level {
    pub level: u8,
    pub bits: u8,
    pub count: usize,
    pub inherited: bool,
}
pub struct Tile {
    pub id: String,
    pub bbox: Bounds,
    pub levels: Vec<Level>,
    pub tre: Part,
    pub rgn: Part,
    pub lbl: Option<Part>,
    pub h: Vec<u8>,
    pub rh: Vec<u8>,
    pub configs: [Config; 3],
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Decoded {
    pub features: Vec<Feature>,
    pub warnings: Vec<String>,
}
pub struct GeometryCache {
    items: LruCache<(String, String, usize), Arc<Decoded>>,
    pub bytes: usize,
    pub hits: u64,
    pub misses: u64,
}
impl GeometryCache {
    pub fn new() -> Self {
        Self {
            items: LruCache::unbounded(),
            bytes: 0,
            hits: 0,
            misses: 0,
        }
    }
    pub fn get(&mut self, k: &(String, String, usize)) -> Option<Arc<Decoded>> {
        let v = self.items.get(k).cloned();
        if v.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        v
    }
    pub fn put(&mut self, k: (String, String, usize), v: Arc<Decoded>) {
        let cost = Self::cost(&v);
        if cost > 64 * 1024 * 1024 {
            return;
        }
        if let Some(old) = self.items.pop(&k) {
            self.bytes -= Self::cost(&old);
        }
        while self.bytes + cost > 64 * 1024 * 1024 {
            if let Some((_, old)) = self.items.pop_lru() {
                self.bytes -= Self::cost(&old);
            } else {
                break;
            }
        }
        self.bytes += cost;
        self.items.put(k, v);
    }
    fn cost(v: &Decoded) -> usize {
        256 + v.features.capacity() * std::mem::size_of::<Feature>()
            + v.features.iter().map(Feature::cost).sum::<usize>()
            + v.warnings.iter().map(|s| s.capacity() + 24).sum::<usize>()
    }
}
pub type SharedCache = Arc<Mutex<GeometryCache>>;
pub struct Chart {
    pub img: Img,
    pub tiles: Vec<Tile>,
    pub warnings: Vec<String>,
    pub id: String,
    pub revision: String,
    pub coverage: Bounds,
    pub cache: SharedCache,
    index: Mutex<LruCache<String, Arc<Vec<Sub>>>>,
    overview: Mutex<Option<std::path::PathBuf>>,
    pub overview_destination: Option<std::path::PathBuf>,
}
impl Chart {
    pub fn open(
        path: &Path,
        id: String,
        overview: Option<&Path>,
        cache: SharedCache,
    ) -> Result<Self> {
        let img = Img::open(path)?;
        let mut tiles = Vec::new();
        let mut warnings = Vec::new();
        for ((name, kind), p) in &img.parts {
            if kind != "TRE" {
                continue;
            }
            match Self::tile(&img, p) {
                Ok(t) => tiles.push(t),
                Err(e) => warnings.push(format!("{name}: {e}")),
            }
        }
        ensure!(
            !tiles.is_empty(),
            "No readable TRE/RGN map tiles: {}",
            warnings
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
        let mut coverage = [180.0f64, 85.0, -180.0, -85.0];
        for t in &tiles {
            coverage[0] = coverage[0].min(t.bbox[0]);
            coverage[1] = coverage[1].min(t.bbox[1]);
            coverage[2] = coverage[2].max(t.bbox[2]);
            coverage[3] = coverage[3].max(t.bbox[3]);
        }
        let mtime = img
            .path
            .metadata()?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let revision = format!("{}:{}:{}", id, img.size, mtime);
        let mut valid_overview = None;
        if let Some(p) = overview.filter(|p| p.is_file()) {
            let check = (|| -> Result<()> {
                let db = rusqlite::Connection::open_with_flags(
                    p,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                let payload: String =
                    db.query_row("SELECT payload FROM metadata", [], |r| r.get(0))?;
                let value: Value = serde_json::from_str(&payload)?;
                ensure!(
                    value == json!({"format":1,"size":img.size,"mtime":mtime,"title":img.title}),
                    "Overview cache belongs to another chart revision"
                );
                Ok(())
            })();
            match check {
                Ok(()) => valid_overview = Some(p.into()),
                Err(e) => eprintln!("Overview: {e}"),
            }
        }
        Ok(Self {
            img,
            tiles,
            warnings,
            id,
            revision,
            coverage,
            cache,
            index: Mutex::new(LruCache::new(NonZeroUsize::new(4).unwrap())),
            overview: Mutex::new(valid_overview),
            overview_destination: overview.map(Path::to_path_buf),
        })
    }
    pub fn publish_overview(&self, path: &Path) {
        *self.overview.lock().unwrap() = Some(path.to_path_buf());
    }
    fn tile(img: &Img, tre: &Part) -> Result<Tile> {
        let h = img.header(tre, 0x121)?;
        ensure!(
            h.len() >= 0x35 && &h[2..12] == b"GARMIN TRE" && h[13] & 128 == 0,
            "Unsupported or locked TRE"
        );
        let rgn = img
            .parts
            .get(&(tre.name.clone(), "RGN".into()))
            .context("Missing RGN")?
            .clone();
        let rh = img.header(&rgn, 0x7d)?;
        ensure!(
            rh.len() >= 0x1d && &rh[2..12] == b"GARMIN RGN",
            "Invalid RGN header"
        );
        ensure!(
            !(rh.len() >= 125 && uint(&rh, 0x75, 4)? != 0 && uint(&rh, 0x79, 4)? & 30 != 0),
            "Huffman coordinate encoding is not supported"
        );
        let mut configs = [Config::default(); 3];
        for (i, at) in [0x29, 0x45, 0x61].into_iter().enumerate() {
            if rh.len() >= at + 16 {
                configs[i] = Config {
                    global: uint(&rh, at, 4)?,
                    local: [
                        uint(&rh, at + 4, 4)?,
                        uint(&rh, at + 8, 4)?,
                        uint(&rh, at + 12, 4)?,
                    ],
                };
            }
        }
        let raw = img.part(
            tre,
            uint(&h, 0x21, 4)? as usize,
            uint(&h, 0x25, 4)? as usize,
        )?;
        ensure!(
            !raw.is_empty() && raw.len() % 4 == 0,
            "Misaligned map levels"
        );
        let mut levels = Vec::new();
        for v in raw.chunks_exact(4) {
            ensure!((1..=24).contains(&v[1]), "Invalid coordinate resolution");
            levels.push(Level {
                level: v[0] & 15,
                bits: v[1],
                count: uint(v, 2, 2)? as usize,
                inherited: v[0] & 128 != 0,
            });
        }
        let bbox = [
            sint(&h, 0x1e, 3)? as f64 * UNIT,
            sint(&h, 0x1b, 3)? as f64 * UNIT,
            sint(&h, 0x18, 3)? as f64 * UNIT,
            sint(&h, 0x15, 3)? as f64 * UNIT,
        ];
        Ok(Tile {
            id: tre.name.clone(),
            bbox,
            levels,
            tre: tre.clone(),
            rgn,
            lbl: img.parts.get(&(tre.name.clone(), "LBL".into())).cloned(),
            h,
            rh,
            configs,
        })
    }
    pub fn subs(&self, t: &Tile) -> Result<Arc<Vec<Sub>>> {
        if let Some(v) = self.index.lock().unwrap().get(&t.id).cloned() {
            return Ok(v);
        }
        let h = &t.h;
        let rh = &t.rh;
        let rgn_offset = uint(rh, 0x15, 4)? as usize;
        let rgn_length = uint(rh, 0x19, 4)? as usize;
        let raw = self.img.part(
            &t.tre,
            uint(h, 0x29, 4)? as usize,
            uint(h, 0x2d, 4)? as usize,
        )?;
        let mut subs = Vec::new();
        let mut pos = 0;
        for (li, l) in t.levels.iter().enumerate() {
            for _ in 0..l.count {
                let shift = 24 - l.bits;
                let cx = sint(&raw, pos + 4, 3)?;
                let cy = sint(&raw, pos + 7, 3)?;
                let w = ((uint(&raw, pos + 10, 2)? & 32767) as i64) << shift;
                let ht = ((uint(&raw, pos + 12, 2)? & 32767) as i64) << shift;
                subs.push(Sub {
                    index: subs.len(),
                    bits: l.bits,
                    shift,
                    cx,
                    cy,
                    flags: uint(&raw, pos + 3, 1)? as u8,
                    start: rgn_offset + (uint(&raw, pos, 4)? & 0x0fffffff) as usize,
                    end: 0,
                    bbox: [
                        (cx - w) as f64 * UNIT,
                        (cy - ht) as f64 * UNIT,
                        (cx + w + 1) as f64 * UNIT,
                        (cy + ht + 1) as f64 * UNIT,
                    ],
                    ext: [(0, 0); 3],
                });
                pos += if li == t.levels.len() - 1 { 14 } else { 16 };
            }
        }
        for i in 0..subs.len() {
            let end = if i + 1 < subs.len() {
                subs[i + 1].start
            } else {
                rgn_offset + rgn_length
            };
            ensure!(
                rgn_offset <= subs[i].start
                    && subs[i].start <= end
                    && end <= rgn_offset + rgn_length
                    && rgn_offset + rgn_length <= t.rgn.size,
                "Invalid subdivision RGN range"
            );
            subs[i].end = end;
        }
        if h.len() >= 0x9a && uint(h, 0x80, 4)? != 0 {
            let size = uint(h, 0x84, 2)? as usize;
            ensure!(
                (4..=64).contains(&size),
                "Unsupported TRE7 record size {size}"
            );
            let ext = self.img.part(
                &t.tre,
                uint(h, 0x7c, 4)? as usize,
                uint(h, 0x80, 4)? as usize,
            )?;
            ensure!(ext.len() % size == 0, "Misaligned TRE7 table");
            let first = (subs.len() + 1)
                .checked_sub(ext.len() / size)
                .context("Too many TRE7 records")?;
            let flags = uint(h, 0x86, 4)?;
            let mut at = 0;
            for (i, bit, hoff) in [(0, 1, 0x1d), (1, 2, 0x39), (2, 4, 0x55)] {
                if flags & bit == 0 {
                    continue;
                }
                let base = uint(rh, hoff, 4)? as usize;
                let length = uint(rh, hoff + 4, 4)? as usize;
                ensure!(
                    base + length <= t.rgn.size && at + 4 <= size,
                    "Extended RGN outside subfile"
                );
                for (j, sub) in subs.iter_mut().enumerate().skip(first) {
                    let start = uint(&ext, (j - first) * size + at, 4)? as usize;
                    let end = uint(&ext, (j - first + 1) * size + at, 4)? as usize;
                    ensure!(start <= end && end <= length, "Invalid TRE7 range");
                    sub.ext[i] = (base + start, end - start);
                }
                at += 4;
            }
        }
        let subs = Arc::new(subs);
        self.index.lock().unwrap().put(t.id.clone(), subs.clone());
        Ok(subs)
    }
    fn label(
        &self,
        t: &Tile,
        mut reference: u32,
        mode: &str,
        cache: &mut ReadCache,
    ) -> Result<String> {
        if reference == 0 || mode == "net" {
            return Ok(String::new());
        }
        let key = (t.id.clone(), reference, mode.to_string());
        if let Some(s) = cache.labels.get(&key) {
            return Ok(s.clone());
        }
        let Some(part) = &t.lbl else {
            return Ok(String::new());
        };
        let h = self.img.header(part, 0xac)?;
        ensure!(
            h.len() >= 0x1f && &h[2..12] == b"GARMIN LBL",
            "Invalid LBL header"
        );
        if mode == "poi" {
            let base = uint(&h, 0x57, 4)? as usize;
            let off = base + reference as usize;
            if off + 3 > base + uint(&h, 0x5b, 4)? as usize {
                return Ok(String::new());
            }
            reference = uint(&self.img.part(part, off, 3)?, 0, 3)? & 0x3fffff;
        }
        let offset = (reference as usize)
            .checked_shl(h[0x1d] as u32)
            .context("Invalid label shift")?;
        let length = uint(&h, 0x19, 4)? as usize;
        if offset >= length {
            return Ok(String::new());
        }
        let data = self.img.part(
            part,
            uint(&h, 0x15, 4)? as usize + offset,
            256.min(length - offset),
        )?;
        let text = match h[0x1e] {
            9 | 10 => {
                let cp = if h.len() >= 0xac {
                    uint(&h, 0xaa, 2)?
                } else {
                    1252
                };
                let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
                let encoding = if h[0x1e] == 10 || cp == 65001 {
                    encoding_rs::UTF_8
                } else {
                    encoding_rs::Encoding::for_label(
                        format!("windows-{}", if cp == 0 { 1252 } else { cp }).as_bytes(),
                    )
                    .unwrap_or(encoding_rs::WINDOWS_1252)
                };
                encoding.decode(&data[..end]).0.into_owned()
            }
            6 => {
                let mut out = String::new();
                let mut escape = None;
                let table = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ\0\0\0\0\0";
                for pos in (0..data.len() * 8 - 5).step_by(6) {
                    let mut c = 0u8;
                    for bit in pos..pos + 6 {
                        c = (c << 1) | ((data[bit / 8] >> (7 - bit % 8)) & 1);
                    }
                    if c > 47 {
                        break;
                    }
                    if let Some(e) = escape.take() {
                        let ch = if e == 27 && c <= 26 {
                            Some(96 + c)
                        } else if e == 28 && (42..=46).contains(&c) {
                            Some(64 + c)
                        } else if e == 28 && (c <= 15 || (26..=31).contains(&c)) {
                            Some(32 + c)
                        } else {
                            None
                        };
                        if let Some(ch) = ch {
                            out.push(ch as char);
                        }
                    } else if c == 27 || c == 28 {
                        escape = Some(c);
                    } else if (29..=31).contains(&c) {
                        out.push(' ');
                    } else if c < 32 {
                        out.push(table[c as usize] as char);
                    } else if c <= 41 {
                        out.push((b'0' + c - 32) as char);
                    } else {
                        out.push('\0');
                    }
                }
                out
            }
            _ => String::new(),
        };
        let text = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
            .trim()
            .to_string();
        if cache.labels.len() >= 8192 {
            cache.labels.clear();
        }
        cache.labels.insert(key, text.clone());
        Ok(text)
    }
    pub fn decode_sub(
        &self,
        t: &Tile,
        s: &Sub,
        labels: &mut ReadCache,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Arc<Decoded>> {
        let key = (self.revision.clone(), t.id.clone(), s.index);
        if let Some(v) = self.cache.lock().unwrap().get(&key) {
            return Ok(v);
        }
        let mut result = Decoded::default();
        let raw = self.img.part(&t.rgn, s.start, s.end - s.start)?;
        let kinds: Vec<_> = [
            (16, "point"),
            (32, "indexed"),
            (64, "line"),
            (128, "polygon"),
        ]
        .into_iter()
        .filter(|(flag, _)| s.flags & flag != 0)
        .map(|(_, k)| k)
        .collect();
        let mut sections = Vec::new();
        if !raw.is_empty() && !kinds.is_empty() {
            let mut offsets = vec![2 * (kinds.len() - 1)];
            for i in 0..kinds.len() - 1 {
                offsets.push(uint(&raw, i * 2, 2)? as usize);
            }
            offsets.push(raw.len());
            ensure!(
                offsets.windows(2).all(|a| a[0] <= a[1]) && offsets.iter().all(|n| *n <= raw.len()),
                "Invalid subdivision section offsets"
            );
            for (i, k) in kinds.iter().enumerate() {
                sections.push((*k, raw[offsets[i]..offsets[i + 1]].to_vec(), false));
            }
        }
        for (i, k) in ["polygon", "line", "point"].into_iter().enumerate() {
            let (off, len) = s.ext[i];
            if len > 0 {
                sections.push((k, self.img.part(&t.rgn, off, len)?, true));
            }
        }
        for (kind, data, extended) in sections {
            let mut p = 0;
            while p < data.len() {
                ensure!(!cancel(), "Superseded map request");
                let read = (|| -> Result<Feature> {
                    let config = t.configs[match kind {
                        "polygon" => 0,
                        "line" => 1,
                        _ => 2,
                    }];
                    let mut f = decode(&data, &mut p, s, kind, extended, config)?;
                    f.label = self.label(t, f.label_ref, &f.label_mode, labels)?;
                    f.tile = t.id.clone();
                    Ok(f)
                })();
                match read {
                    Ok(f) => result.features.push(f),
                    Err(e) => {
                        result
                            .warnings
                            .push(format!("{}/{} {kind}: {e}", t.id, s.index));
                        break;
                    }
                }
            }
        }
        let result = Arc::new(result);
        self.cache.lock().unwrap().put(key, result.clone());
        Ok(result)
    }
    pub fn features(
        &self,
        bbox: Bounds,
        bits: u8,
        fishing: bool,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Value> {
        self.features_limited(bbox, bits, fishing, cancel, 180)
    }
    fn selection(
        &self,
        bbox: Bounds,
        bits: u8,
        fishing: bool,
        cancel: &dyn Fn() -> bool,
        limit: usize,
    ) -> Result<(Vec<(usize, Sub)>, Vec<String>)> {
        let mut selected = Vec::new();
        let mut errors = Vec::new();
        let base = self.tiles.iter().any(|t| t.id.starts_with('B'));
        for (i, t) in self.tiles.iter().enumerate() {
            ensure!(!cancel(), "Superseded map request");
            if !intersects(t.bbox, bbox)
                || (base && t.id.starts_with('B') != (bits <= 17))
                || (!fishing && t.id.starts_with('F'))
            {
                continue;
            }
            let subs = match self.subs(t) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("{}: {e}", t.id));
                    continue;
                }
            };
            let Some(level) = t
                .levels
                .iter()
                .filter(|l| !l.inherited && l.count > 0)
                .min_by_key(|l| (l.bits.abs_diff(bits), l.bits))
            else {
                continue;
            };
            for sub in subs.iter() {
                if sub.bits == level.bits && intersects(sub.bbox, bbox) {
                    selected.push((i, sub.clone()));
                    if selected.len() > limit {
                        return Ok((selected, errors));
                    }
                }
            }
        }
        Ok((selected, errors))
    }
    pub fn features_limited(
        &self,
        bbox: Bounds,
        bits: u8,
        fishing: bool,
        cancel: &dyn Fn() -> bool,
        limit: usize,
    ) -> Result<Value> {
        let start = Instant::now();
        let before = self.img.bytes.load(Ordering::Relaxed);
        let (hits, misses) = {
            let c = self.cache.lock().unwrap();
            (c.hits, c.misses)
        };
        let span = (bbox[2] - bbox[0]).max(bbox[3] - bbox[1]);
        let mut effective = bits.min(if span > 20.0 {
            13
        } else if span > 5.0 {
            15
        } else if span > 2.0 {
            17
        } else {
            bits
        });
        let (mut selected, mut errors) = loop {
            let (s, e) = self.selection(bbox, effective, fishing, cancel, limit)?;
            if s.len() <= limit || effective <= 17 {
                break (s, e);
            }
            effective -= 1;
        };
        let mut result = Vec::new();
        let mut used_overview = false;
        if effective <= 17 {
            if let Some(path) = self.overview.lock().unwrap().clone() {
                let db = rusqlite::Connection::open_with_flags(
                    path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                db.execute_batch("PRAGMA cache_size=-8192; PRAGMA query_only=ON;")?;
                let level = [13u8, 15, 17]
                    .into_iter()
                    .min_by_key(|v| v.abs_diff(effective))
                    .unwrap();
                let mut query=db.prepare("SELECT payload FROM overview WHERE level=? AND id IN (SELECT id FROM bounds WHERE west<=? AND east>=? AND south<=? AND north>=?)")?;
                let rows = query.query_map(
                    rusqlite::params![level, bbox[2], bbox[0], bbox[3], bbox[1]],
                    |r| r.get::<_, String>(0),
                )?;
                for row in rows {
                    ensure!(!cancel(), "Superseded map request");
                    result.push(serde_json::from_str::<Feature>(&row?)?);
                }
                selected.clear();
                used_overview = true;
            } else {
                errors.push(
                    "Overview cache missing: building automatically at startup; check server logs if unavailable.".into(),
                );
            }
        }
        ensure!(
            selected.len() <= limit,
            "No bounded overview level is available for this map"
        );
        let mut labels = ReadCache::default();
        for (i, s) in &selected {
            ensure!(!cancel(), "Superseded map request");
            match self.decode_sub(&self.tiles[*i], s, &mut labels, cancel) {
                Ok(payload) => {
                    errors.extend(payload.warnings.clone());
                    result.extend(
                        payload
                            .features
                            .iter()
                            .filter(|f| intersects(f.bbox, bbox))
                            .cloned(),
                    );
                }
                Err(e) => {
                    ensure!(!cancel(), "Superseded map request");
                    errors.push(format!("{}/{}: {e}", self.tiles[*i].id, s.index));
                }
            }
        }
        let cache = self.cache.lock().unwrap();
        Ok(
            json!({"features":result,"warnings":errors,"chart_id":self.id,"chart_title":self.img.title,"stats":{"features":result.len(),"subdivisions":selected.len(),"effective_bits":effective,"overview_cache":used_overview,"bytes_read":self.img.bytes.load(Ordering::Relaxed)-before,"total_bytes_read":self.img.bytes.load(Ordering::Relaxed),"max_read_bytes":self.img.max_read.load(Ordering::Relaxed),"cache_estimate_bytes":cache.bytes,"cache_hits":cache.hits-hits,"cache_misses":cache.misses-misses,"seconds":start.elapsed().as_secs_f64()}}),
        )
    }
    pub fn metadata(&self) -> Value {
        json!({"chart_id":self.id,"title":self.img.title,"file_size":self.img.size,"block_size":self.img.block_size,"parts":self.img.parts.len(),"tiles":self.tiles.iter().map(|t|json!({"id":t.id,"bbox":t.bbox,"levels":t.levels,"subdivisions":t.levels.iter().map(|l|l.count).sum::<usize>()})).collect::<Vec<_>>(),"warnings":self.warnings,"initial_bbox":self.tiles[0].bbox,"coverage_bbox":self.coverage,"coverage_bounds":self.coverage,"overview_available":self.overview.lock().unwrap().is_some(),"limitations":["Technical prototype; not for navigation.","3D perspective shows the chart plane without terrain or seabed elevations.","Simplified symbols; light sectors, NET labels, satellite imagery and routing are not displayed."]})
    }
}
