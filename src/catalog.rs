// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    chart::{Chart, SharedCache},
    format::*,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub role: String,
    pub status: String,
    pub priority: i32,
    pub companion_of: Option<String>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            role: "base".into(),
            status: "partial".into(),
            priority: 0,
            companion_of: None,
        }
    }
}
pub struct Entry {
    pub id: String,
    pub name: String,
    pub chart: Option<Arc<Chart>>,
    pub error: Option<String>,
    pub policy: Policy,
}
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub default: usize,
}
pub fn id_for(name: &str) -> String {
    format!("{:x}", Sha256::digest(name.as_bytes()))[..16].into()
}
impl Catalog {
    pub fn discover(
        chart: Option<&Path>,
        dirs: &[PathBuf],
        cache_dir: &Path,
        overview: Option<&Path>,
        config: Option<&Path>,
        cache: SharedCache,
    ) -> Result<Self> {
        let policies: BTreeMap<String, Policy> = if let Some(path) = config {
            serde_json::from_slice(&std::fs::read(path)?)?
        } else {
            BTreeMap::new()
        };
        for p in policies.values() {
            ensure!(
                ["base", "overlay"].contains(&p.role.as_str())
                    && ["supported", "partial"].contains(&p.status.as_str()),
                "Invalid catalog policy"
            );
        }
        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        if let Some(p) = chart {
            let absolute = p.canonicalize()?;
            let name = dirs
                .iter()
                .filter_map(|d| d.canonicalize().ok())
                .find_map(|d| absolute.strip_prefix(d).ok().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from(p.file_name().expect("chart filename")));
            paths.push((
                p.to_path_buf(),
                name.to_string_lossy().replace('\\', "/"),
                true,
            ));
        }
        for dir in dirs {
            ensure!(
                dir.is_dir(),
                "Chart directory does not exist: {}",
                dir.display()
            );
            let mut found = Vec::new();
            for item in walkdir::WalkDir::new(dir).follow_links(false) {
                let item = item?;
                if item.file_type().is_file()
                    && item
                        .path()
                        .extension()
                        .is_some_and(|s| s.eq_ignore_ascii_case("img"))
                {
                    found.push(item.path().to_path_buf());
                }
            }
            found.sort();
            for p in found {
                let name = p.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
                paths.push((p, name, false));
            }
        }
        let mut entries = Vec::new();
        let mut default = None;
        for (path, name, explicit) in paths {
            let canonical = path.canonicalize()?;
            if !seen.insert(canonical) {
                continue;
            }
            let id = id_for(&name);
            ensure!(
                !entries.iter().any(|e: &Entry| e.id == id),
                "Duplicate chart relative path; use one common charts directory"
            );
            let overview_path = cache_dir.join(format!("{id}.sqlite3"));
            let selected_overview = if explicit && overview.is_some() {
                overview
            } else {
                Some(overview_path.as_path())
            };
            match Chart::open(&path, id.clone(), selected_overview, cache.clone()) {
                Ok(c) => {
                    let gmp = c.img.parts.keys().any(|k| k.1 == "GMP");
                    let policy = policies.get(&name).cloned().unwrap_or(Policy {
                        status: if gmp { "partial" } else { "supported" }.into(),
                        ..Policy::default()
                    });
                    if explicit || default.is_none() {
                        default = Some(entries.len());
                    }
                    entries.push(Entry {
                        id,
                        name,
                        chart: Some(Arc::new(c)),
                        error: None,
                        policy,
                    });
                }
                Err(e) => {
                    if explicit {
                        return Err(e.context("Default chart failed"));
                    }
                    let policy = policies.get(&name).cloned().unwrap_or_default();
                    entries.push(Entry {
                        id,
                        name,
                        chart: None,
                        error: Some(format!("{e}")),
                        policy,
                    });
                }
            }
        }
        let default = default.context("No readable charts found")?;
        let mut unpaired = Vec::new();
        for (index, e) in entries.iter().enumerate() {
            if let Some(base) = &e.policy.companion_of {
                ensure!(
                    e.policy.role == "overlay",
                    "Only overlays can have a companion"
                );
                if !entries
                    .iter()
                    .any(|b| &b.name == base && b.chart.is_some() && b.policy.role == "base")
                {
                    unpaired.push(index);
                }
            }
        }
        for i in unpaired {
            entries[i].policy.status = "partial".into();
            entries[i].error = Some("Companion base chart is unavailable".into());
        }
        Ok(Self { entries, default })
    }
    pub fn list(&self) -> Value {
        let charts: Vec<_> = self.entries.iter().map(|e| {
            let overview = e.chart.as_ref().is_some_and(|c| c.metadata()["overview_available"] == true);
            let mut features: Vec<&str> = if e.chart.is_none() { vec![] } else if e.policy.role == "overlay" { vec!["2d", "contours"] } else { vec!["2d", "labels", "soundings", "layers"] };
            if overview { features.push("overview"); }
            json!({
                "id": e.id, "name": e.name,
                "title": e.chart.as_ref().map(|c|c.img.title.as_str()).unwrap_or(&e.name),
                "coverage_bbox": e.chart.as_ref().map(|c|c.coverage),
                "status": if e.chart.is_none() { "unreadable" } else { &e.policy.status },
                "role": e.policy.role, "priority": e.policy.priority,
                "companion_of": e.policy.companion_of.as_ref().and_then(|name|self.entries.iter().find(|b|&b.name==name).map(|b|b.id.clone())),
                "error": e.error, "features": features, "overview_available": overview
            })
        }).collect();
        json!({"default_chart": self.entries[self.default].id, "charts": charts})
    }
    pub fn select(&self, requested: Option<&str>, bbox: Option<Bounds>) -> Result<usize> {
        match requested {
            None => Ok(self.default),
            Some("auto") => {
                let Some(bbox) = bbox else {
                    return Ok(self.default);
                };
                let mut eligible: Vec<_> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        e.chart.is_some()
                            && e.policy.role == "base"
                            && e.policy.status == "supported"
                    })
                    .collect();
                eligible.sort_by_key(|(_, e)| (-i64::from(e.policy.priority), e.id.clone()));
                if let Some((i, _)) = eligible
                    .iter()
                    .find(|(_, e)| contains(e.chart.as_ref().unwrap().coverage, bbox))
                {
                    return Ok(*i);
                }
                if eligible.iter().any(|(i, _)| *i == self.default)
                    && intersects(
                        self.entries[self.default].chart.as_ref().unwrap().coverage,
                        bbox,
                    )
                {
                    return Ok(self.default);
                }
                Ok(eligible
                    .into_iter()
                    .max_by(|(_, a), (_, b)| {
                        overlap(a.chart.as_ref().unwrap().coverage, bbox)
                            .total_cmp(&overlap(b.chart.as_ref().unwrap().coverage, bbox))
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(self.default))
            }
            Some(id) => {
                let (i, e) = self
                    .entries
                    .iter()
                    .enumerate()
                    .find(|(_, e)| e.id == id)
                    .context("Unknown chart id")?;
                ensure!(e.chart.is_some(), "Chart is unreadable");
                Ok(i)
            }
        }
    }
    pub fn metadata(&self, requested: Option<&str>) -> Result<Value> {
        let i = self.select(requested, None)?;
        let mut m = self.entries[i].chart.as_ref().unwrap().metadata();
        if requested == Some("auto") {
            let mut b = self.entries[i].chart.as_ref().unwrap().coverage;
            for e in &self.entries {
                if e.policy.role == "base" && e.policy.status == "supported" {
                    if let Some(c) = &e.chart {
                        b = [
                            b[0].min(c.coverage[0]),
                            b[1].min(c.coverage[1]),
                            b[2].max(c.coverage[2]),
                            b[3].max(c.coverage[3]),
                        ];
                    }
                }
            }
            m["coverage_bbox"] = json!(b);
            m["coverage_bounds"] = json!(b);
        }
        m["support_status"] = json!(self.entries[i].policy.status);
        m["role"] = json!(self.entries[i].policy.role);
        Ok(m)
    }
}
fn overlap(a: Bounds, b: Bounds) -> f64 {
    (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0)
}
