# syntax=docker/dockerfile:1

# ---------- 构建 ----------
FROM rust:1-slim-bookworm AS build
WORKDIR /src

# 依赖单独缓存一层，改代码不用重装依赖
COPY Cargo.toml Cargo.lock* ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs \
 && cargo build --release --locked 2>/dev/null || cargo build --release
RUN rm -rf src

COPY src ./src
COPY web ./web
# 源码时间戳变化需要强制重建二进制本身
RUN touch src/main.rs && cargo build --release

# ---------- 运行 ----------
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -r -u 10001 -m -d /data tab

COPY --from=build /src/target/release/tab /usr/local/bin/tab

ENV TAB_DATA_DIR=/data \
    TAB_BIND=0.0.0.0 \
    TAB_PORT=8080 \
    TZ=Asia/Shanghai

VOLUME ["/data"]
EXPOSE 8080
USER tab
WORKDIR /data

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/usr/local/bin/tab", "--health"]

ENTRYPOINT ["/usr/local/bin/tab"]