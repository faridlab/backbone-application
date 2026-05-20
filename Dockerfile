# syntax=docker/dockerfile:1.7
#
# Multi-stage build for backbone-app.
#
#   chef     — shared base with rustc + cargo-chef installed.
#   planner  — produces `recipe.json` (dep manifest fingerprint) from
#              Cargo.toml / Cargo.lock only, so recipe changes only on
#              manifest edits.
#   builder  — cooks the recipe (builds deps once, cached), then builds the
#              binary. Source changes only invalidate the final compile step.
#   runtime  — distroless/cc-debian12; no shell, no package manager, ~25 MB.

# ─── Shared base ─────────────────────────────────────────────────────────
# Pin rustc minor version to avoid silent toolchain drift on rebuilds. Bump
# intentionally alongside Cargo.lock updates. Matches local `rustc --version`.
FROM rust:1.91-slim-bookworm AS chef
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config \
      libssl-dev \
      ca-certificates \
      git \
      protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked --version ^0.1

# ─── Planner ─────────────────────────────────────────────────────────────
# Copy only the manifests + a stub main so `cargo chef prepare` has a valid
# package to inspect. Recipe changes only when Cargo.toml/Cargo.lock change.
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo chef prepare --recipe-path recipe.json

# ─── Builder ─────────────────────────────────────────────────────────────
FROM chef AS builder

# Cook the recipe: compiles ALL deps and caches the layer. Reused across
# builds as long as recipe.json is unchanged.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Version metadata baked into the binary by build.rs (see ../build.rs).
# Defaults make local `docker build` work without flags. CI populates
# them from the git tag — see .github/workflows/release-image.yml.
ARG APP_VERSION=0.0.0-dev
ARG GIT_SHA=unknown
ARG BUILT_AT=unknown
ENV APP_VERSION=$APP_VERSION
ENV GIT_SHA=$GIT_SHA
ENV BUILT_AT=$BUILT_AT

# Real sources + final build. Only this layer re-runs on src/ edits or
# build-arg changes (ENV invalidates downstream cache layers).
COPY . .
RUN cargo build --release --bin backbone-app \
    && strip target/release/backbone-app

# ─── Runtime ─────────────────────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12 AS runtime
WORKDIR /app

COPY --from=builder /build/target/release/backbone-app /app/backbone-app
COPY --from=builder /build/config /app/config

EXPOSE 8080 50051 9090

# Distroless provides a pre-created `nonroot` user (UID 65532). The service
# binds to unprivileged ports (8080/50051/9090), so no root capabilities
# needed.
USER nonroot:nonroot

# Re-invokes the binary with the `healthcheck` subcommand (distroless has
# no shell / curl, so we can't shell out). Start period covers DB pool
# warmup + module init — tighten once startup is faster.
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
    CMD ["/app/backbone-app", "healthcheck"]

# Distroless has no shell; argv entrypoint only.
ENTRYPOINT ["/app/backbone-app"]
CMD []
