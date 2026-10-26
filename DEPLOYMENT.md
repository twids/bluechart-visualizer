# Docker Compose deployment

Requires Docker with Compose v2. Python is included in the image. Run commands
from the project directory. Supply your own compatible chart separately;
chart content is not included with the software.

## 1. Supply a chart

```text
bluechart-viewer/
  compose.yaml
  Dockerfile
  .env
  charts/
    chart.img
  cache/
    overview.sqlite3    # generated in the next step
```

Create directories and local settings:

```sh
mkdir charts cache
cp .env.example .env
```

In PowerShell, use `mkdir charts, cache` and `Copy-Item .env.example .env`.
Place your chart at `charts/chart.img`, or change `CHART_FILE` in `.env`:

```dotenv
CHART_FILE=/path/to/your/chart.img
```

The path refers to the Docker host. With a remote Docker server, the file must
exist on that server. Relative paths resolve beside `compose.yaml`. Quote paths
containing spaces in `.env`.

The file is mounted read-only at `/charts/chart.img` and is never baked into
the image. Missing bind-mount paths produce an error. Chart directories,
cache directories, and `.env` are Git-ignored. One chart is selected at a time.
Compatibility across all g2/g3 products is unverified. This deployment workflow
does not unlock or decrypt charts. Extract archives before use.

## 2. Build and start

The container runs as `10001:10001`. On Linux, this user needs read access to the
chart and traversal access to its parent directories. Make the newly created
cache directory writable by this user:

```sh
sudo chown 10001:10001 cache
sudo chmod 755 cache
```

When running as root, omit `sudo`. Preserve the chart's owner and use your
host's existing permission model to grant read access. Docker Desktop on
Windows manages bind-mount permissions.

```sh
docker compose config --quiet
docker compose build
docker compose --profile tools run --rm build-overview
docker compose up -d viewer
docker compose ps
```

Open http://localhost:8765. Check the metadata endpoint:

```sh
curl --fail http://localhost:8765/api/metadata
```

`overview_available` should be `true`. Cache generation may take time and must
finish successfully before startup. Only the cache builder has a writable
cache mount; the viewer mounts it read-only. The cache contains derived chart
data and must remain private.

If a build is interrupted, `cache/overview.sqlite3.tmp` may remain. Confirm that
no builder is running, remove that incomplete temporary file, and retry.
A completed cache is replaced only after a successful build.

## 3. Network access

The default port binds to loopback. To allow LAN access, configure your host's
address and port in `.env`, for example:

```dotenv
BIND_ADDRESS=0.0.0.0
HTTP_PORT=8765
PUBLIC_HOST=viewer.example.test:8765
```

Arrange DNS or a hosts entry for your chosen hostname, then run
`docker compose up -d viewer`. Open `http://viewer.example.test:8765`.
Alternatively, use your host's LAN IP address in `PUBLIC_HOST`.

`PUBLIC_HOST` must exactly match the browser's Host header, including a
nonstandard port and excluding the URL scheme or path. A mismatch returns
HTTP 403. If changing the port, update both `HTTP_PORT` and `PUBLIC_HOST`.
The internal healthcheck continues to use port 8765.

The application has no built-in authentication. Restrict access using your
network or an authenticated reverse proxy as appropriate for your chart's
permissions. The API sends decoded chart content to connected clients.

## Replace a chart

1. Run `docker compose stop viewer`.
2. Place the new chart on the host and update `CHART_FILE` in `.env`.
3. Run `docker compose --profile tools run --rm build-overview`.
4. Run `docker compose up -d --force-recreate viewer`.
5. Check metadata and rendering.

Always rebuild the overview for the new chart. Cache identity uses file size,
modification time, and chart title; it is not a full content hash. Do not replace
a file while the viewer is running. Keep the previous chart and matching cache
separately if you need a quick rollback.

## Update or stop

After updating source code:

```sh
docker compose build
docker compose stop viewer
docker compose --profile tools run --rm build-overview
docker compose up -d viewer
docker compose ps
docker compose logs --tail 50 viewer
```

`docker compose down` removes the stack's containers and network. Host chart
and cache files remain. For reproducible rollback, retain the previous source
or image and its matching chart/cache separately.

## Dockhand and reverse proxies

`compose.dockhand.yaml` is a generic proxy deployment template. Configure its
build context, chart/cache paths, image tag, exact allowed Host header, and
external Docker network for your environment. Its default external network is
`proxy`; create or select the network your reverse proxy actually uses.
The internal service endpoint is `http://bluechart:8765`. This template does
not publish a host port. Keep production paths and domain settings in private
local environment configuration.

Build a locally tagged image from source before deploying through your stack
manager. Disable image pulling when using a local image. Build the overview
using the same chart and image, and mount it read-only in the viewer.
