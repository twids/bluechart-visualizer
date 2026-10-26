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
