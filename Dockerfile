FROM rust:slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch

COPY src ./src
RUN cargo build --release


FROM debian:bookworm-slim AS server

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/Serwer ./server

EXPOSE 3000

CMD ["./server"]


FROM httpd:2.4 AS web

RUN sed -i \
    -e '/mod_proxy\.so/s/^#//' \
    -e '/mod_proxy_http\.so/s/^#//' \
    -e '/mod_proxy_wstunnel\.so/s/^#//' \
    -e '/mod_proxy_connect\.so/s/^#//' \
    -e '/mod_rewrite\.so/s/^#//' \
    conf/httpd.conf \
 && echo "Include conf/extra/apache.conf" >> conf/httpd.conf

COPY apache.conf /usr/local/apache2/conf/extra/apache.conf
COPY index.html /usr/local/apache2/htdocs/index.html
