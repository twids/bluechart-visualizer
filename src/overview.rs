// SPDX-License-Identifier: GPL-3.0-only
use crate::{catalog::Catalog, chart::Chart, img::ReadCache};
use anyhow::{ensure, Result};
use serde_json::json;
use std::path::Path;
pub fn simplify(points: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if points.len() < 4 {
        return points.to_vec();
    }
    let mut closed = points.to_vec();
    if closed.first() != closed.last() {
        closed.push(points[0]);
    }
    let mut keep = vec![false; closed.len()];
    keep[0] = true;
    let last = closed.len() - 1;
    keep[last] = true;
    let mut stack = vec![(0, last)];
    let limit = tolerance * tolerance;
    while let Some((a, b)) = stack.pop() {
        let [ax, ay] = closed[a];
        let dx = closed[b][0] - ax;
        let dy = closed[b][1] - ay;
        let length = dx * dx + dy * dy;
        let mut far = limit;
        let mut index = None;
        for (i, p) in closed.iter().enumerate().take(b).skip(a + 1) {
            let [x, y] = *p;
            let t = if length > 0.0 {
                (((x - ax) * dx + (y - ay) * dy) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let dist = (x - ax - t * dx).powi(2) + (y - ay - t * dy).powi(2);
            if dist > far {
                far = dist;
                index = Some(i);
            }
        }
        if let Some(i) = index {
            keep[i] = true;
            stack.push((a, i));
            stack.push((i, b));
        }
    }
    let out: Vec<_> = closed
        .into_iter()
        .zip(keep)
        .enumerate()
        .filter(|(i, (_, k))| *k && *i != last)
        .map(|(_, (p, _))| p)
        .collect();
    if out.len() >= 3 {
        out
    } else {
        vec![]
    }
}
pub fn ensure_overviews(catalog: &Catalog, cancel: &dyn Fn() -> bool, status: &dyn Fn(bool)) {
    for entry in &catalog.entries {
        if cancel() {
            break;
        }
        if entry.policy.role != "base" {
            continue;
        }
        let Some(chart) = &entry.chart else { continue };
        let Some(path) = &chart.overview_destination else {
            continue;
        };
        if chart.metadata()["overview_available"] == true {
            continue;
        }
        eprintln!("Overview {}: building missing or invalid cache", entry.name);
        status(true);
        match build_with_cancel(chart, path, cancel) {
            Ok(()) => chart.publish_overview(path),
            Err(error) => eprintln!("Overview {}: build failed: {error:#}", entry.name),
        }
        status(false);
    }
}
pub fn build(chart: &Chart, path: &Path) -> Result<()> {
    build_with_cancel(chart, path, &|| false)
}
fn build_with_cancel(chart: &Chart, path: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    ensure!(!cancel(), "Overview build stopped during shutdown");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("sqlite3.tmp");
    ensure!(
        !temp.exists(),
        "Incomplete overview exists; inspect {}",
        temp.display()
    );
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let built = (|| -> Result<()> {
        let mut db = rusqlite::Connection::open(&temp)?;
        db.execute_batch("CREATE TABLE metadata(payload TEXT); CREATE TABLE overview(id INTEGER PRIMARY KEY,level INTEGER,payload TEXT); CREATE INDEX level_idx ON overview(level); CREATE VIRTUAL TABLE bounds USING rtree(id,west,east,south,north);")?;
        let tx = db.transaction()?;
        let mut counts = [0usize; 3];
        let mut labels = ReadCache::default();
        let mut processed = 0;
        // Prefer detailed marine tiles, not coarse B tiles or fishing/auxiliary tiles.
        let tiles: Vec<_> = chart
            .tiles
            .iter()
            .filter(|t| t.id.starts_with('M'))
            .collect();
        for t in tiles {
            ensure!(!cancel(), "Overview build stopped during shutdown");
            let Some(bits) = t
                .levels
                .iter()
                .filter(|l| !l.inherited)
                .map(|l| l.bits)
                .min()
            else {
                continue;
            };
            let subs = chart.subs(t)?;
            for sub in subs.iter().filter(|s| s.bits == bits) {
                ensure!(!cancel(), "Overview build stopped during shutdown");
                let payload = chart.decode_sub(t, sub, &mut labels, cancel)?;
                ensure!(
                    payload.warnings.is_empty(),
                    "Overview not published: {}",
                    payload.warnings.join("; ")
                );
                processed += 1;
                for f in &payload.features {
                    if f.kind != "polygon" || f.typ != 0x10101 {
                        continue;
                    }
                    for (i, (level, tolerance)) in [(13, 0.04), (15, 0.01), (17, 0.002)]
                        .into_iter()
                        .enumerate()
                    {
                        if (f.bbox[2] - f.bbox[0]).max(f.bbox[3] - f.bbox[1]) < tolerance {
                            continue;
                        }
                        let points = simplify(&f.coords, tolerance);
                        if points.is_empty() {
                            continue;
                        }
                        let mut feature = f.clone();
                        feature.coords = points
                            .into_iter()
                            .map(|p| p.map(|v| (v * 1e6).round() / 1e6))
                            .collect();
                        feature.label.clear();
                        tx.execute(
                            "INSERT INTO overview(level,payload) VALUES (?,?)",
                            rusqlite::params![level, serde_json::to_string(&feature)?],
                        )?;
                        tx.execute(
                            "INSERT INTO bounds VALUES (?,?,?,?,?)",
                            rusqlite::params![
                                tx.last_insert_rowid(),
                                f.bbox[0],
                                f.bbox[2],
                                f.bbox[1],
                                f.bbox[3]
                            ],
                        )?;
                        counts[i] += 1;
                    }
                }
            }
            eprintln!("{}: {processed} subdivisions", t.id);
        }
        ensure!(counts[2]>0,"No land polygons available for overview; auxiliary charts do not provide a complete overview");
        let mtime = chart
            .img
            .path
            .metadata()?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        tx.execute(
            "INSERT INTO metadata VALUES (?)",
            [
                json!({"format":1,"size":chart.img.size,"mtime":mtime,"title":chart.img.title})
                    .to_string(),
            ],
        )?;
        ensure!(!cancel(), "Overview build stopped during shutdown");
        tx.commit()?;
        let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        ensure!(integrity == "ok", "Overview integrity check failed");
        drop(db);
        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        std::fs::rename(&temp, path)?;
        println!(
            "{}",
            json!({"features_per_level":counts,"subdivisions":processed})
        );
        Ok(())
    })();
    if built.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    built
}
