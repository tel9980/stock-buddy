# ── 前端构建 ──
FROM node:20-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm ci || npm install
COPY frontend/ ./
RUN npm run build

# ── Rust 构建 ──
FROM rust:1.88-slim AS backend
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release

# ── 运行时：nginx 托管前端静态文件 + 反代 API/WS ──
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 nginx-light && rm -rf /var/lib/apt/lists/*

COPY --from=backend /build/target/release/stock-buddy /app/stock-buddy
COPY --from=backend /build/migrations /app/migrations
COPY --from=frontend /app/frontend/dist /usr/share/nginx/html

# nginx 配置：SPA 静态托管 + /api 反代 + /ws WebSocket 升级
RUN cat > /etc/nginx/sites-available/default <<'EOF'
server {
    listen 8080;
    server_name _;

    root /usr/share/nginx/html;
    index index.html;

    # 前端 SPA：history 路由回退到 index.html
    location / {
        try_files $uri $uri/ /index.html;
    }

    # API 反代到本地 Rust 服务
    location /api/ {
        proxy_pass http://127.0.0.1:8081;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 300s;   # AI 流式响应耗时较长
        proxy_buffering off;       # SSE 需要关闭缓冲
    }

    # WebSocket 反代
    location /ws/ {
        proxy_pass http://127.0.0.1:8081;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 3600s;
    }
}
EOF

COPY <<'EOF' /app/entrypoint.sh
#!/bin/bash
set -e

# 数据与日志目录
mkdir -p /app/data/reports /app/logs
cd /app

# 后端监听 8081（nginx 占 8080 对外）
export LISTEN_ADDR=0.0.0.0:8081
export DB_PATH=/app/data/stock.db
export REPORT_DIR=/app/data/reports

/app/stock-buddy &
API_PID=$!
nginx -g "daemon off;" &
NGINX_PID=$!

trap 'kill $API_PID $NGINX_PID 2>/dev/null' TERM INT

# dash 不支持 wait -n，改用 bash + 轮询任一进程退出即退出
while kill -0 $API_PID 2>/dev/null && kill -0 $NGINX_PID 2>/dev/null; do
    sleep 5
done
exit 1
EOF
RUN chmod +x /app/entrypoint.sh

ENV TZ=Asia/Shanghai \
    RUST_LOG=info
VOLUME ["/app/data"]
EXPOSE 8080
CMD ["/app/entrypoint.sh"]
