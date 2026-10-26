# BlueChart Viewer

An experimental browser viewer for user-supplied Garmin IMG marine charts.
Supports panning, zooming, chart coverage, depth soundings, labels, simplified
symbols, layer controls, and a locally generated overview cache.

No chart files, extracted chart data, caches, or chart screenshots are included.
Supply your own compatible chart and ensure you have permission to use it.
The software license does not grant any rights to chart content.

## Docker Compose

Docker Compose is the recommended deployment method. Place your chart at
`charts/chart.img`, or set `CHART_FILE` in your local `.env` file. Chart files
are mounted read-only and are never included in the container image.

See [DEPLOYMENT.md](DEPLOYMENT.md) for setup, permissions, cache generation,
network access, chart replacement, updates, and reverse proxy configuration.

## Run with Python

Python 3.10 or later is required. No third-party Python packages are needed.
Place your chart at `charts/chart.img`, then run:

```sh
python build_overview.py --chart charts/chart.img
python server.py --chart charts/chart.img
```

Open http://127.0.0.1:8765 and stop the server with Ctrl+C.
On Windows, `Start-Viewer.ps1` starts the server using Python from PATH:

```powershell
.\Start-Viewer.ps1 -ChartPath "C:/charts/chart.img"
```

For a chart stored elsewhere, pass its path with `--chart` to both commands.
The overview is written to `.cache/overview.sqlite3`. It contains derived chart
data and must remain private. Rebuild it whenever the chart changes. Without
an overview cache, detailed views remain available, but broad views may show
only the coverage boundary and a warning.

## Controls

Drag to pan; scroll, double-click, or use the buttons to zoom. Arrow keys pan,
plus/minus zoom, and Home restores the initial view when the canvas has focus.
The coverage button fits the entire chart. Hover over point features for
available labels, depth, type, and coordinates. Toggle layers in the sidebar.

## How it works

The reader opens chart files read-only and uses seekable reads rather than
loading the entire file. Tile indexes are loaded on demand. Geometry and label
caches are bounded; the overview uses a spatially indexed SQLite database.
Large views select coarser levels and limit the number of decoded subdivisions.

The viewer uses Python's standard library and browser APIs. It does not request
external map tiles, fonts, or scripts. The local server binds to loopback by
default and validates the HTTP Host header. It serves only the viewer assets
and read APIs; it does not provide a download endpoint for the original IMG.
The feature API does expose decoded chart content to connected clients.

## Read API

- `GET /api/metadata`: chart metadata, coverage, initial view, overview status,
  warnings, and limitations.
- `GET /api/features?bbox=west,south,east,north&bits=23`: decoded features,
  warnings, and read statistics. Coordinates use WGS 84 longitude/latitude.
  `fishing=0` skips the fishing layer; `compact=1` reduces response size.
  Responses support gzip. Client/generation identifiers cancel superseded views.

These APIs are experimental. Keep access appropriate to your chart permissions.

## Validation

Synthetic tests require no chart file:

```sh
python -m unittest discover -s tests -v
```

Optional integration checks use a separately supplied chart. Set
`BLUECHART_INTEGRATION=1`, `BLUECHART_CHART` to its path, and
`BLUECHART_TEST_BBOX` to a small populated area as `west,south,east,north`.
Then run the same test command. These checks do not establish general product
compatibility or navigational accuracy.

For local measurements, choose a small populated bounding box within your chart:

```sh
python measure.py --chart charts/chart.img --bbox WEST SOUTH EAST NORTH
python benchmark.py --chart charts/chart.img --bbox WEST SOUTH EAST NORTH
```

Optional browser checks require Playwright and a running viewer:

```sh
node tests/browser-smoke.cjs
```

Set `BLUECHART_BROWSER` to an installed Playwright browser channel and
`BLUECHART_URL` to the local viewer URL if needed. Generated measurements and
screenshots stay in the ignored `artifacts/` directory and must not be published
with chart content.

## Limitations

This is a technical prototype, not a navigation tool. Symbol rendering is
simplified. Depth-area colors do not have a verified depth legend. Some labels
and objects may be omitted. Light sectors, Vision features, routes, NET labels,
and Huffman-coded geometry are not fully supported. Compatibility across all
g2/g3 products is unverified. Decoding warnings appear in the interface.

## Licensing and chart content

See [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md) for the software
license and upstream attribution. Chart content retains its separate rights.

Do not commit or distribute chart files, extracted geometry, overview databases,
or screenshots containing chart content with this project. Local chart and
cache directories, IMG files, databases, generated artifacts, and archives are
ignored by Git. The Docker build uses an allowlist of source files and assets.
