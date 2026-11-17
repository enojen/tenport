FROM rust:1.99.1-alpine3.22 AS base
RUN rustup component add rustfmt clippy
WORKDIR /src
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock LICENSE ./
COPY .sqlx .sqlx
COPY crates crates

FROM base AS check
RUN cargo fmt --check -p tenport-server -p tenport-run-credentials && cargo clippy --locked -p tenport-server -p tenport-run-credentials --all-targets -- -D warnings && cargo test --locked -p tenport-server -p tenport-run-credentials

FROM base AS build
RUN cargo build --release --locked -p tenport-server

FROM scratch
COPY --from=build /src/target/release/tenport /tenport
USER 65534:65534
ENV TENPORT_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/tenport"]
