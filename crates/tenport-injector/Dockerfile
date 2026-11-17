FROM rust:1.99.1-alpine3.22@sha256:9a744dc47d1ac2a7d17fc3cedfc8b4928b1696dfcba3274ab25bfb40988a9ff0 AS chef
RUN cargo install cargo-chef --locked --version 0.1.78 && rustup component add rustfmt clippy
WORKDIR /src

FROM chef AS plan
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS deps
COPY LICENSE ./
COPY --from=plan /src/recipe.json recipe.json

FROM deps AS check
ENV CARGO_PROFILE_DEV_DEBUG=0
RUN cargo chef cook --locked --all-targets -p tenport-injector --recipe-path recipe.json
RUN cargo chef cook --clippy --locked --all-targets -p tenport-injector --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo fmt --check -p tenport-injector && cargo clippy --locked -p tenport-injector --all-targets -- -D warnings && cargo test --locked -p tenport-injector

FROM deps AS build
RUN cargo chef cook --release --locked -p tenport-injector --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo build --release --locked -p tenport-injector

FROM scratch
COPY --from=build /src/target/release/injector /injector
USER 65534:65534
ENTRYPOINT ["/injector"]
