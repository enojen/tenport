# Tenport

Tenport is the backend of Tenyard. It is one service, written in Rust, that holds all the state:
the users and their organizations, the projects, the lanes and their tasks, the agents, the
events and the costs. Everything else talks to it over HTTP.

`docs/capabilities.md` says what Tenport must do. Read it first.

## The crates

The repository is a Cargo workspace: one `Cargo.toml` and one `Cargo.lock` at the root, and one
crate for each program under `crates/`. Each program has its own image.

| Crate | What it is | Image |
|---|---|---|
| `crates/injector` | The proxy that writes the real token into a box's request to GitHub or its tracker | `ghcr.io/enojen/tenport-injector` |
| `crates/procs` | The sidecar of each box pod: the CPU and memory of each group of processes | `ghcr.io/enojen/tenport-procs` |

The server crate comes next. The injector and procs came from the tenyard repository, where
their earlier history stays.

## The four repositories

| Repository | What it is | How it uses Tenport |
|---|---|---|
| `tenport` | The backend: state, accounts, tokens, agents; also the injector and procs | |
| `tenbox` | The CLI that runs one task through plan, build, check, review and PR | Sends events, asks for state, starts agents. It keeps the git and worktree work |
| `tenface` | The panel: lanes, tasks and agents, on the phone first | Reads the state over HTTP and SSE. It only shows; it does not change state |
| `tenyard` | The cluster: k3s on Hetzner, Flux, monitoring | Runs Tenport's images. Holds their manifests and Secrets, not their code |

Tenport replaces the server in `tenbox/server` (TypeScript). That server keeps running until
Tenport does the same work.

## The API contract

Tenport writes the contract as an OpenAPI file. The panel and the CLI make their types from that
file, so the three repositories cannot disagree about a field.

## First steps with Rust

Rust is installed on this computer with `rustup` (rustc 1.100, edition 2024). The tools are in
`~/.cargo/bin`.

The commands you will use every day:

| Command | What it does |
|---|---|
| `cargo build` | Compiles every crate |
| `cargo build -p injector` | Compiles one crate |
| `cargo run -p procs` | Compiles and starts one program |
| `cargo test` | Runs the tests |
| `cargo fmt` | Formats the code |
| `cargo clippy` | Lints the code; treat each warning as an error |
| `cargo add <crate>` | Adds a library to `Cargo.toml` |

In VS Code, install the `rust-analyzer` extension. It shows the types, the errors and the
completions while you write.

Where to learn:

- "The Rust Programming Language" (the book): https://doc.rust-lang.org/book/. Chapters 1 to 10
  are enough to start. Ownership (chapter 4) is the part that is new for a TypeScript developer.
- "Rust by Example": https://doc.rust-lang.org/rust-by-example/, short examples for each topic.
- `crates/injector` is a small Rust service that already runs on the cluster. It uses `tokio`
  and `hyper`, the same base as the server will use.

## The stack (proposed, not decided)

| Need | Crate | Why |
|---|---|---|
| Async runtime | `tokio` | The base of almost every Rust server; the injector uses it |
| HTTP and SSE | `axum` | Built on `tokio` and `hyper`; has SSE support |
| Database | `sqlx` with SQLite | Checks each SQL query at compile time. The tenbox server uses SQLite today |
| OpenAPI | `utoipa` | Makes the OpenAPI file from the Rust types |
| Kubernetes | `kube` | Watches Jobs, Pods, Nodes and PVCs, and makes Secrets |
| JSON | `serde`, `serde_json` | The standard way to read and write JSON |
| Errors | `thiserror` | One error type per module, with clear messages |
| Logs | `tracing` | Structured logs |

## License

FSL 1.1 with Apache 2.0 as the future license. See `LICENSE`.
