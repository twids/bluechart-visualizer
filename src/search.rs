// SPDX-License-Identifier: GPL-3.0-only
use crate::{catalog::Catalog, chart::Chart, format::Feature, img::ReadCache};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

const VERSION: i64 = 2;
const LIMIT: usize = 20;

pub fn normalize(text: &str) -> String {
    text.trim()
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.search.sqlite3"))
}

fn schema(db: &Connection, revision: &str) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE metadata(version INTEGER, revision TEXT);
        CREATE TABLE places(label TEXT NOT NULL, normalized TEXT NOT NULL, kind TEXT NOT NULL,
            type INTEGER NOT NULL, lon REAL NOT NULL, lat REAL NOT NULL, bounds TEXT NOT NULL,
            x INTEGER NOT NULL, y INTEGER NOT NULL, UNIQUE(normalized,x,y));",
    )?;
    db.execute(
        "INSERT INTO metadata VALUES (?,?)",
        params![VERSION, revision],
    )?;
    Ok(())
}

fn insert(db: &Connection, f: &Feature) -> Result<()> {
    let name = f.label.trim();
    if name.is_empty() || f.coords.is_empty() || f.depth_m.is_some() {
        return Ok(());
    }
    let b = f.bbox;
    let p = if f.kind == "point" {
        f.coords[0]
    } else {
        [(b[0] + b[2]) / 2., (b[1] + b[3]) / 2.]
    };
    if !p.iter().chain(b.iter()).all(|v| v.is_finite()) || p[0].abs() > 180. || p[1].abs() > 85. {
        return Ok(());
    }
    // About a metre at the equator; collapse duplicated labels at the same location.
    db.prepare_cached("INSERT OR IGNORE INTO places VALUES (?,?,?,?,?,?,?,?,?)")?
        .execute(params![
            name,
            normalize(name),
            f.kind,
            f.typ,
            p[0],
            p[1],
            serde_json::to_string(&b)?,
            (p[0] * 100000.).round() as i64,
            (p[1] * 100000.).round() as i64
        ])?;
    Ok(())
}

// Validate before deciding to reuse an index; a corrupt or incompatible cache is rebuilt.
fn current(file: &Path, revision: &str) -> Result<()> {
    let db = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let (version, stored): (i64, String) =
        db.query_row("SELECT version,revision FROM metadata", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    ensure!(
        version == VERSION && stored == revision,
        "Stale search index"
    );
    let integrity: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    ensure!(integrity == "ok", "Invalid search index");
    db.prepare("SELECT label,normalized,kind,type,lon,lat,bounds FROM places LIMIT 0")?;
    Ok(())
}

pub fn ensure_indexes(catalog: &Catalog, root: &Path, cancel: &dyn Fn() -> bool) {
    ensure_indexes_with_status(catalog, root, cancel, &|_| {});
}

pub fn ensure_indexes_with_status(
    catalog: &Catalog,
    root: &Path,
    cancel: &dyn Fn() -> bool,
    status: &dyn Fn(bool),
) {
    for entry in &catalog.entries {
        if cancel() {
            break;
        }
        if entry.policy.role != "base" {
            continue;
        }
        let Some(chart) = &entry.chart else { continue };
        let destination = path(root, &entry.id);
        if current(&destination, &chart.revision).is_ok() {
            eprintln!("Search {}: existing index is current", entry.name);
            continue;
        }
        eprintln!(
            "Search {}: building missing, stale or invalid index",
            entry.name
        );
        status(true);
        if let Err(error) = build_with_cancel(chart, &destination, cancel) {
            eprintln!("Search {}: index build failed: {error:#}", entry.name);
        }
        status(false);
    }
}

pub fn build(chart: &Chart, destination: &Path) -> Result<()> {
    build_with_cancel(chart, destination, &|| false)
}

fn build_with_cancel(chart: &Chart, destination: &Path, cancel: &dyn Fn() -> bool) -> Result<()> {
    ensure!(!cancel(), "Search indexing stopped during shutdown");
    std::fs::create_dir_all(destination.parent().context("Missing cache directory")?)?;
    // create_new reserves the temporary path and prevents two builders sharing it.
    let temp = destination.with_extension("sqlite3.tmp");
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .context(
            "Search build temporary file exists or cannot be created; inspect before retrying",
        )?;
    drop(reservation);
    let built = (|| -> Result<()> {
        let mut db = Connection::open(&temp)?;
        schema(&db, &chart.revision)?;
        let tx = db.transaction()?;
        let mut labels = ReadCache::default();
        let mut tiles: Vec<_> = chart
            .tiles
            .iter()
            .filter(|t| !t.id.starts_with('F') && (t.id.starts_with('M') || t.id.starts_with('B')))
            .collect();
        tiles.sort_by(|a, b| a.id.cmp(&b.id));
        let mut subdivisions = 0;
        for tile in tiles {
            ensure!(!cancel(), "Search indexing stopped during shutdown");
            let bits: std::collections::HashSet<_> = tile
                .levels
                .iter()
                .filter(|l| !l.inherited)
                .map(|l| l.bits)
                .collect();
            let subs = chart.subs(tile)?;
            // Prefer detailed geometry when duplicate records exist at several levels.
            let mut selected: Vec<_> = subs.iter().filter(|s| bits.contains(&s.bits)).collect();
            selected.sort_by_key(|s| (std::cmp::Reverse(s.bits), s.index));
            for sub in selected {
                ensure!(!cancel(), "Search indexing stopped during shutdown");
                let decoded = chart.decode_sub(tile, sub, &mut labels, cancel)?;
                ensure!(
                    decoded.warnings.is_empty(),
                    "Search index not published: {}",
                    decoded.warnings.join("; ")
                );
                for f in &decoded.features {
                    insert(&tx, f)?;
                }
                subdivisions += 1;
            }
            eprintln!("Search {}: {subdivisions} subdivisions", tile.id);
        }
        ensure!(!cancel(), "Search indexing stopped during shutdown");
        tx.commit()?;
        let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        ensure!(integrity == "ok", "Search index integrity check failed");
        let count: i64 = db.query_row("SELECT count(*) FROM places", [], |r| r.get(0))?;
        drop(db);
        // Same-directory rename publishes a complete index without exposing a partial file.
        std::fs::rename(&temp, destination)?;
        println!(
            "{}",
            json!({"chart_id":chart.id,"places":count,"subdivisions":subdivisions})
        );
        Ok(())
    })();
    if built.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    built
}

#[derive(Serialize)]
struct Place {
    chart_id: String,
    chart_title: String,
    label: String,
    kind: String,
    #[serde(rename = "type")]
    typ: u32,
    coordinates: [f64; 2],
    bounds: [f64; 4],
    #[serde(skip)]
    normalized: String,
    #[serde(skip)]
    rank: u8,
}

fn query_index(
    file: &Path,
    revision: &str,
    id: &str,
    title: &str,
    query: &str,
) -> Result<Vec<Place>> {
    let db = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-2048;")?;
    let (version, stored): (i64, String) =
        db.query_row("SELECT version,revision FROM metadata", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    ensure!(
        version == VERSION && stored == revision,
        "Stale search index"
    );
    // instr/substr treat SQL wildcard characters as literal text.
    let mut stmt = db.prepare("SELECT label,normalized,kind,type,lon,lat,bounds,
        CASE WHEN normalized=?1 THEN 0 WHEN substr(normalized,1,length(?1))=?1 THEN 1 ELSE 2 END AS rank
        FROM places WHERE instr(normalized,?1)>0
        ORDER BY rank,normalized,label,lon,lat,kind,type LIMIT 20")?;
    let rows = stmt.query_map([query], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, u32>(3)?,
            r.get::<_, f64>(4)?,
            r.get::<_, f64>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, u8>(7)?,
        ))
    })?;
    let mut found = Vec::new();
    for row in rows {
        let (label, normalized, kind, typ, lon, lat, bounds, rank) = row?;
        found.push(Place {
            chart_id: id.into(),
            chart_title: title.into(),
            label,
            normalized,
            kind,
            typ,
            coordinates: [lon, lat],
            bounds: serde_json::from_str(&bounds)?,
            rank,
        });
    }
    Ok(found)
}

pub fn validate(query: Option<&str>) -> Result<String> {
    let query = query.context("Missing q")?.trim();
    ensure!(
        (2..=100).contains(&query.chars().count()) && !query.chars().any(char::is_control),
        "Search requires 2–100 characters"
    );
    let normalized = normalize(query);
    ensure!(
        normalized.chars().count() >= 2,
        "Search requires at least two searchable characters"
    );
    Ok(normalized)
}

pub fn scope(catalog: &Catalog, requested: Option<&str>) -> Result<Vec<usize>> {
    if requested == Some("auto") {
        Ok(catalog
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.chart.is_some() && e.policy.role == "base" && e.policy.status == "supported"
            })
            .map(|(i, _)| i)
            .collect())
    } else {
        let i = catalog.select(requested, None)?;
        ensure!(
            catalog.entries[i].policy.role == "base",
            "Search requires a base chart"
        );
        Ok(vec![i])
    }
}

pub fn search(catalog: &Catalog, root: &Path, indices: &[usize], query: &str) -> Value {
    let mut results = Vec::new();
    let mut warnings = Vec::new();
    for &i in indices {
        let c = catalog.entries[i].chart.as_ref().unwrap();
        match query_index(&path(root,&c.id), &c.revision, &c.id, &c.img.title, query) {
            Ok(places)=>results.extend(places),
            Err(_)=>warnings.push(format!("Search index unavailable or stale for {}. Indexes build automatically at startup; if still unavailable, check the server logs.", c.img.title)),
        }
    }
    results.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then(a.normalized.cmp(&b.normalized))
            .then(a.label.cmp(&b.label))
            .then(a.chart_id.cmp(&b.chart_id))
            .then(a.coordinates[0].total_cmp(&b.coordinates[0]))
            .then(a.coordinates[1].total_cmp(&b.coordinates[1]))
    });
    results.truncate(LIMIT);
    json!({"results":results,"warnings":warnings})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn feature(label: &str, lon: f64) -> Feature {
        Feature {
            kind: "point".into(),
            typ: 0x10500,
            coords: vec![[lon, 59.]],
            label: label.into(),
            bbox: [lon, 59., lon, 59.],
            tile: "M0000001".into(),
            label_ref: 0,
            label_mode: String::new(),
            depth_m: None,
            symbol_detail: None,
        }
    }
    #[test]
    fn matching_ranking_dedup_and_revision() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("index.sqlite3");
        let db = Connection::open(&file).unwrap();
        schema(&db, "rev").unwrap();
        for (name, lon) in [
            ("Östergård", 18.),
            ("OSTERGARD", 18.),
            ("Östergård", 19.),
            ("Östergården", 18.),
            ("Norra Östergård", 18.),
        ] {
            insert(&db, &feature(name, lon)).unwrap();
        }
        let rows = query_index(&file, "rev", "id", "chart", &normalize(" ÖSTERGÅRD ")).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter().map(|r| r.rank).collect::<Vec<_>>(),
            vec![0, 0, 1, 2]
        );
        assert_eq!(rows[0].coordinates, [18., 59.]);
        assert_eq!(rows[1].coordinates, [19., 59.]);
        assert!(query_index(&file, "other", "id", "chart", "ostergard").is_err());
        assert!(query_index(
            &dir.path().join("missing"),
            "rev",
            "id",
            "chart",
            "ostergard"
        )
        .is_err());
        assert!(!dir.path().join("missing").exists());
        db.execute("UPDATE metadata SET version=999", []).unwrap();
        assert!(query_index(&file, "rev", "id", "chart", "ostergard").is_err());
    }
    #[test]
    fn literal_queries_limits_and_unicode() {
        assert_eq!(normalize("ÅÄÖ åäö é A\u{030a}"), "aao aao e a");
        for q in [
            None,
            Some("a"),
            Some(" \t "),
            Some("a\nxx"),
            Some("\u{0301}\u{0308}"),
        ] {
            assert!(validate(q).is_err());
        }
        assert!(validate(Some(&"a".repeat(101))).is_err());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("index.sqlite3");
        let db = Connection::open(&file).unwrap();
        schema(&db, "rev").unwrap();
        for i in 0..30 {
            insert(&db, &feature(&format!("Island {i:02}"), i as f64)).unwrap();
        }
        insert(&db, &feature("100%_harbour", 18.)).unwrap();
        let rows = query_index(&file, "rev", "id", "chart", "island").unwrap();
        assert_eq!(rows.len(), 20);
        assert_eq!(rows[0].label, "Island 00");
        assert_eq!(rows[19].label, "Island 19");
        assert_eq!(
            query_index(&file, "rev", "id", "chart", "%_")
                .unwrap()
                .len(),
            1
        );
        assert!(query_index(&file, "rev", "id", "chart", "' OR 1=1 --")
            .unwrap()
            .is_empty());
    }
}
