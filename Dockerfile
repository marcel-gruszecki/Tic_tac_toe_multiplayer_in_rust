# ── Etap 1: Budowanie ─────────────────────────────────────────────────────────
# sqlx weryfikuje zapytania SQL w czasie kompilacji, więc DATABASE_URL
# musi być dostępne podczas budowania.
# Przekaż je przez: docker build --build-arg DATABASE_URL="..." .
FROM rust:1.87-slim AS builder

ARG DATABASE_URL
ENV DATABASE_URL=${DATABASE_URL}

WORKDIR /app

# Instalacja zależności systemowych potrzebnych do kompilacji
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Skopiuj manifest i pobierz zależności (warstwa cache)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch

# Skopiuj kod źródłowy i zbuduj release
COPY src ./src
RUN cargo build --release


# ── Etap 2: Obraz końcowy ──────────────────────────────────────────────────────
FROM debian:bookworm-slim

WORKDIR /app

# Zależności runtime (OpenSSL, certyfikaty CA)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Skopiuj skompilowany binarny z etapu budowania
COPY --from=builder /app/target/release/Serwer ./server

# Serwer nasłuchuje na porcie 3000
EXPOSE 3000

# DATABASE_URL musi być przekazane przez -e lub plik .env / docker-compose
ENV DATABASE_URL=""

CMD ["./server"]
