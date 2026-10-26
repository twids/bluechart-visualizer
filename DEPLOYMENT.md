# Docker Compose deployment

The image runs only the Rust application; Python is retained in the repository
as a development reference. Supply your own compatible charts separately.

## Configure charts

```text
bluechart-viewer/
  compose.yaml
  Dockerfile
  Cargo.toml
  Cargo.lock
  src/
  web/
  chart-config.example.json
  .env
  charts/
    chart.img
    region/
      other.img
  cache/
    overview.sqlite3
```

Create `charts/` and `cache/`, then copy `.env.example` to `.env`.
`CHARTS_DIR` points to a directory on the Docker host. The default file is
`chart.img` inside that directory. To choose a differently named default, edit
`--chart` in the viewer and overview service commands. Directory discovery is
recursive and happens at startup; symbolic links are not followed.

`CHART_CONFIG` names a JSON policy file; the sample is mounted at
`/config/charts.json`. Keys are paths relative to `/charts`. Supported bases
participate in Auto, higher priorities win, and `companion_of` connects an overlay
to its base. Unknown GMP files remain partial until verified. There is no file
upload, unlock or decryption workflow.

Paths in `.env` refer to the Docker host. Relative paths resolve beside Compose;
quote paths containing spaces. Charts are never copied into the image. Give user
`10001:10001` read/traverse access to chart directories and read access to caches.
Only the overview-building tool needs write access to the cache directory.

## Build and start

```sh
docker compose config --quiet
docker compose build
docker compose --profile tools run --rm build-overview
docker compose up -d viewer
docker compose ps
```

Open http://localhost:8765. Set `HTTP_PORT`, `BIND_ADDRESS` and `PUBLIC_HOST` in `.env`
as needed. `PUBLIC_HOST` must exactly match the browser Host including nonstandard
ports. Default access binds to loopback. Configure wider access deliberately;
clients receive decoded chart content even though IMG downloads are unavailable.

The default overview is `/cache/overview.sqlite3`. Additional charts use
`/cache/ID.sqlite3`; find IDs through `/api/charts` or the `inspect` command.
For example, to build an additional chart's overview:

```sh
docker compose --profile tools run --rm build-overview build-overview --chart /charts/region/other.img --output /cache/ID.sqlite3
```

The builder uses the same Rust parser, writes to a temporary database, checks
errors and SQLite integrity, then atomically publishes the completed cache.
It preserves an existing published cache on failure. An existing `.tmp` file
requires inspection before retrying. Supplementary contour-only files do not
need their own overview; they are hidden at broad scales while their base's
cached coastline remains visible.

Caches are bound to title, file size and modification time. Rebuild after replacing
a chart and restart the viewer. Do not replace chart contents underneath a running
viewer; stop/recreate it so open file handles and cached data have the same revision.

## Reverse proxy and Dockhand

Use `compose.dockhand.yaml` as the template. Configure `PROXY_NETWORK`, `PUBLIC_HOST`,
`CHARTS_DIR`, `CACHE_DIR`, `CHART_CONFIG` and `VIEWER_IMAGE` in the stack environment.
It exposes no host port. Point the reverse proxy to `http://bluechart:8765` on the
shared Docker network. It accepts the configured public Host plus `bluechart:8765`.
TLS and authentication belong at the proxy.

Build a versioned image before deployment and keep **Pull images** disabled for
locally built images. Deploy only this stack through Dockhand. The runtime preserves
user `10001:10001`, read-only filesystem and mounts, dropped capabilities,
no-new-privileges, a 512 MiB limit, one CPU, PID limit, restart policy and rotated logs.
Healthcheck is `bluechart healthcheck` and needs no interpreter or external utility.

## Verify, update and roll back

Verify the healthcheck, `/api/metadata`, `/api/charts`, a populated detail request,
zoom/pan, source selection, overview and any companion layers. Old generation
requests should return 409 and raw chart/source paths should return 404.

For updates, save the existing Compose file, build a new versioned image, update
the source snapshot and image tag, validate Compose and recreate only the viewer.
Keep the previous image and cache files. Restore the saved Compose file and
redeploy it to roll back, including its old command and mounts when changing
runtime languages. Merely selecting a Python image while retaining Rust commands
is not a valid rollback.

The Rust backend, unlike its predecessor, has no Python runtime dependency.
There is no side-by-side production migration service. Keep chart data, private
policies and generated overview databases outside Git and the Docker build context.
