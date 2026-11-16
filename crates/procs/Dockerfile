FROM rust:1.99.1-alpine3.22 AS base
RUN rustup component add rustfmt clippy
WORKDIR /src
COPY Cargo.toml Cargo.lock LICENSE ./
COPY crates crates

FROM base AS check
RUN cargo fmt --check -p procs && cargo clippy --locked -p procs --all-targets -- -D warnings && cargo test --locked -p procs

FROM base AS build
RUN cargo build --profile procs --locked -p procs

FROM scratch
COPY --from=build /src/target/procs/procs /procs
COPY crates/procs/groups.conf /config/groups.conf
USER 65534:65534
ENTRYPOINT ["/procs"]
