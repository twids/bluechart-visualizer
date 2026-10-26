FROM rust:1-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY web ./web
RUN cargo build --release --locked

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /build/target/release/bluechart /usr/local/bin/bluechart
COPY LICENSE THIRD_PARTY.md ./
USER 10001:10001
EXPOSE 8765
ENTRYPOINT ["bluechart"]
CMD ["serve", "--bind", "0.0.0.0", "--chart", "/charts/chart.img", "--charts-dir", "/charts", "--cache-dir", "/cache", "--overview", "/cache/overview.sqlite3"]
