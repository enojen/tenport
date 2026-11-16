FROM rust:1.99.1-alpine3.22 AS base
RUN rustup component add rustfmt clippy
WORKDIR /src
COPY Cargo.toml Cargo.lock LICENSE ./
COPY crates crates

FROM base AS check
RUN cargo fmt --check -p tenport-procs && cargo clippy --locked -p tenport-procs --all-targets -- -D warnings && cargo test --locked -p tenport-procs

FROM base AS build
RUN cargo build --profile procs --locked -p tenport-procs

FROM scratch
COPY --from=build /src/target/procs/procs /procs
COPY crates/tenport-procs/groups.conf /config/groups.conf
USER 65534:65534
ENTRYPOINT ["/procs"]
