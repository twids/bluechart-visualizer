FROM python:3.12-slim-bookworm
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
WORKDIR /app
COPY bluechart.py server.py build_overview.py LICENSE THIRD_PARTY.md ./
COPY web ./web
USER 10001:10001
EXPOSE 8765
CMD ["python", "server.py", "--bind", "0.0.0.0", "--chart", "/charts/norden.img", "--overview", "/cache/overview.sqlite3", "--allowed-host", "garmin.widsell.nu", "--allowed-host", "bluechart:8765"]
