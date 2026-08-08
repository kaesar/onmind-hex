# hex

Servicio ABCode de **arquitectura hexagonal** en **Rust + Feather + BoaJS (abcodelib)**, que replica el contrato de **hex4w** (Spring WebFlux) pero con **scripts `.abc`** en lugar de Javascript/GraalJS, y con **abcodefun** prevaleciendo de forma separada (este crate **no** expone un invoke tipo lambda).

## Contrato (rutas hex4w)

| Método | Ruta | Descripción |
|--------|------|-------------|
| `POST` | `/api/v1/script/execute` | Ejecuta un script `*.abc` **whitelisteado** → `{ value, stdout, stderr }` |
| `GET`  | `/api/v1/xdb/sheet?show=&from=&some=` | Lista hojas XDB (`/abc`) |
| `GET`  | `/health` | Health check (público aun en modo JWT) |

Seguridad: **whitelist de nombres de scripts** + rechazo de path-traversal (único control requerido). Auth JWT/NoAuth vía `JWT_SECRET` (igual que abcodefun).

## Arquitectura

```
src/
  domain/          # modelos puros + DomainError (ScriptResult, StoreItem, AbcResponse)
  application/     # puertos (out), facade `services.*`, whitelist, use cases
  infrastructure/  # adapter: engine ABCode (compile+execute), source de scripts, XDB HTTP
  graph.rs         # composition root (leo env, inyecta adapters)
  main.rs          # Feather App: rutas hexes + middleware JWT
```

El facade `services.*` (inyectado como global en Boa por abcodelib) agrega los puertos de salida, igual que `ScriptServicesFacade` de hex4w. Mismas firmas: `abcSheet`, `abcExec`, `cacheGet/Set/Evict`, y parciales `publish`, `invoke`, `invokeAsync`, `listItems`, `sendEmail` (devuelven `Unsupported` hasta montar sus adapters).

## Scripts (`scripts/*.abc`)

```abcode
goal: any
fun: handler()
  val: cached = services.cacheGet("demo-key")
  run: services.cacheSet("hello-flag", "1", 300)
  pass: {"hello": "hex", "cached": cached}
run: handler()
```

## Config de ambiente

| Var | Default | Nota |
|-----|---------|------|
| `HEX_SCRIPTS_DIR` | `./scripts` | directorio de `.abc` |
| `HEX_SCRIPTS_WHITELIST` | `hello.abc` | CSV de permitidos |
| `HEX_XDB_BASE_URL` | `http://localhost:9990` | feature `abc` |
| `PORT` | `3001` | puerto HTTP |
| `JWT_SECRET` | (vacío) | habilita JWT si se setea |

## Features Rust (style serial perfil hex4x)

| Feature | Adapter | Requiere* |
|---------|---------|-----------|
| `abc` (default) | XDB `/abc` HTTP (`reqwest`) | `HEX_XDB_BASE_URL` |
| `cache` (default) | cache in-memory (Redis) | — |
| `store` | MintStore → S3 (`aws-sdk-s3`) | `HEX_STORE_BUCKET` |
| `events` | publish → SNS (`aws-sdk-sns`) | envío vía creds AWS |
| `lambda` | invoke / invokeAsync → Lambda (`aws-sdk-lambda`) | creds AWS |
| `email` | sendEmail → SMTP (`lettre`, bloqueante) | `HEX_SMTP_HOST/FROM` (+`USER/PASSWORD`) |
| `graphql` | BFF GraphQL en `/graphql` (`async-graphql`) | — |
| `grpc` | gRPC XDB/Sheet en `HEX_GRPC_PORT` (50051) (`tonic`+`prost`) | protoc (\*) |

\* «protoc» se requiere en build-time solo para la feature `grpc` (genera `proto/xdb.proto`).

Los adapters cloud usan la cadena estándar de credenciales de AWS (env / profile). Con `store`/`events`/`lambda`, si falta la config viable, el adapter se omite y el service ejecuta `Unsupported`.

### Circuit breaker
Todos los `services.*` de I/O (XDB, cache) pasan por un **circuit breaker** (estilo resilience4j, puro, sin dep): `CLOSED → OPEN → HALF_OPEN`.

| Env | Default |
|-----|---------|
| `HEX_CB_FAILURE_THRESHOLD` | `5` |
| `HEX_CB_RESET_MS` | `500` |
| `HEX_CB_HALF_OPEN_MAX` | `1` |

Al abrirse el circuito, las llamadas responden **503 `SERVICE_UNAVAILABLE`** sin tocar el backend.

### gRPC (feature `grpc`)
Corre en su propio runtime Tokio (thread de fondo). `proto/xdb.proto` → paquete `hex4w.xdb`, servicio `Xdb::Sheet` (unary). Comparte el `Graph` con REST/GraphQL.

### GraphQL (feature `graphql`)
`POST /graphql` con body `{"query","variables","operation_name"}`. Campos: `health`, `execute(script)`, `sheet(show,from,some)`.

`services.*` expuesto a scripts (hex4w `ScriptServicesFacade`):

```
abcSheet, abcExec        cacheGet, cacheSet, cacheEvict
saveItem, getItem, listItems, deleteItem
publish                  invoke, invokeAsync          sendEmail
```

```bash
cargo build                # features default (abc + cache)
cargo build --all-features # todo: nube + graphql + grpc
cargo test
cargo clippy --all-targets
```

> MSRV: 1.94. Las versiones `aws-*` están fijadas a la línea Rust 1.91.1 (compatible
> con 1.94.0) a través del `Cargo.lock`; no se aplica un bump del toolchain.

## Roadmap posterior
- SQS / EventBridge como destino de `publish` (hoy SNS).
- Kafka / RabbitMQ (features) como buses adicionales.
- Fire-and-forget `abcExec` (hoy `Unsupported`).
- Redis como implementación real del puerto `cache`.

---
© 2021-2026 by César Andres Arcila Buitrago