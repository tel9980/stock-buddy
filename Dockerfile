# ── 前端构建 ──
FROM node:20-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm ci || npm install
COPY frontend/ ./
RUN npm run build

# ── Rust 构建 ──
FROM rust:1.83-slim AS backend
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
RUN cargo build --release

# ── 运行时 ──
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /build/target/release/stock-buddy-api ./stock-buddy-api
COPY --from=frontend /app/frontend/dist ./static
ENV LISTEN_ADDR=0.0.0.0:8080 \
    DB_PATH=/app/data/stock.db \
    REPORT_DIR=/app/data/reports \
    RUST_LOG=info
VOLUME ["/app/data"]
EXPOSE 8080
CMD ["./stock-buddy-api"]
