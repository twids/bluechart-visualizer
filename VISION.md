# Perspective and Vision format investigation

The viewer has a perspective camera over the existing chart plane. Its **3D view**
button toggles perspective and exposes a keyboard-accessible tilt control (0–60°).
North stays up. Soundings, labels, feature inspection, place search, layer toggles,
dragging and cursor-centred zoom share the camera. Viewport requests include all
four ground corners. Geometry is clipped before perspective division; the scale
bar describes horizontal distance near the bottom of the view. Fit/reset honours
the selected tilt. The mode is retained when changing charts or choosing places.

This version does not generate terrain, a seabed surface, inferred depths or Garmin
MarinerEye/FishEye rendering. Chart depth-area colours remain unverified.

## Read-only inspection

`tools/inspect_vision.py` uses only the Python standard library:

```sh
python tools/inspect_vision.py charts/chart.img --scan-jpeg > cache/vision-inventory.json
```

The input is opened read-only. Output includes product titles, subfile identifiers,
section offsets and sizes, and aggregate structural checks. It is private chart
metadata: keep reports in an ignored cache directory, outside distributed images
and Git. No extracted images or coordinates are exported. This is a developer
tool, not a server endpoint or a chart-content capability declaration.

The inventory follows IMG allocation chains, including continuation records and
fragmentation. It validates embedded GMP headers and reports TRE/RGN/LBL/NET/NOD/
DEM/MAR/MET presence. LBL image index/storage lengths follow GPXSee's raster
reader. The optional physical JPEG scan validates marker structure through a
supported start-of-frame header, rather than counting arbitrary byte matches.
An empty result cannot rule out fragmented or differently encoded imagery.

An observed 92-byte MAR directory has seven referenced sections. The probe checks
their ranges, adjacency and record alignment. A separate hypothesis interprets
12-byte records in section 3 as an offset into section 2 plus two signed map
coordinates at 28-bit precision. It checks these against the associated TRE
coverage and the section-2 bounds/alignment, reporting counts rather than positions.
These structural checks do **not** establish depth values, bottom triangles,
image references or routing semantics. Other layouts require separate analysis.

MET section descriptors and product metadata can be inventoried independently of
MAR. Product branding alone does not verify which premium payloads are present.
Neither marine format is exposed as new rendering data in this version.

Further work should establish section semantics and units against independently
decoded chart features before adding bottom geometry. Image support needs an
actual decodable image together with its geographic or POI association.

## References

- [Garmin IMG subfile format notes, 2019-11-16](https://ge0mlib.com/papers/File_Formats/rvrs/Garmin_IMG_Subfiles.pdf): GMP embedded header pointers and MET descriptors. Reverse-engineered reference, not a Garmin specification. Newer or undocumented fields remain unclassified.
- [GPXSee LBL reader](https://github.com/tumic0/GPXSee/blob/master/src/map/IMG/lblfile.cpp): raster index/storage descriptors and image references.
- [GPXSee RGN reader](https://github.com/tumic0/GPXSee/blob/master/src/map/IMG/rgnfile.cpp): raster image association with geographic bounds.

## Validation

```sh
node tests/view.cjs
node tests/features.cjs
node tests/features-browser.cjs
node tests/search-browser.cjs
python tests/test_vision.py
cargo fmt --check
cargo test --release --locked
```

Tests use synthetic data. Browser checks require Playwright and Edge, as documented
in README.md. Camera checks cover inversion, clipping through the camera plane,
wide and narrow viewports and separated line segments. Browser checks exercise
tilted feature inspection, fit, layer visibility, cursor zoom and anchored dragging.
