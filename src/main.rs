// SPDX-License-Identifier: GPL-3.0-only
mod catalog;
mod chart;
mod format;
mod img;
mod overview;
mod search;
#[cfg(test)]
mod tests;
use crate::{
    catalog::Catalog,
    chart::{Chart, GeometryCache},
    format::*,
};
use anyhow::{ensure, Context, Result};
use axum::{
    body::Body,
    extract::{RawQuery, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::Response,
    Router,
};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    num::NonZeroUsize,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
#[derive(Parser)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long, global = true)]
    chart: Option<PathBuf>,
    #[arg(long, global = true)]
    charts_dir: Vec<PathBuf>,
    #[arg(long, default_value = "cache", global = true)]
    cache_dir: PathBuf,
    #[arg(long, global = true)]
    overview: Option<PathBuf>,
    #[arg(long, global = true)]
    chart_config: Option<PathBuf>,
    #[arg(long, default_value = "127.0.0.1", global = true)]
    bind: String,
    #[arg(long, default_value_t = 8765, global = true)]
    port: u16,
    #[arg(long, global = true)]
    allowed_host: Vec<String>,
    #[arg(long, global = true)]
    bbox: Option<String>,
    #[arg(long, default_value_t = 22, global = true)]
    bits: u8,
    #[arg(long, global = true)]
    dump_features: bool,
    #[arg(long, global = true)]
    output: Option<PathBuf>,
}
#[derive(Subcommand)]
enum Command {
    Serve,
    Inspect,
    BuildOverview,
    BuildSearch,
    Healthcheck,
}
#[derive(Clone)]
struct App {
    catalog: Arc<Catalog>,
    search_root: PathBuf,
    search_building: Arc<AtomicBool>,
    overview_building: Arc<AtomicBool>,
    search_slots: Arc<tokio::sync::Semaphore>,
    hosts: Arc<Vec<String>>,
    slots: Arc<tokio::sync::Semaphore>,
    latest: Arc<Mutex<lru::LruCache<String, Arc<AtomicU64>>>>,
}
#[derive(Deserialize, Default)]
struct Query {
    chart: Option<String>,
    q: Option<String>,
    bbox: Option<String>,
    bits: Option<String>,
    fishing: Option<String>,
    compact: Option<String>,
    client: Option<String>,
    generation: Option<String>,
    overlays: Option<String>,
}
fn parse_bbox(s: &str) -> Result<Bounds> {
    let v = s
        .split(',')
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        v.len() == 4
            && v.iter().all(|v| v.is_finite())
            && v[0] >= -180.0
            && v[0] < v[2]
            && v[2] <= 180.0
            && v[1] >= -85.0
            && v[1] < v[3]
            && v[3] <= 85.0,
        "Invalid bbox"
    );
    Ok([v[0], v[1], v[2], v[3]])
}
fn flag(v: Option<&str>, default: bool) -> Result<bool> {
    match v {
        None => Ok(default),
        Some("1") => Ok(true),
        Some("0") => Ok(false),
        _ => anyhow::bail!("Invalid boolean parameter"),
    }
}
fn gzip_allowed(headers: &HeaderMap) -> bool {
    headers
        .get("accept-encoding")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',').any(|s| {
                let mut parts = s.trim().split(';');
                let name = parts.next().unwrap_or("").trim();
                let q = parts
                    .find_map(|p| {
                        p.trim()
                            .strip_prefix("q=")
                            .and_then(|v| v.parse::<f32>().ok())
                    })
                    .unwrap_or(1.0);
                name == "gzip" && q > 0.0
            })
        })
}
fn response(status: StatusCode, mut bytes: Vec<u8>, content: &str, gzip: bool) -> Response {
    let compressed = gzip && bytes.len() > 4096;
    if compressed {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        if encoder.write_all(&bytes).is_ok() {
            if let Ok(b) = encoder.finish() {
                bytes = b;
            }
        }
    }
    let len = bytes.len();
    let mut r=Response::builder().status(status).header("Content-Type",content).header("Content-Length",len).header("Cache-Control","no-store").header("Vary","Accept-Encoding").header("X-Content-Type-Options","nosniff").header("Content-Security-Policy","default-src 'self'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'");
    if compressed {
        r = r.header("Content-Encoding", "gzip");
    }
    r.body(Body::from(bytes)).unwrap()
}
fn error(status: StatusCode, msg: &str) -> Response {
    response(
        status,
        json!({"error":msg}).to_string().into_bytes(),
        "application/json; charset=utf-8",
        false,
    )
}
async fn handle(
    State(app): State<App>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let host = headers
        .get("host")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");
    if !app.hosts.iter().any(|s| s == host) {
        return error(StatusCode::FORBIDDEN, "Host not allowed");
    }
    if method != Method::GET {
        return error(StatusCode::METHOD_NOT_ALLOWED, "Only GET is supported");
    }
    let gzip = gzip_allowed(&headers);
    match uri.path() {
        "/" => {
            return response(
                StatusCode::OK,
                include_bytes!("../web/index.html").to_vec(),
                "text/html; charset=utf-8",
                gzip,
            )
        }
        "/app.js" => {
            return response(
                StatusCode::OK,
                include_bytes!("../web/app.js").to_vec(),
                "text/javascript; charset=utf-8",
                gzip,
            )
        }
        "/features.js" => {
            return response(
                StatusCode::OK,
                include_bytes!("../web/features.js").to_vec(),
                "text/javascript; charset=utf-8",
                gzip,
            )
        }
        "/view.js" => {
            return response(
                StatusCode::OK,
                include_bytes!("../web/view.js").to_vec(),
                "text/javascript; charset=utf-8",
                gzip,
            )
        }
        "/style.css" => {
            return response(
                StatusCode::OK,
                include_bytes!("../web/style.css").to_vec(),
                "text/css; charset=utf-8",
                gzip,
            )
        }
        "/api/build-status" => {
            return response(StatusCode::OK, json!({"search": app.search_building.load(Ordering::Relaxed), "overview": app.overview_building.load(Ordering::Relaxed)}).to_string().into_bytes(), "application/json; charset=utf-8", false)
        }
        "/api/charts" => {
            return response(
                StatusCode::OK,
                app.catalog.list().to_string().into_bytes(),
                "application/json; charset=utf-8",
                gzip,
            )
        }
        "/api/metadata" | "/api/features" | "/api/search" => {}
        _ => return error(StatusCode::NOT_FOUND, "Not found"),
    }
    // Decode query values without accepting arbitrary paths or URLs.
    let query = parse_query(raw.as_deref().unwrap_or(""));
    let q: Query = match query.and_then(|v| Ok(serde_json::from_value(v)?)) {
        Ok(q) => q,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid query"),
    };
    if uri.path() == "/api/search" {
        let validated = search::validate(q.q.as_deref())
            .and_then(|text| Ok((text, search::scope(&app.catalog, q.chart.as_deref())?)));
        let (text, indices) = match validated {
            Ok(v) => v,
            Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
        };
        let permit = match app.search_slots.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "Search busy; try again"),
        };
        return match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            search::search(&app.catalog, &app.search_root, &indices, &text)
        })
        .await
        {
            Ok(payload) => response(
                StatusCode::OK,
                payload.to_string().into_bytes(),
                "application/json; charset=utf-8",
                gzip,
            ),
            Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "Search worker failed"),
        };
    }
    if uri.path() == "/api/metadata" {
        return match app.catalog.metadata(q.chart.as_deref()) {
            Ok(m) => response(
                StatusCode::OK,
                m.to_string().into_bytes(),
                "application/json; charset=utf-8",
                gzip,
            ),
            Err(e) => error(StatusCode::BAD_REQUEST, &e.to_string()),
        };
    }
    let validated = (|| -> Result<_> {
        let bbox = parse_bbox(q.bbox.as_deref().context("Missing bbox")?)?;
        let bits = q.bits.as_deref().unwrap_or("24").parse::<u8>()?;
        ensure!((11..=24).contains(&bits), "Invalid detail level");
        let fishing = flag(q.fishing.as_deref(), true)?;
        let compact = flag(q.compact.as_deref(), false)?;
        let client = q.client.clone().unwrap_or_default();
        ensure!(client.len() <= 64, "Invalid client id");
        let generation = q.generation.as_deref().unwrap_or("0").parse::<u64>()?;
        ensure!(generation < u64::MAX, "Invalid generation");
        let index = app.catalog.select(q.chart.as_deref(), Some(bbox))?;
        let mut overlays = Vec::new();
        if let Some(ids) = &q.overlays {
            for id in ids.split(',').filter(|s| !s.is_empty()) {
                let i = app.catalog.select(Some(id), None)?;
                let e = &app.catalog.entries[i];
                ensure!(
                    e.policy.role == "overlay"
                        && e.policy.companion_of.as_deref()
                            == Some(app.catalog.entries[index].name.as_str()),
                    "Overlay does not belong to the selected base chart"
                );
                if !overlays.contains(&i) {
                    overlays.push(i);
                }
            }
        }
        Ok((
            bbox, bits, fishing, compact, client, generation, index, overlays,
        ))
    })();
    let (bbox, bits, fishing, compact, client, generation, index, overlays) = match validated {
        Ok(v) => v,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    let token = if client.is_empty() {
        Arc::new(AtomicU64::new(generation))
    } else {
        let mut latest = app.latest.lock().unwrap();
        if let Some(t) = latest.get(&client) {
            t.fetch_max(generation, Ordering::SeqCst);
            t.clone()
        } else {
            let t = Arc::new(AtomicU64::new(generation));
            if let Some((_, old)) = latest.push(client, t.clone()) {
                old.store(u64::MAX, Ordering::SeqCst);
            }
            t
        }
    };
    let cancelled = || token.load(Ordering::SeqCst) != generation;
    // A stale queued request must never start a decoder.
    let acquire = app.slots.clone().acquire_owned();
    tokio::pin!(acquire);
    let permit = loop {
        if cancelled() {
            return error(StatusCode::CONFLICT, "Superseded map request");
        }
        tokio::select! {p=&mut acquire=>match p{Ok(p)=>break p,Err(_)=>return error(StatusCode::SERVICE_UNAVAILABLE,"Server shutting down")},_=tokio::time::sleep(std::time::Duration::from_millis(25))=>{}}
    };
    let catalog = app.catalog.clone();
    let task = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let _permit = permit;
        let cancel = || token.load(Ordering::SeqCst) != generation;
        ensure!(!cancel(), "Superseded map request");
        let started = std::time::Instant::now();
        let chart = catalog.entries[index].chart.as_ref().unwrap();
        let mut payload = if intersects(bbox, chart.coverage) {
            let c = chart.coverage;
            chart.features([bbox[0].max(c[0]), bbox[1].max(c[1]), bbox[2].min(c[2]), bbox[3].min(c[3])], bits, fishing, &cancel)?
        } else {
            json!({"features": [], "warnings": ["Outside chart coverage."], "stats": {}, "chart_id": chart.id, "chart_title": chart.img.title})
        };
        for i in overlays {
            // Companion contours are useful at detail scales, not in an overview.
            if payload["stats"]["effective_bits"].as_u64().is_some_and(|b| b <= 17) { break; }
            ensure!(!cancel(), "Superseded map request");
            let overlay = catalog.entries[i].chart.as_ref().unwrap();
            if !intersects(bbox, overlay.coverage) { continue; }
            let remaining = 180usize.saturating_sub(payload["stats"]["subdivisions"].as_u64().unwrap_or(0) as usize);
            if remaining == 0 {
                payload["warnings"].as_array_mut().unwrap().push(json!("Supplementary contours omitted: subdivision limit reached."));
                break;
            }
            let other = match overlay.features_limited(bbox, bits, fishing, &cancel, remaining) {
                Ok(p) => p,
                Err(e) => {
                    ensure!(!cancel(), "Superseded map request");
                    payload["warnings"].as_array_mut().unwrap().push(json!(format!("Supplementary contours: {e}")));
                    continue;
                }
            };
            payload["features"].as_array_mut().unwrap().extend(other["features"].as_array().unwrap().clone());
            payload["warnings"].as_array_mut().unwrap().extend(other["warnings"].as_array().unwrap().clone());
            for field in ["subdivisions", "bytes_read"] {
                let sum = payload["stats"][field].as_u64().unwrap_or(0) + other["stats"][field].as_u64().unwrap_or(0);
                payload["stats"][field] = json!(sum);
            }
            payload["stats"]["features"] = json!(payload["features"].as_array().unwrap().len());
        }
        if compact {
            for feature in payload["features"].as_array_mut().unwrap() {
                let f = feature.as_object_mut().unwrap();
                f.retain(|k, _| ["kind", "type", "label", "tile", "depth_m", "symbol_detail", "coords"].contains(&k.as_str()));
                if let Some(points) = f.get_mut("coords").and_then(Value::as_array_mut) {
                    for p in points { for v in p.as_array_mut().unwrap() { *v = json!((v.as_f64().unwrap() * 1e6).round() / 1e6); } }
                }
            }
        }
        ensure!(!cancel(), "Superseded map request");
        if payload["stats"]["features"].is_number() { payload["stats"]["seconds"] = json!(started.elapsed().as_secs_f64()); }
        Ok(serde_json::to_vec(&payload)?)
    }).await;
    match task {
        Ok(Ok(bytes)) => response(
            StatusCode::OK,
            bytes,
            "application/json; charset=utf-8",
            gzip,
        ),
        Ok(Err(e)) => {
            let msg = e.to_string();
            error(
                if msg == "Superseded map request" {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::BAD_REQUEST
                },
                &msg,
            )
        }
        Err(e) => {
            eprintln!("Worker error: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "Map worker failed")
        }
    }
}
fn parse_query(raw: &str) -> Result<Value> {
    let mut map = serde_json::Map::new();
    for pair in raw.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let decode = |s: &str| -> Result<String> {
            let bytes = s.as_bytes();
            let mut out = Vec::new();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' {
                    ensure!(i + 2 < bytes.len(), "Invalid percent encoding");
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3])?;
                    out.push(u8::from_str_radix(hex, 16)?);
                    i += 3;
                } else {
                    out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
                    i += 1;
                }
            }
            Ok(String::from_utf8(out)?)
        };
        let key = decode(k)?;
        map.entry(key).or_insert(Value::String(decode(v)?));
    }
    Ok(Value::Object(map))
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();
    if matches!(args.command, Some(Command::Healthcheck)) {
        let mut stream = std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", args.port).parse()?,
            std::time::Duration::from_secs(5),
        )?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        write!(
            stream,
            "GET /api/metadata HTTP/1.0\r\nHost: 127.0.0.1:{}\r\n\r\n",
            args.port
        )?;
        let mut result = String::new();
        stream.read_to_string(&mut result)?;
        ensure!(
            result.starts_with("HTTP/1.0 200") || result.starts_with("HTTP/1.1 200"),
            "Healthcheck failed"
        );
        return Ok(());
    }
    let cache = Arc::new(Mutex::new(GeometryCache::new()));
    if matches!(args.command, Some(Command::BuildOverview)) {
        let path = args.chart.context("--chart is required")?;
        let c = Chart::open(&path, "builder".into(), None, cache)?;
        return overview::build(&c, &args.output.context("--output is required")?);
    }
    let fallback = PathBuf::from("charts/chart.img");
    let single = args.chart.as_deref().or_else(|| {
        if args.charts_dir.is_empty() {
            Some(fallback.as_path())
        } else {
            None
        }
    });
    let catalog = Catalog::discover(
        single,
        &args.charts_dir,
        &args.cache_dir,
        args.overview.as_deref(),
        args.chart_config.as_deref(),
        cache,
    )?;
    if matches!(args.command, Some(Command::BuildSearch)) {
        for entry in &catalog.entries {
            if entry.policy.role == "base" {
                if let Some(chart) = &entry.chart {
                    search::build(chart, &search::path(&args.cache_dir, &entry.id))?;
                }
            }
        }
        return Ok(());
    }
    if matches!(args.command, Some(Command::Inspect)) {
        if let Some(b) = args.bbox {
            let c = catalog.entries[catalog.default].chart.as_ref().unwrap();
            let p = c.features(parse_bbox(&b)?, args.bits, true, &|| false)?;
            if args.dump_features {
                println!("{}", p);
            } else {
                let mut types = HashMap::<String, usize>::new();
                let features = p["features"].as_array().unwrap();
                for f in features {
                    *types
                        .entry(format!(
                            "{}:0x{:x}",
                            f["kind"].as_str().unwrap(),
                            f["type"].as_u64().unwrap()
                        ))
                        .or_default() += 1;
                }
                println!(
                    "{}",
                    json!({"metadata":c.metadata(),"stats":p["stats"],"warnings":p["warnings"],"types":types,"soundings":features.iter().filter(|f|f.get("depth_m").is_some()).count()})
                );
            }
        } else {
            println!("{}", catalog.list());
        }
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind((args.bind.as_str(), args.port)).await?;
    let port = listener.local_addr()?.port();
    let mut hosts = args.allowed_host;
    hosts.extend([format!("127.0.0.1:{port}"), format!("localhost:{port}")]);
    let app = App {
        catalog: Arc::new(catalog),
        search_root: args.cache_dir,
        search_building: Arc::new(AtomicBool::new(false)),
        overview_building: Arc::new(AtomicBool::new(false)),
        search_slots: Arc::new(tokio::sync::Semaphore::new(2)),
        hosts: Arc::new(hosts),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
        latest: Arc::new(Mutex::new(lru::LruCache::new(
            NonZeroUsize::new(256).unwrap(),
        ))),
    };
    // Keep HTTP/health checks and available indexes responsive during a large scan.
    let search_catalog = app.catalog.clone();
    let search_root = app.search_root.clone();
    let search_cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = search_cancel.clone();
    let search_building = app.search_building.clone();
    tokio::task::spawn_blocking(move || {
        search::ensure_indexes_with_status(
            &search_catalog,
            &search_root,
            &|| worker_cancel.load(Ordering::Relaxed),
            &|active| search_building.store(active, Ordering::Relaxed),
        )
    });
    let overview_catalog = app.catalog.clone();
    let overview_cancel = search_cancel.clone();
    let overview_building = app.overview_building.clone();
    tokio::task::spawn_blocking(move || {
        overview::ensure_overviews(
            &overview_catalog,
            &|| overview_cancel.load(Ordering::Relaxed),
            &|active| overview_building.store(active, Ordering::Relaxed),
        )
    });
    eprintln!("BlueChart Rust listening on {}:{port}", args.bind);
    axum::serve(listener, Router::new().fallback(handle).with_state(app))
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("Install SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = term.recv() => {},
                }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            search_cancel.store(true, Ordering::Relaxed);
        })
        .await?;
    Ok(())
}
