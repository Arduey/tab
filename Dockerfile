# 镜像只装「已经编译好的静态二进制」，不在容器里编译 Rust。
#
# 为什么这么做：
#  1. CI 产出的 x86_64-unknown-linux-musl 是静态链接，不依赖 glibc，
#     Alpine / Debian / CentOS 都能直接跑；
#  2. 与宝塔上手动部署用的是同一个文件，行为完全一致，出问题好对照；
#  3. 镜像从 ~800MB 降到 ~20MB，构建从十几分钟降到几秒。
#
# 需要自己从源码构建镜像时（没有 CI 产物）：
#   docker build --build-arg BIN=./target/x86_64-unknown-linux-musl/release/tab -t tab .

FROM alpine:3.20

# ca-certificates：抓取网站 favicon 走 HTTPS 必须有根证书
# tzdata：日志时间戳按本地时区显示
RUN apk add --no-cache ca-certificates tzdata \
 && addgroup -g 10001 -S tab \
 && adduser -u 10001 -S -G tab -h /data tab \
 && mkdir -p /data \
 && chown -R tab:tab /data

# 允许用 --build-arg 指定产物路径，默认取 CI 下载的位置
ARG BIN=tab-linux-x86_64
COPY ${BIN} /usr/local/bin/tab
RUN chmod +x /usr/local/bin/tab

ENV TAB_DATA_DIR=/data \
    TAB_BIND=0.0.0.0 \
    TAB_PORT=8080 \
    TZ=Asia/Shanghai

VOLUME ["/data"]
EXPOSE 8080
USER tab
WORKDIR /data

# 二进制自带 --health：连一下本机端口就返回，不依赖 curl
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/tab", "--health"]

ENTRYPOINT ["/usr/local/bin/tab"]
