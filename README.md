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
Charts are mounted read-only; the cache is writable for automatic search indexing.
Both are excluded from images.

## Run with Rust

Install the stable Rust toolchain and a C compiler (for bundled SQLite), then:

```sh
cargo build --release --locked
target/release/bluechart build-overview --chart charts/chart.img --output cache/overview.sqlite3
target/release/bluechart serve --chart charts/chart.img --charts-dir charts --cache-dir cache --overview cache/overview.sqlite3 --chart-config chart-config.example.json
```

The default listener is http://127.0.0.1:8765. On Windows use
`Start-Viewer.ps1 -ChartPath C:/charts/chart.img -ChartsDir C:/charts`.
The script builds the Rust executable if necessary.

## Charts and controls

The selection menu lists charts discovered recursively when the server starts.
**All / Auto** is selected initially and selects one supported
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
selected chart.

**3D view** toggles a perspective view of the chart plane, with a **Tilt** slider
from 0–60°. Pan, zoom, search and object inspection also work when tilted. Terrain
and seabed heights are not displayed. See [VISION.md](VISION.md) for the camera,
format investigation and read-only marine section inventory tool.
Hover over or tap points, lines and areas to inspect their readable type, chart
label, available depth and coordinates. Unknown types retain their raw code and
are explicitly marked as unidentified. Marine type names depend on geometry:
the same code can identify different point, line and area features. Boundaries
and symbols are simplified; color bands do not establish numeric depth ranges.
Unidentified polygon outlines, including unverified chart extensions, are
available through the optional **Unidentified areas** layer, which is off by
default.
Chart changes clear old objects and cancel superseded requests. Supplementary
contours are detail-only and share the 180-subdivision request budget with the base.

### Place search

The **Find a place** box searches decoded names anywhere in the selected chart,
including outside the current view. **All / Auto** searches supported base charts;
explicit selections also allow partial base charts. Supplementary overlays are excluded.
Use the search box at the top: type at least two characters for instant suggestions
below the input. Click a suggestion or use Up/Down and Enter to select it. Enter
without a highlighted suggestion (or the Search button) opens results in the left
sidebar. Selecting a sidebar result keeps the list available for comparing places;
Back to chart settings restores the controls. Editing the query returns to instant
suggestions. Escape dismisses suggestions first; another Escape or the clear button
clears the search and location highlight. Selecting a result
centres and zooms the map without changing the chart selection. Names match without
case or accents (`Ostergard` matches `Östergård`); exact names rank before prefixes
and substrings. At most 20 results are shown. Results depend on decoder support.

On every server startup, missing, stale or invalid search indexes are built in the
background for each readable base chart. Existing valid indexes are reused. New or
replaced chart files are discovered on restart. The cache directory must be writable
by the viewer (Docker user `10001:10001`). Map browsing, health checks and searches
against available indexes remain responsive while indexing runs; search warns
when an index is not yet available. Progress and build failures appear in server logs. A normal stop cancels indexing
and removes its temporary file so the next startup can retry.

To force a rebuild manually, use the **same chart discovery arguments and paths**
as the viewer so chart IDs match:

```sh
target/release/bluechart build-search --chart charts/chart.img --charts-dir charts --cache-dir cache --chart-config chart-config.example.json
```

Each `cache/ID.search.sqlite3` scans all non-inherited detail levels in ordinary
marine/base tiles. Fishing/auxiliary tiles and depth soundings are excluded.
Duplicate names within roughly one metre are collapsed; identical names at separate
locations remain separate results. Large charts may take time to index.
Indexes are published atomically; failed builds preserve the previous index and do
not prevent other charts being indexed. A leftover `.sqlite3.tmp` file requires
inspection before retrying. Chart revisions use chart ID, file size and modification
time. Do not replace charts under a running viewer; restart after changes. Existing
overview caches do not need rebuilding for this feature.

Search uses no external service. Extracted names and indexes are private chart data
and must stay outside Git and distributed images.

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
Missing or invalid overview caches are built in the background at startup and become available when complete. Broad views show a warning while a cache is unavailable; check server logs if building fails.

## Read API

- `GET /api/charts`: stable chart IDs, relative names, titles, coverage, support
  status, capabilities, companion relationships and default ID. No absolute paths.
- `GET /api/metadata?chart=ID`: chart metadata, initial view, coverage, overview
  availability and limitations. `chart=auto` returns combined supported coverage.
- `GET /api/features?chart=ID&bbox=WEST,SOUTH,EAST,NORTH&bits=23`: decoded objects,
  warnings, active chart ID/title and statistics. Coordinates use WGS 84.
- `GET /api/search?chart=ID&q=TEXT`: up to 20 ranked `results` with `chart_id`,
  `chart_title`, `label`, `kind`, `type`, `coordinates` (`[lon,lat]`) and `bounds`
  (`[west,south,east,north]`), plus `warnings` for unavailable indexes.
  Omit `chart` for the default or use `auto` for all supported base charts.
  Trimmed queries require 2–100 characters without control characters; malformed
  queries, unknown charts and overlay selections return HTTP 400. Busy search
  workers return HTTP 503. Query text is literal, including SQL wildcard characters.

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

Feature naming and geometry inspection have synthetic checks that do not need
chart files: `node tests/features.cjs`. The browser regression check is
`node tests/features-browser.cjs` and requires Playwright and Microsoft Edge
(or a Chromium channel selected with `BLUECHART_BROWSER`). It checks the actual
viewer against an in-memory synthetic API, including touch taps and layer toggles.

Synthetic tests cover fragmented files, continuation validation, embedded GMP
headers, signed coordinates, bit boundaries, variable lengths, sounding attributes,
truncation, cache isolation/budget, query validation and catalog selection with
synthetic files, including a deliberately invalid extra file.

Optional browser smoke checks use `node tests/browser-smoke.cjs` with Playwright
and a running viewer. Rust tests use synthetic data; no real-chart fixtures,
extracted geometry, private fixture manifests or screenshots are bundled.

`node tests/search-browser.cjs` checks search with a synthetic API and Playwright:
debouncing, keyboard and mouse selection, distant locations, Auto scope, missing
indexes, failures, superseded requests, chart changes and location highlights.
It uses Microsoft Edge by default (`BLUECHART_BROWSER` selects another installed
Chromium channel). Rust search tests cover accent folding, ranking, deduplication,
query validation, scope, result limits, revisions and atomic build failure handling.

Keep generated artifacts private. Use only a small cold/warm detail check and a
two-client check for performance validation; no extensive benchmark suite is required.

## Limitations and licensing

Technical prototype, not for navigation. Symbols are simplified. Depth-area colors
have no verified depth legend. Some labels and attributes are omitted. Light sectors,
NET labels, Huffman coordinates, satellite imagery, terrain/seabed 3D and routing are not
implemented. A chart providing these features can still expose supported 2D content.

See [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md). Software is GPL-3.0-only;
chart content retains its own rights. Do not commit charts, extracted geometry,
overview databases or chart screenshots. Docker builds use an explicit source allowlist.
