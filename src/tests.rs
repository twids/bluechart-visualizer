// SPDX-License-Identifier: GPL-3.0-only
use super::*;
fn put(d: &mut [u8], p: usize, n: usize, v: u32) {
    for i in 0..n {
        d[p + i] = (v >> (i * 8)) as u8;
    }
}
fn fixture() -> Vec<u8> {
    let mut d = vec![0; 14 * 512];
    d[16..23].copy_from_slice(b"DSKIMG\0");
    d[65..72].copy_from_slice(b"GARMIN\0");
    d[0x61] = 9;
    d[0x600] = 1;
    d[0x601..0x60c].fill(b' ');
    put(&mut d, 0x60c, 4, 0xc00);
    for (off, index, block) in [(0x800, 0, 10), (0xa00, 1, 12)] {
        d[off] = 1;
        d[off + 1..off + 12].copy_from_slice(b"MAP00001RGN");
        put(&mut d, off + 12, 4, if index == 0 { 800 } else { 0 });
        d[off + 17] = index;
        d[off + 32..off + 512].fill(255);
        put(&mut d, off + 32, 2, block);
    }
    d[5120..5632].fill(b'A');
    d[6144..6656].fill(b'B');
    d
}
fn sub() -> format::Sub {
    format::Sub {
        index: 0,
        bits: 24,
        shift: 0,
        cx: 1000,
        cy: 2000,
        flags: 0,
        start: 0,
        end: 0,
        bbox: [-180.0, -85.0, 180.0, 85.0],
        ext: [(0, 0); 3],
    }
}
#[test]
fn fragmented_and_continuation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.img");
    let mut d = fixture();
    std::fs::write(&path, &d).unwrap();
    let i = img::Img::open(&path).unwrap();
    let p = &i.parts[&("MAP00001".into(), "RGN".into())];
    assert_eq!(
        i.part(p, 500, 25).unwrap(),
        [vec![b'A'; 12], vec![b'B'; 13]].concat()
    );
    assert!(i.part(p, 799, 2).is_err());
    drop(i);
    d[0xa11] = 2;
    std::fs::write(&path, &d).unwrap();
    assert!(img::Img::open(&path)
        .err()
        .unwrap()
        .to_string()
        .contains("continuation"));
    d[16] = 0;
    std::fs::write(&path, &d).unwrap();
    assert!(img::Img::open(&path).is_err());
}
#[test]
fn embedded_gmp_headers_preserve_absolute_data_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.img");
    let mut d = fixture();
    d[0x809..0x80c].copy_from_slice(b"GMP");
    d[0xa09..0xa0c].copy_from_slice(b"GMP");
    let base = 5120;
    d[base..base + 100].fill(0);
    d[base + 2..base + 12].copy_from_slice(b"GARMIN GMP");
    put(&mut d, base + 0x19, 4, 100);
    d[base + 102..base + 112].copy_from_slice(b"GARMIN TRE");
    std::fs::write(&path, &d).unwrap();
    let i = img::Img::open(&path).unwrap();
    let p = &i.parts[&("MAP00001".into(), "TRE".into())];
    assert_eq!(&i.header(p, 20).unwrap()[2..12], b"GARMIN TRE");
    assert_eq!(
        i.part(p, 500, 25).unwrap(),
        [vec![b'A'; 12], vec![b'B'; 13]].concat()
    );
}
#[test]
fn signed_and_bounded_bits() {
    assert_eq!(sint(&[255, 255, 255], 0, 3).unwrap(), -1);
    assert_eq!(sint(&[0, 0, 128], 0, 3).unwrap(), -(1 << 23));
    assert!(uint(&[0], 0, 3).is_err());
    let data: Vec<u8> = (0..=255).collect();
    let mut b = Bits::new(&data);
    let mut pos = 0;
    while b.remaining() > 0 {
        let n = (1 + pos % 23).min(b.remaining());
        let mut expected = 0;
        for j in 0..n {
            expected |= (((data[(pos + j) / 8] >> ((pos + j) % 8)) & 1) as u32) << j;
        }
        assert_eq!(b.get(n).unwrap(), expected);
        pos += n;
    }
    assert!(b.get(1).is_err());
}
#[test]
fn polygon_and_sounding() {
    let mut d = vec![1, 0, 0, 0];
    d.extend((-2i16).to_le_bytes());
    d.extend(3i16.to_le_bytes());
    d.extend([1, 0, 0x65]);
    let mut p = 0;
    let f = format::decode(&d, &mut p, &sub(), "polygon", false, Default::default()).unwrap();
    assert_eq!(
        f.coords,
        vec![
            [998.0 * UNIT, 2003.0 * UNIT],
            [1000.0 * UNIT, 2004.0 * UNIT]
        ]
    );
    assert_eq!(p, d.len());
    assert!(format::decode(
        &d[..d.len() - 1],
        &mut 0,
        &sub(),
        "polygon",
        false,
        Default::default()
    )
    .is_err());
    let mut d = vec![3, 0x81];
    d.extend((-10i16).to_le_bytes());
    d.extend(20i16.to_le_bytes());
    d.extend([0x88, 84]);
    let f = format::decode(&d, &mut 0, &sub(), "point", true, Default::default()).unwrap();
    assert_eq!(f.typ, 0x10301);
    assert_eq!(f.depth_m, Some(8.4));
    assert_eq!(f.coords[0], [990.0 * UNIT, 2020.0 * UNIT]);
}
#[test]
fn variable_lengths_and_delta_extension() {
    assert_eq!(format::var(&[21], &mut 0).unwrap(), 10);
    assert!(format::var(&[0], &mut 0).is_err());
    let mut d = vec![1, 0, 0, 0];
    d.extend(0i16.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    let values = [(0, 1), (0, 1), (4, 3), (2, 3), (7, 3)];
    let (mut value, mut shift) = (0u32, 0);
    for (v, n) in values {
        value |= v << shift;
        shift += n;
    }
    d.extend([2, 0]);
    d.extend((value as u16).to_le_bytes());
    let f = format::decode(&d, &mut 0, &sub(), "line", false, Default::default()).unwrap();
    assert_eq!(f.coords[1], [1005.0 * UNIT, 1999.0 * UNIT]);
}
#[test]
fn bbox_query_and_gzip_validation() {
    for s in ["nan,59,19,60", "18,60,17,59", "1,2,3", "-181,0,1,1"] {
        assert!(parse_bbox(s).is_err());
    }
    assert!(parse_bbox("18,59,19,60").is_ok());
    assert_eq!(
        parse_query("chart=a%2Fb&generation=2").unwrap()["chart"],
        "a/b"
    );
    assert!(parse_query("chart=%xx").is_err());
    assert!(parse_query("chart=%aé").is_err());
    let mut h = HeaderMap::new();
    h.insert("accept-encoding", "gzip;q=0, br".parse().unwrap());
    assert!(!gzip_allowed(&h));
    h.insert("accept-encoding", "gzip".parse().unwrap());
    assert!(gzip_allowed(&h));
}
#[test]
fn overview_outline() {
    let p = [
        [0., 0.],
        [0.25, 0.],
        [0.5, 0.],
        [1., 0.],
        [1., 1.],
        [0.5, 1.],
        [0., 1.],
    ];
    let simplified = overview::simplify(&p, 0.01);
    assert_eq!(simplified, vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]]);
}
#[test]
fn cache_isolation_and_budget() {
    let mut c = chart::GeometryCache::new();
    let a = Arc::new(chart::Decoded::default());
    let one = ("chart-a:revision-1".into(), "M1".into(), 0);
    let two = ("chart-b:revision-1".into(), "M1".into(), 0);
    c.put(one.clone(), a);
    assert!(c.get(&one).is_some());
    assert!(c.get(&two).is_none());
    for i in 0..1500 {
        let mut d = chart::Decoded::default();
        let mut f = format::decode(
            &[3, 0x81, 0, 0, 0, 0, 0x88, 84],
            &mut 0,
            &sub(),
            "point",
            true,
            Default::default(),
        )
        .unwrap();
        f.coords = vec![[0., 0.]; 4000];
        d.features.push(f);
        c.put(("chart".into(), "M1".into(), i), Arc::new(d));
    }
    assert!(c.bytes <= 64 * 1024 * 1024);
}

// Complete synthetic containers for catalog tests; no real chart data.
fn catalog_chart(path: &std::path::Path, bounds: Bounds) {
    let mut data = fixture();
    for (entry, kind, block) in [(0x800, b"TRE", 10), (0xa00, b"RGN", 12)] {
        data[entry + 9..entry + 12].copy_from_slice(kind);
        put(&mut data, entry + 12, 4, 512);
        data[entry + 17] = 0;
        data[entry + 32..entry + 512].fill(255);
        put(&mut data, entry + 32, 2, block);
    }
    data[5120..5632].fill(0);
    data[6144..6656].fill(0);
    let header = &mut data[5120..5632];
    header[2..12].copy_from_slice(b"GARMIN TRE");
    for (offset, value) in [
        (0x1e, bounds[0]),
        (0x1b, bounds[1]),
        (0x18, bounds[2]),
        (0x15, bounds[3]),
    ] {
        put(header, offset, 3, (value / UNIT) as i32 as u32);
    }
    put(header, 0x21, 4, 0x100);
    put(header, 0x25, 4, 4);
    header[0x101] = 24;
    data[6146..6156].copy_from_slice(b"GARMIN RGN");
    std::fs::write(path, data).unwrap();
}

#[test]
fn catalog_auto_and_bad_extras() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    catalog_chart(&root.join("base.img"), [-10., -10., 10., 10.]);
    catalog_chart(&root.join("detail.img"), [1., 1., 2., 2.]);
    std::fs::write(root.join("broken.img"), b"invalid synthetic fixture").unwrap();
    let config = root.join("policy.json");
    std::fs::write(
        &config,
        r#"{
        "base.img": {"role":"base","status":"supported","priority":0},
        "detail.img": {"role":"base","status":"supported","priority":100}
    }"#,
    )
    .unwrap();
    let primary = root.join("base.img");
    let catalog = catalog::Catalog::discover(
        Some(&primary),
        &[root.to_path_buf()],
        root,
        None,
        Some(&config),
        Arc::new(Mutex::new(chart::GeometryCache::new())),
    )
    .unwrap();
    let detail = catalog
        .entries
        .iter()
        .position(|e| e.name == "detail.img")
        .unwrap();
    assert_eq!(catalog.select(None, None).unwrap(), catalog.default);
    assert_eq!(
        catalog
            .select(Some("auto"), Some([1.2, 1.2, 1.8, 1.8]))
            .unwrap(),
        detail
    );
    assert_eq!(
        catalog
            .select(Some("auto"), Some([-5., -5., -4., -4.]))
            .unwrap(),
        catalog.default
    );
    assert_eq!(
        catalog
            .select(Some("auto"), Some([-6., -6., 6., 6.]))
            .unwrap(),
        catalog.default
    );
    let bad = catalog
        .entries
        .iter()
        .find(|e| e.name == "broken.img")
        .unwrap();
    assert!(bad.chart.is_none());
    assert!(catalog.select(Some(&bad.id), None).is_err());
    assert!(catalog.select(Some("unknown"), None).is_err());
    assert_ne!(
        catalog::id_for("a/chart.img"),
        catalog::id_for("b/chart.img")
    );
}

#[tokio::test]
async fn search_api_scope_and_unavailable_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in ["base.img", "detail.img", "partial.img", "overlay.img"] {
        catalog_chart(&root.join(name), [17., 58., 19., 60.]);
    }
    let policy = root.join("policy.json");
    std::fs::write(
        &policy,
        r#"{
        "base.img":{"role":"base","status":"supported"},
        "detail.img":{"role":"base","status":"supported"},
        "partial.img":{"role":"base","status":"partial"},
        "overlay.img":{"role":"overlay","status":"supported","companion_of":"base.img"}
    }"#,
    )
    .unwrap();
    let catalog = Catalog::discover(
        Some(&root.join("base.img")),
        &[root.to_path_buf()],
        root,
        None,
        Some(&policy),
        Arc::new(Mutex::new(GeometryCache::new())),
    )
    .unwrap();
    let base = &catalog.entries[catalog.default];
    let base_id = base.id.clone();
    search::build(base.chart.as_ref().unwrap(), &search::path(root, &base_id)).unwrap();
    let db = rusqlite::Connection::open(search::path(root, &base_id)).unwrap();
    db.execute("INSERT INTO places VALUES ('Östergård','ostergard','point',66816,18,59,'[18,59,18,59]',1800000,5900000)", []).unwrap();
    drop(db);
    assert_eq!(search::scope(&catalog, Some("auto")).unwrap().len(), 2);
    let partial = catalog
        .entries
        .iter()
        .find(|e| e.name == "partial.img")
        .unwrap();
    assert_eq!(search::scope(&catalog, Some(&partial.id)).unwrap().len(), 1);
    let overlay_id = catalog
        .entries
        .iter()
        .find(|e| e.name == "overlay.img")
        .unwrap()
        .id
        .clone();
    let app = App {
        catalog: Arc::new(catalog),
        search_root: root.into(),
        search_building: Arc::new(AtomicBool::new(false)),
        overview_building: Arc::new(AtomicBool::new(false)),
        search_slots: Arc::new(tokio::sync::Semaphore::new(2)),
        hosts: Arc::new(vec!["localhost".into()]),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
        latest: Arc::new(Mutex::new(lru::LruCache::new(
            NonZeroUsize::new(2).unwrap(),
        ))),
    };
    async fn request(app: &App, raw: &str, host: &str, method: Method) -> (StatusCode, Value) {
        let mut headers = HeaderMap::new();
        headers.insert("host", host.parse().unwrap());
        let response = handle(
            State(app.clone()),
            method,
            "/api/search".parse().unwrap(),
            headers,
            RawQuery(Some(raw.into())),
        )
        .await;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 100000)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    let (status, data) = request(&app, "chart=auto&q=harbour", "localhost", Method::GET).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["warnings"].as_array().unwrap().len(), 1);
    assert!(data["warnings"][0]
        .as_str()
        .unwrap()
        .contains("automatically at startup"));
    let (_, data) = request(
        &app,
        &format!("chart={base_id}&q=harbour"),
        "localhost",
        Method::GET,
    )
    .await;
    assert!(data["warnings"].as_array().unwrap().is_empty());
    let (status, data) = request(&app, "chart=auto&q=OSTERGARD", "localhost", Method::GET).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["results"].as_array().unwrap().len(), 1);
    assert_eq!(data["results"][0]["chart_id"], base_id);
    assert_eq!(data["results"][0]["label"], "Östergård");
    assert_eq!(data["results"][0]["coordinates"], json!([18., 59.]));
    for raw in [
        "q=x".into(),
        "q=%GG".into(),
        "q=%FF".into(),
        "chart=unknown&q=harbour".into(),
        format!("chart={overlay_id}&q=harbour"),
        "q=".into(),
        "".into(),
    ] {
        assert_eq!(
            request(&app, &raw, "localhost", Method::GET).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(&app, "q=harbour", "evil.example", Method::GET)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "q=harbour", "localhost", Method::POST)
            .await
            .0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    let db = rusqlite::Connection::open(search::path(root, &base_id)).unwrap();
    db.execute("UPDATE metadata SET revision='stale'", [])
        .unwrap();
    drop(db);
    let (_, data) = request(&app, "q=ostergard", "localhost", Method::GET).await;
    assert!(data["results"].as_array().unwrap().is_empty());
    assert_eq!(data["warnings"].as_array().unwrap().len(), 1);
    let _permits = app.search_slots.acquire_many(2).await.unwrap();
    assert_eq!(
        request(&app, "q=ostergard", "localhost", Method::GET)
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[test]
fn search_builder_preserves_published_index_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let chart_file = dir.path().join("base.img");
    catalog_chart(&chart_file, [17., 58., 19., 60.]);
    let c = Chart::open(
        &chart_file,
        "test".into(),
        None,
        Arc::new(Mutex::new(GeometryCache::new())),
    )
    .unwrap();
    let output = search::path(dir.path(), &c.id);
    search::build(&c, &output).unwrap();
    let original = std::fs::read(&output).unwrap();
    let temp = output.with_extension("sqlite3.tmp");
    std::fs::write(&temp, b"reserved").unwrap();
    assert!(search::build(&c, &output).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), original);
    std::fs::remove_file(&temp).unwrap();
    // Force an out-of-range TRE subdivision read in a fresh Chart (no cached index).
    let mut c = Chart::open(
        &chart_file,
        "test".into(),
        None,
        Arc::new(Mutex::new(GeometryCache::new())),
    )
    .unwrap();
    c.tiles[0].h[0x2d..0x31].fill(255);
    assert!(search::build(&c, &output).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), original);
    assert!(!temp.exists());
}
#[test]
#[ignore = "requires the private Norden sample"]
fn norden_subdivision_regression() {
    let chart = Chart::open(
        &PathBuf::from(
            std::env::var_os("BLUECHART_NORDEN_SAMPLE").expect("BLUECHART_NORDEN_SAMPLE"),
        ),
        "norden".into(),
        None,
        Arc::new(Mutex::new(GeometryCache::new())),
    )
    .unwrap();
    let tile = chart.tiles.iter().find(|t| t.id == "M0000008").unwrap();
    let subs = chart.subs(tile).unwrap();
    let decoded = chart
        .decode_sub(tile, &subs[3139], &mut img::ReadCache::default(), &|| false)
        .unwrap();
    assert!(decoded.warnings.is_empty(), "{:?}", decoded.warnings);
    assert!(!decoded.features.is_empty());
}

#[test]
fn startup_search_indexes_build_reuse_repair_and_continue_after_failure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in ["a.img", "b.img", "overlay.img"] {
        catalog_chart(&root.join(name), [17., 58., 19., 60.]);
    }
    let config = root.join("policy.json");
    std::fs::write(&config, r#"{"overlay.img":{"role":"overlay"}}"#).unwrap();
    let catalog = Catalog::discover(
        None,
        &[root.to_path_buf()],
        root,
        None,
        Some(&config),
        Arc::new(Mutex::new(GeometryCache::new())),
    )
    .unwrap();
    let a = search::path(root, &catalog::id_for("a.img"));
    let b = search::path(root, &catalog::id_for("b.img"));
    search::ensure_indexes(&catalog, root, &|| true);
    assert!(
        !a.exists() && !b.exists(),
        "cancelled startup must not build indexes"
    );
    let lock = a.with_extension("sqlite3.tmp");
    std::fs::write(&lock, b"another builder").unwrap();
    search::ensure_indexes(&catalog, root, &|| false);
    assert!(!a.exists());
    assert!(b.exists(), "failure on one chart must not block the next");
    assert!(!search::path(root, &catalog::id_for("overlay.img")).exists());
    assert_eq!(std::fs::read(&lock).unwrap(), b"another builder");
    // A newly supplied chart is picked up on the next startup discovery.
    catalog_chart(&root.join("new.img"), [17., 58., 19., 60.]);
    let refreshed = Catalog::discover(
        None,
        &[root.to_path_buf()],
        root,
        None,
        Some(&config),
        Arc::new(Mutex::new(GeometryCache::new())),
    )
    .unwrap();
    search::ensure_indexes(&refreshed, root, &|| false);
    assert!(search::path(root, &catalog::id_for("new.img")).exists());
    std::fs::remove_file(&lock).unwrap();
    search::ensure_indexes(&catalog, root, &|| false);
    assert!(a.exists());
    let db = rusqlite::Connection::open(&a).unwrap();
    db.execute_batch("CREATE TABLE reuse_marker(value TEXT);")
        .unwrap();
    drop(db);
    let before = std::fs::read(&a).unwrap();
    let db = rusqlite::Connection::open(&b).unwrap();
    db.execute("UPDATE metadata SET revision='stale'", [])
        .unwrap();
    drop(db);
    search::ensure_indexes(&catalog, root, &|| false);
    assert_eq!(
        std::fs::read(&a).unwrap(),
        before,
        "valid indexes must be reused"
    );
    let db = rusqlite::Connection::open(&b).unwrap();
    let revision: String = db
        .query_row("SELECT revision FROM metadata", [], |r| r.get(0))
        .unwrap();
    assert_ne!(revision, "stale");
    drop(db);
    std::fs::write(&a, b"invalid SQLite").unwrap();
    std::fs::remove_file(&b).unwrap();
    search::ensure_indexes(&catalog, root, &|| false);
    for path in [&a, &b] {
        let db = rusqlite::Connection::open(path).unwrap();
        let integrity: String = db
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
    }
}

#[test]
fn trailing_delta_extensions_do_not_create_partial_coordinates() {
    // The final byte can start an escape without containing another complete pair.
    // Match the reference decoder's end-of-stream handling for this tail.
    for tail in [[(8u16, 4u16), (0, 3)], [(0, 4), (4, 3)]] {
        let mut value = 0u16;
        let mut shift = 0;
        for (v, n) in [(0, 1), (0, 1), (1, 4), (2, 3)].into_iter().chain(tail) {
            value |= v << shift;
            shift += n;
        }
        let mut data = vec![1, 0, 0, 0, 0, 0, 0, 0, 2, 1];
        data.extend(value.to_le_bytes());
        let mut p = 0;
        let feature =
            format::decode(&data, &mut p, &sub(), "polygon", false, Default::default()).unwrap();
        assert_eq!(p, data.len());
        assert_eq!(
            feature.coords,
            vec![[1000. * UNIT, 2000. * UNIT], [1001. * UNIT, 2002. * UNIT]]
        );
        assert!(
            format::decode(
                &data[..data.len() - 1],
                &mut 0,
                &sub(),
                "polygon",
                false,
                Default::default()
            )
            .is_err(),
            "declared byte lengths remain strict"
        );
    }
}
