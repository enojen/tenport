FROM rust:1.99.1-alpine3.22 AS base
RUN rustup component add rustfmt clippy
WORKDIR /src
COPY Cargo.toml Cargo.lock LICENSE ./
COPY crates crates

FROM base AS check
RUN cargo fmt --check -p tenport-injector && cargo clippy --locked -p tenport-injector --all-targets -- -D warnings && cargo test --locked -p tenport-injector

FROM base AS build
RUN cargo build --release --locked -p tenport-injector

FROM scratch
COPY --from=build /src/target/release/injector /injector
USER 65534:65534
ENTRYPOINT ["/injector"]
