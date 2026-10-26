# Chart Viewer

An experimental browser viewer for compatible, user-supplied IMG marine charts.
The backend, chart decoder and overview builder are a standalone Rust application.
The browser provides panning, zooming, chart selection, depth soundings, labels,
simplified symbols, layers and an automatically selected chart source.

No chart files, extracted chart data, caches or chart screenshots are included.
Supply your own charts. The software license grants no rights to chart content.

## Run with Docker Compose

Docker Compose is the recommended deployment method. Put the default chart at
`charts/chart.img` and additional IMG files anywhere below `charts/`.
Copy `.env.example` to `.env` to configure the chart directory and HTTP Host.

```sh
mkdir -p charts cache
docker compose build
docker compose --profile tools run --rm build-overview
docker compose up -d viewer
```

Open http://localhost:8765. See [DEPLOYMENT.md](DEPLOYMENT.md) for configuration,
per-chart overview caches, reverse proxy setup, updates and rollback.
Charts and caches are mounted read-only in the viewer and excluded from images.

## Run with Rust

Install the stable Rust toolchain and a C compiler (for bundled SQLite), then:

```sh
cargo build --release --locked
target/release/bluechart build-overview --chart charts/chart.img --output cache/overview.sqlite3
target/release/bluechart serve --chart charts/chart.img --charts-dir charts --cache-dir cache --overview cache/overview.sqlite3 --chart-config chart-config.example.json
```

The default listener is http://127.0.0.1:8765. On Windows use
`Start-Viewer.ps1 -ChartPath C:/charts/chart.img -ChartsDir C:/charts`.
The script builds the Rust executable if necessary. Python is not required in production.

## Charts and controls

The selection menu lists charts discovered recursively when the server starts.
The default chart remains selected initially. **All / Auto** selects one supported
base chart for the entire viewport, using configured priorities and coverage.
It falls back to the default chart, then the largest overlapping supported base.
It does not merge overlapping base charts. The active source is shown below the menu.
Restart the server after adding or replacing files; there is no upload endpoint.

Copy `chart-config.example.json` and edit entries keyed by relative chart path.
Each entry specifies `role` (`base` or `overlay`), `status` (`supported` or `partial`)
and integer `priority` (higher first). An overlay uses `companion_of` to name its
base chart's relative path. Only supported base charts participate in Auto.
Unknown GMP charts start as partial until inspected and verified. Broken extra
files remain listed as unavailable and do not prevent readable charts starting.
The explicit default chart must be readable.

The sample policy uses fictional filenames. Inspect and verify each supplied
file before changing its status from partial to supported. A supplementary file
can be configured as a layer on its base chart rather than a separate base.
No charts, product-specific examples or private deployment configuration are bundled.

Drag to pan, scroll/double-click or use the buttons to zoom. Arrow keys pan;
plus/minus zoom; Home restores the initial view. The coverage button fits the
selected chart. Hover over points for available labels, depth and coordinates.
Chart changes clear old objects and cancel superseded requests. Supplementary
contours are detail-only and share the 180-subdivision request budget with the base.

## How it works

The decoder supports IMG/FAT continuation records and fragmented allocation,
separate TRE/RGN/LBL subfiles and embedded GMP headers, packed coordinates,
marine depth attributes and label encodings. Reads use file positions without
a shared seek cursor. Tile indexes are loaded on demand and retain four entries
per chart. The geometry LRU has a combined estimated 64 MiB budget across charts;
labels are bounded per request. Active requests and response buffers use additional memory.

At most two map jobs run concurrently. Cache locks cover lookups and insertion,
not entire decodes. Superseded jobs are checked while queued and during decoding.
Broad views use spatially indexed SQLite overview caches with three land-outline levels.
An invalid cache revision is rejected. Missing overviews produce a visible warning.

## Read API

- `GET /api/charts`: stable chart IDs, relative names, titles, coverage, support
  status, capabilities, companion relationships and default ID. No absolute paths.
- `GET /api/metadata?chart=ID`: chart metadata, initial view, coverage, overview
  availability and limitations. `chart=auto` returns combined supported coverage.
- `GET /api/features?chart=ID&bbox=WEST,SOUTH,EAST,NORTH&bits=23`: decoded objects,
  warnings, active chart ID/title and statistics. Coordinates use WGS 84.

Omitting `chart` selects the configured default. `chart=auto` selects a base for
the requested viewport. `overlays=ID,ID` adds only configured companions of the
selected base. `fishing=0` skips fishing tiles, `compact=1` removes internal fields
and rounds wire coordinates to six decimals. Responses support gzip.
Client/generation identifiers cancel superseded views (HTTP 409).
The server validates bbox, resolution and Host, serves only explicit web assets
and read APIs, and provides no raw IMG download. Decoded chart content is exposed
to connected clients; configure access according to your chart permissions.

## Validation

```sh
cargo test --release --locked
cargo fmt --check
```

Synthetic tests cover fragmented files, continuation validation, embedded GMP
headers, signed coordinates, bit boundaries, variable lengths, sounding attributes,
truncation, cache isolation/budget, query validation and catalog selection with
synthetic files, including a deliberately invalid extra file.

The original Python reader and tests remain as development references.
`tests/rust-integration.py` compares a test server against that reference using
a private fixture manifest supplied with `--fixtures`. The manifest specifies
local files, matching relative catalog names, test bounds and detail levels.
The repository includes neither real-chart fixtures nor their manifests.
A minimal fixture manifest has this structure (all values below are fictional):

```json
{
  "fixtures": [{
    "path": "region/detail.img",
    "name": "region/detail.img",
    "samples": [{"bbox": [1.2, 1.2, 1.8, 1.8], "bits": 22}]
  }]
}
```

Keep real manifests in an ignored directory or a `*.private.json` file, then run:

```sh
python tests/rust-integration.py --url http://127.0.0.1:8765 --charts-root charts --fixtures fixtures.private.json
```

Optional browser smoke checks use `node tests/browser-smoke.cjs` with Playwright
and a running viewer. Tests print counts and status, never extracted geometry.

Keep generated artifacts private. Use only a small cold/warm detail check and a
two-client check for performance validation; no extensive benchmark suite is required.

## Limitations and licensing

Technical prototype, not for navigation. Symbols are simplified. Depth-area colors
have no verified depth legend. Some labels and attributes are omitted. Light sectors,
NET labels, Huffman coordinates, satellite imagery, 3D views and routing are not
implemented. A chart providing these features can still expose supported 2D content.

See [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md). Software is GPL-3.0-only;
chart content retains its own rights. Do not commit charts, extracted geometry,
overview databases or chart screenshots. Docker builds use an explicit source allowlist.
