# hex

App-template with **hexagonal architecture** in **Rust + Feather + BoaJS (abcodelib)** (based in [ABCode](https://github.com/kaesar/abcode/tree/main/abcodefun) service), which replicates the contract of [**hex4w**](https://github.com/kaesar/onmind-hex4w) (Spring WebFlux) but with **`.abc` scripts** instead of Javascript/GraalJS, and with **abcodefun** taking precedence separately (this crate **does not** expose a lambda-type invoke).

## Contract (hex4w routes)

| Method | Route | Description |
|--------|------|-------------|
| `POST` | `/api/v1/script/execute` | Executes a whitelisted `*.abc` script → `{ value, stdout, stderr }` |
| `GET`  | `/api/v1/xdb/sheet?show=&from=&some=` | Lists XDB sheets (`/abc`) |
| `GET`  | `/health` | Health check (public even in JWT mode) |

Security: **script name whitelist** + path-traversal rejection (the only required control). JWT/Auth-NoAuth via `JWT_SECRET` (same as abcodefun).

## Architecture

```
src/
  domain/          # pure models + DomainError (ScriptResult, StoreItem, AbcResponse)
  application/     # ports (out), facade `services.*`, whitelist, use cases
  infrastructure/  # ABCode engine adapter (compile+execute), script source, XDB HTTP
  graph.rs         # composition root (leo env, injects adapters)
  main.rs          # Feather App: hex routes + JWT middleware
```

The `services.*` facade (injected as global in Boa by abcodelib) adds the output ports, same as `ScriptServicesFacade` in hex4w. Same signatures: `abcSheet`, `abcExec`, `publish`, `invoke`, `invokeAsync`, `listItems`, `sendEmail`, `cacheGet/Set/Evict`. When the feature's corresponding adapter is not mounted, the service responds `Unsupported` (the full surface is always present).

## Scripts (`scripts/*.abc`)

```abcode
goal: any
fun: handler()
  val: cached = services.cacheGet("demo-key")
  run: services.cacheSet("hello-flag", "1", 300)
  pass: {"hello": "hex", "cached": cached}
run: handler()
```

## Environment Configuration

| Var | Default | Note |
|-----|---------|------|
| `HEX_SCRIPTS_DIR` | `./scripts` | script directory |
| `HEX_SCRIPTS_WHITELIST` | `hello.abc` | allowed CSV |
| `HEX_XDB_BASE_URL` | `http://localhost:9990` | feature `abc` |
| `PORT` | `3001` | HTTP port |
| `JWT_SECRET` | (empty) | enables JWT if set |

## Rust Features (serial style hex4x profile)

| Feature | Adapter | Requires* |
|---------|---------|-----------|
| `abc` (default) | XDB `/abc` HTTP (`reqwest`) | `HEX_XDB_BASE_URL` |
| `cache` (default) | in-memory cache (Redis) | — |
| `store` | MintStore → S3 (`aws-sdk-s3`) | `HEX_STORE_BUCKET` |
| `events` | publish → SNS (`aws-sdk-sns`) | AWS credentials via env |
| `lambda` | invoke / invokeAsync → Lambda (`aws-sdk-lambda`) | AWS credentials |
| `email` | sendEmail → SMTP (`lettre`, blocking) | `HEX_SMTP_HOST/FROM` (+`USER/PASSWORD`) |
| `graphql` | BFF GraphQL at `/graphql` (`async-graphql`) | — |
| `grpc` | gRPC XDB/Sheet at `HEX_GRPC_PORT` (50051) (`tonic`+`prost`) | protoc (\*) |

* `protoc` is required at build-time only for the `grpc` feature (generates `proto/xdb.proto`).

Cloud adapters use the standard AWS credential chain (env / profile). With `store`/`events`/`lambda`, if viable config is missing, the adapter is omitted and the service executes `Unsupported`.

### Circuit breaker

All `services.*` I/O (XDB, cache) go through a **circuit breaker** (resilience4j style, pure, no dep): `CLOSED → OPEN → HALF_OPEN`.

| Env | Default |
|-----|---------|
| `HEX_CB_FAILURE_THRESHOLD` | `5` |
| `HEX_CB_RESET_MS` | `500` |
| `HEX_CB_HALF_OPEN_MAX` | `1` |

When the circuit opens, calls respond with **503 `SERVICE_UNAVAILABLE`** without touching the backend.

### gRPC (feature `grpc`)

Runs in its own Tokio runtime (background thread). `proto/xdb.proto` → package `hex4w.xdb`, service `Xdb::Sheet` (unary). Shares the `Graph` with REST/GraphQL.

### GraphQL (feature `graphql`)

POST /graphql with body `{"query","variables","operation_name"}`. Fields: `health`, `execute(script)`, `sheet(show,from,some)`.

`services.*` exposed to scripts (hex4w `ScriptServicesFacade`):

```
abcSheet, abcExec        cacheGet, cacheSet, cacheEvict
saveItem, getItem, listItems, deleteItem
publish                  invoke, invokeAsync          sendEmail
```

```bash
cargo build                # default features (abc + cache)
cargo build --all-features # all: cloud + graphql + grpc
cargo test
cargo clippy --all-targets
```

> MSRV: 1.94. The `aws-*` versions are pinned to Rust line 1.91.1 (compatible with 1.94.0) via `Cargo.lock`; no toolchain bump is applied.

© 2025 by César Andres Arcila Buitrago