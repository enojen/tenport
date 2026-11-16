FROM rust:1.99.1-alpine3.22 AS base
RUN rustup component add rustfmt clippy
WORKDIR /src
COPY Cargo.toml Cargo.lock LICENSE ./
COPY crates crates

FROM base AS check
RUN cargo fmt --check -p injector && cargo clippy --locked -p injector --all-targets -- -D warnings && cargo test --locked -p injector

FROM base AS build
RUN cargo build --release --locked -p injector

FROM scratch
COPY --from=build /src/target/release/injector /injector
USER 65534:65534
ENTRYPOINT ["/injector"]
