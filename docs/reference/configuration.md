# Configuration

> Generated rustdoc: <https://docs.rs/babar/latest/babar/struct.Config.html>

> See also: [Book Chapter 1 — Connecting](../book/01-connecting.md),
> [Chapter 6 — Pooling](../book/06-pooling.md),
> [Chapter 12 — TLS & security](../book/12-tls.md).

## `babar::Config`

`Config` holds everything `Session::connect` needs. Required fields
are positional in the constructor; optional fields are chained
methods. Build it from any source — env vars, a config file, a
`clap::Parser`. babar deliberately doesn't ship a DSN parser.

### Constructors

| Method | Required arguments |
|---|---|
| `Config::new(host, port, user, dbname)` | `impl Into<String>` for host/user/dbname, `u16` for port. Resolves `host` via DNS at connect time. |
| `Config::with_addr(addr, user, dbname)` | `impl Into<SocketAddr>`. Skips DNS — useful for IP-direct deployments. |

### Optional fields (chained, value-returning)

| Method | Type | Default | Notes |
|---|---|---|---|
| `.password(p)` | `impl Into<String>` | none | Sent to the server only as part of the auth handshake. |
| `.application_name(n)` | `impl Into<String>` | none | Surfaces in `pg_stat_activity.application_name`. Cheapest observability win. |
| `.connect_timeout(d)` | `Duration` | none | Wall-clock cap on `Session::connect`. |
| `.tls_mode(m)` | `TlsMode` | `Prefer` | `Disable` / `Prefer` / `Require`. See ch12. |
| `.require_tls()` | — | — | Sugar for `.tls_mode(TlsMode::Require)`. |
| `.tls_backend(b)` | `TlsBackend` | `Rustls` (with `rustls` feature) | `Rustls` or `NativeTls`. |
| `.tls_server_name(n)` | `impl Into<String>` | host | Override SNI / certificate-name match. |
| `.tls_root_cert_path(p)` | `impl Into<PathBuf>` | system roots / `webpki-roots` | PEM bundle of additional root CAs. |

### TLS-mode and backend enums

| Enum | Variants | Re-exported as |
|---|---|---|
| `TlsMode` | `Disable`, `Prefer`, `Require` | `babar::config::TlsMode` |
| `TlsBackend` | `Rustls`, `NativeTls` | `babar::config::TlsBackend` |

## `babar::PoolConfig`

`PoolConfig` is everything `Pool::new` needs that isn't a `Config`.

### Constructor

`PoolConfig::new()` — conservative defaults. All knobs are chained,
value-returning methods.

### Knobs

| Method | Type | Default | Notes |
|---|---|---|---|
| `.min_idle(n)` | `usize` | `0` | Keep at least `n` warm connections when traffic permits. |
| `.max_size(n)` | `usize` | `16` | Hard cap on total connections in the pool. |
| `.acquire_timeout(d)` | `Duration` | implementation default (~30s) | How long `pool.acquire()` waits before returning `PoolError::Timeout`. |
| `.idle_timeout(d)` | `Duration` | unset (no idle timeout) | Close idle connections older than this. |
| `.max_lifetime(d)` | `Duration` | unset (no lifetime cap) | Recycle connections after this age regardless of idle state. |
| `.health_check(h)` | `HealthCheck` | `HealthCheck::None` | Per-acquire validation policy (off by default). |

### `PoolError`

| Variant | When |
|---|---|
| `PoolError::Timeout` | `acquire_timeout` elapsed before a slot freed up. |
| `PoolError::AcquireFailed(babar::Error)` | The pool tried to open a fresh connection and the underlying `Session::connect` failed. |
| `PoolError::PoolClosed` | The pool itself has been closed. |

## Picking values

Some tested starting points:

| Service shape | `max_size` | `acquire_timeout` | `min_idle` |
|---|---|---|---|
| HTTP service, low/medium traffic | `8`–`16` | `5–10s` | `0` |
| HTTP service, high traffic | `≈ #worker threads × 2` | `1–3s` | `≥ 2` |
| Long-running batch / ETL | `1`–`4` | `30s+` | `0` |

Beyond that, watch:

- `pg_stat_activity` for connection count vs server's `max_connections`.
- Pool acquire latency (you wrap it yourself; see [Chapter 13](../book/13-observability.md)).
- p99 query latency vs pool size — if increasing `max_size` doesn't move p99, the pool isn't the bottleneck.

## Next

For the cargo features that gate TLS backends and codec types, see
[feature-flags.md](./feature-flags.md). For the errors these knobs
can produce, see [errors.md](./errors.md).
