// SPDX-License-Identifier: GPL-3.0-only
use crate::format::*;
use anyhow::{bail, ensure, Context, Result};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
#[derive(Clone)]
pub struct Part {
    pub name: String,
    pub size: usize,
    pub blocks: Arc<Vec<u32>>,
    pub header: usize,
}
pub struct Img {
    pub path: PathBuf,
    file: File,
    pub size: u64,
    pub block_size: usize,
    pub title: String,
    pub parts: BTreeMap<(String, String), Part>,
    pub bytes: AtomicU64,
    pub max_read: AtomicU64,
}
impl Img {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        let mut img = Self {
            path: path.into(),
            file,
            size,
            block_size: 512,
            title: String::new(),
            parts: BTreeMap::new(),
            bytes: AtomicU64::new(0),
            max_read: AtomicU64::new(0),
        };
        let h = img.read(0, 512)?;
        ensure!(
            h[0] == 0 && &h[16..23] == b"DSKIMG\0" && &h[65..72] == b"GARMIN\0",
            "Unsupported IMG signature or XOR encoding"
        );
        let exp = h[0x61] as u32 + h[0x62] as u32;
        ensure!((9..=20).contains(&exp), "Invalid IMG block size");
        img.block_size = 1 << exp;
        let mut title = h[0x49..0x5d].to_vec();
        title.extend(&h[0x65..0x83]);
        img.title = encoding_rs::WINDOWS_1252
            .decode(&title)
            .0
            .trim()
            .to_string();
        let mut dir = None;
        for off in (0x600..size.min(65536)).step_by(512) {
            let e = img.read(off, 512)?;
            if e[0] == 1 && e[1..12] == [b' '; 11] {
                dir = Some((off, uint(&e, 12, 4)? as u64));
                break;
            }
        }
        let (start, end) = dir.context("Cannot locate IMG allocation directory")?;
        ensure!(
            start < end && end <= size.min(16 * 1024 * 1024),
            "Invalid allocation directory bounds"
        );
        type Chunks = BTreeMap<u32, (usize, Vec<u32>)>;
        let mut records: BTreeMap<(String, String), Chunks> = BTreeMap::new();
        for off in ((start + 512)..end).step_by(512) {
            let e = img.read(off, 512)?;
            if e[0] != 1 {
                continue;
            }
            ensure!(e[1..12].is_ascii(), "Invalid directory name");
            let name = String::from_utf8(e[1..9].to_vec())?.trim().to_string();
            let kind = String::from_utf8(e[9..12].to_vec())?.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let part = e[17] as u32;
            let mut blocks = Vec::new();
            for p in (32..512).step_by(2) {
                let b = uint(&e, p, 2)?;
                if b != 65535 {
                    blocks.push(b);
                }
            }
            ensure!(
                records
                    .entry((name, kind))
                    .or_default()
                    .insert(part, (uint(&e, 12, 4)? as usize, blocks))
                    .is_none(),
                "Duplicate FAT record"
            );
        }
        for ((name, kind), chunks) in records {
            ensure!(
                chunks.keys().copied().eq(0..chunks.len() as u32),
                "Missing FAT continuation"
            );
            let size = chunks[&0].0;
            let blocks: Vec<u32> = chunks.into_values().flat_map(|x| x.1).collect();
            ensure!(size <= blocks.len() * img.block_size, "Too few blocks");
            ensure!(
                blocks
                    .iter()
                    .all(|b| (*b as u64) * (img.block_size as u64) < img.size),
                "Block outside IMG"
            );
            img.parts.insert(
                (name.clone(), kind),
                Part {
                    name,
                    size,
                    blocks: Arc::new(blocks),
                    header: 0,
                },
            );
        }
        // GMP offsets point to embedded headers; all data offsets remain GMP-relative.
        let gmps: Vec<_> = img
            .parts
            .iter()
            .filter(|(k, _)| k.1 == "GMP")
            .map(|(_, p)| p.clone())
            .collect();
        for gmp in gmps {
            let h = img.part(&gmp, 0, 64)?;
            ensure!(&h[2..12] == b"GARMIN GMP", "Invalid GMP header");
            for (kind, at) in [("TRE", 0x19), ("RGN", 0x1d), ("LBL", 0x21)] {
                let base = uint(&h, at, 4)? as usize;
                if base == 0 {
                    continue;
                }
                ensure!(base + 14 <= gmp.size, "Embedded header outside GMP");
                let mut p = gmp.clone();
                p.header = base;
                ensure!(
                    !img.parts.contains_key(&(p.name.clone(), kind.into())),
                    "Ambiguous GMP subfile"
                );
                img.parts.insert((p.name.clone(), kind.into()), p);
            }
        }
        Ok(img)
    }
    pub fn read(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        ensure!(
            len <= 16 * 1024 * 1024
                && offset
                    .checked_add(len as u64)
                    .is_some_and(|e| e <= self.size),
            "Invalid physical read"
        );
        let mut out = vec![0; len];
        let mut done = 0;
        while done < len {
            #[cfg(unix)]
            let n = {
                use std::os::unix::fs::FileExt;
                self.file.read_at(&mut out[done..], offset + done as u64)?
            };
            #[cfg(windows)]
            let n = {
                use std::os::windows::fs::FileExt;
                self.file
                    .seek_read(&mut out[done..], offset + done as u64)?
            };
            if n == 0 {
                bail!("Short physical read");
            }
            done += n;
        }
        self.bytes.fetch_add(len as u64, Ordering::Relaxed);
        self.max_read.fetch_max(len as u64, Ordering::Relaxed);
        Ok(out)
    }
    pub fn part(&self, p: &Part, offset: usize, len: usize) -> Result<Vec<u8>> {
        ensure!(
            len <= 16 * 1024 * 1024 && offset.checked_add(len).is_some_and(|e| e <= p.size),
            "Invalid subfile read"
        );
        let mut out = Vec::with_capacity(len);
        let (mut off, mut left) = (offset, len);
        while left > 0 {
            let i = off / self.block_size;
            let within = off % self.block_size;
            let n = left.min(self.block_size - within);
            let block = *p.blocks.get(i).context("Missing physical block")?;
            out.extend(self.read(block as u64 * self.block_size as u64 + within as u64, n)?);
            off += n;
            left -= n;
        }
        Ok(out)
    }
    pub fn header(&self, p: &Part, len: usize) -> Result<Vec<u8>> {
        self.part(p, p.header, len.min(p.size - p.header))
    }
}
#[derive(Default)]
pub struct ReadCache {
    pub labels: HashMap<(String, u32, String), String>,
}
