# Stage 1: Generate a dependency recipe using cargo-chef
FROM rust:1.93-slim AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Build dependencies (cached) then the application
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# Stage 3: Minimal runtime image
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/rust-confluence-documenter ./

# Required at runtime (must be provided via docker run -e or compose):
#   DATABASE_URL - PostgreSQL connection string (e.g. postgres://user:pass@host:5432/dbname)
#
# Optional:
ENV PORT=3000
ENV RUST_LOG=info

EXPOSE 3000

CMD ["./rust-confluence-documenter"]
