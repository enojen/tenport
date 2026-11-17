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
RUN cargo chef cook --locked --all-targets -p tenport-server -p tenport-run-credentials --recipe-path recipe.json
RUN cargo chef cook --clippy --locked --all-targets -p tenport-server -p tenport-run-credentials --recipe-path recipe.json
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY .sqlx .sqlx
COPY crates crates
RUN cargo fmt --check -p tenport-server -p tenport-run-credentials && cargo clippy --locked -p tenport-server -p tenport-run-credentials --all-targets -- -D warnings && cargo test --locked -p tenport-server -p tenport-run-credentials

FROM deps AS build
RUN cargo chef cook --release --locked -p tenport-server --recipe-path recipe.json
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY .sqlx .sqlx
COPY crates crates
RUN cargo build --release --locked -p tenport-server

FROM scratch
COPY --from=build /src/target/release/tenport /tenport
USER 65534:65534
ENV TENPORT_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/tenport"]
