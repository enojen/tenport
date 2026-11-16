# CLAUDE.md

Tenport: the backend of Tenyard, in Rust. `README.md` says what is here, `docs/capabilities.md`
what it must do.

## Layout

- One Cargo workspace. Every crate is in `crates/`, flat, and its folder name is its crate name.
- Every crate name starts with `tenport-`. A binary keeps a short name through `[[bin]]`.
- Versions of dependencies and lints go in the root `Cargo.toml`; a crate uses
  `x.workspace = true`.
- `tenport-server` is one crate. Its modules follow the topics, not the layers: `accounts`,
  `projects`, `lanes`, `agents`, `runs`, `events`, `costs`, `cluster`. A topic folder has
  `model.rs` (types and rules) and `store.rs` (SQL).
- A `store.rs` function starts with its verb: `find_` returns one row or none
  (`find_user_by_token_hash`), `list_` a `Vec` (`list_memberships_of_user`), `count_` a number,
  `insert_` writes.
- Only `http/` knows axum: the router, auth, the error responses, OpenAPI, the panel's `sse`
  stream, and one file of endpoints per topic. A topic never imports `http`.
- No `mod.rs`: `foo.rs` next to the folder `foo/`. No module named `utils`, `helpers` or `common`.

## Code

- An id is a newtype (`UserId(String)`), never a bare `String`. A state is an enum.
- A value with rules is a type that can only be made valid: `Name::parse`, `GithubLogin::parse`,
  `Token::parse`. Parse once at the edge (HTTP, JSON through `serde(try_from)`); the code
  inside never checks again. No validation library.
- Each module has its own error type with `thiserror`, named `<Module>Error`: `ConfigError`,
  `DbError`, `HttpError`. `anyhow` only in `main.rs`.
- A trait only when it has two real implementations, such as a clock or the Kubernetes client
  for tests. Everything else is a concrete type.
- Logs go through `tracing`, never `println!` or `eprintln!`. `RUST_LOG` sets the filter.
- No `unsafe` (the workspace forbids it).

## Database

- SQL is written by hand. A new migration:
  `sqlx migrate add --source crates/tenport-server/migrations <name>`.
- A migration that has run is never changed: sqlx keeps its checksum and the server will not
  start. A fix is a new migration. No down migrations; the way back is the daily backup.
- Queries use `sqlx::query!` and `query_as!`, checked at compile time against the database in
  `.env` (`target/sqlx.db`). Make it once, and again after each new migration:
  `sqlx database create && sqlx migrate run --source crates/tenport-server/migrations`.
- After a query or a migration changes, `cargo sqlx prepare --workspace -- --all-targets` and
  commit `.sqlx/`. CI builds with `SQLX_OFFLINE=true` from it.
- Every change is an event: `events::append` writes it to the log and updates the tables in one
  transaction. Each topic has its own event enum (`AccountsEvent`) and its own `apply`; a table
  is written only there.

## Tests

- Unit tests in `#[cfg(test)] mod tests;` with the body in `tests.rs` next to the module.
- HTTP tests in one `tests/it/` crate. They call a real server over a real SQLite, no mocks.

## Before a PR

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace`. CI runs the same: `server.yml` for the server, the Dockerfile's `check`
stage for the injector and procs. `rust-toolchain.toml` pins Rust to the version of the images.

## Rules

- This repo is public: no IP, server or volume id, token or client name in code, commits, issues
  or PRs.
- An image is pinned by tag and digest where Tenyard uses it, never by `latest`.
