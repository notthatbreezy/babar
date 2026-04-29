# Error catalog

> Generated rustdoc: <https://docs.rs/babar/latest/babar/enum.Error.html>

> See also: [Book Chapter 9 — Error handling](../book/09-error-handling.md).

## Variants

Every `babar::Error` variant match on error directly.

| Variant | Shape | When it fires |
|---|---|---|
| `Io` | `Io(std::io::Error)` | TCP, TLS, or socket I/O failure (DNS, refused, reset, EOF). |
| `Closed` | `Closed { sql: Option<String>, origin: Option<Origin> }` | The session was closed and the call lost its connection. `sql` and `origin` carry the in-flight statement. |
| `Protocol` | `Protocol(String)` | The server sent something babar can't make sense of (framing error, unexpected message). |
| `Auth` | `Auth(String)` | SCRAM rejected, password wrong, role can't log in, no password configured. |
| `UnsupportedAuth` | `UnsupportedAuth(String)` | The server selected an auth method babar doesn't speak (e.g. `gss`, `sspi`, or any code babar hasn't implemented). |
| `Server` | `Server { code, severity, message, detail, hint, position, sql, origin }` | An `ErrorResponse` from Postgres. `code` is the five-character SQLSTATE. |
| `Config` | `Config(String)` | Bad client-side configuration (malformed TLS settings, bad timeouts, …). |
| `Codec` | `Codec(String)` | An `Encoder` / `Decoder` rejected the bytes — wrong column count, NULL where not expected, malformed wire bytes. |
| `ColumnAlignment` | `ColumnAlignment { expected, actual, sql, origin }` | A `Decoder` was expecting `expected` columns but `RowDescription` advertised `actual`. |
| `SchemaMismatch` | `SchemaMismatch { position, expected_oid, actual_oid, column_name, sql, origin }` | The `Decoder`'s declared OID at `position` doesn't match the OID Postgres returned. |
| `Migration` | `Migration(MigrationError)` | A migration step failed; the inner enum carries the migration-specific cause. |

`Closed`, `Server`, `ColumnAlignment`, and `SchemaMismatch` carry an
`origin` field that, with macros like `sql!`, `query!`, `command!`, and
`typed_query!`, points at the call site (file:line:col). Surfacing it
in your logs almost always pays for itself the first time.

## SQLSTATE patterns

The `code` field on `Error::Server` is a five-character SQLSTATE.
This editorial section lists the codes most worth recognizing
explicitly — it is *guidance for application code*, not a
machine-extracted list. The full registry is in the Postgres docs
(<https://www.postgresql.org/docs/current/errcodes-appendix.html>).

### Constraint and concurrency

| SQLSTATE | Class | Common cause | Typical reaction |
|---|---|---|---|
| `23505` | unique_violation | Duplicate key on insert/upsert. | Map to a 409 in your service; consider `INSERT ... ON CONFLICT`. |
| `23503` | foreign_key_violation | Inserting a row whose parent doesn't exist. | 422 / validation error. |
| `23502` | not_null_violation | Missing required column. | 422 / validation error. |
| `23514` | check_violation | A `CHECK` constraint rejected the row. | 422 / validation error. |
| `40001` | serialization_failure | Conflicting concurrent transactions at SERIALIZABLE. | Retry with backoff. |
| `40P01` | deadlock_detected | The deadlock detector aborted your transaction. | Retry; investigate the lock order. |

### Authentication and resource

| SQLSTATE | Class | Common cause |
|---|---|---|
| `28P01` | invalid_password | Wrong password. |
| `28000` | invalid_authorization_specification | Role can't log in / `pg_hba.conf` rejected. |
| `53300` | too_many_connections | Server `max_connections` reached. Tune your pool. |
| `57P03` | cannot_connect_now | Server in startup or recovery; retry shortly. |

### Schema

| SQLSTATE | Class | Common cause |
|---|---|---|
| `42P01` | undefined_table | Missing table — typically a missing migration. |
| `42703` | undefined_column | Missing column — schema drift. |
| `42P07` | duplicate_table | A migration that already ran. |

## Choosing what to retry

A starting policy:

| Variant / code | Retry? |
|---|---|
| `Error::Io(_)` | Yes, with backoff. The connection is gone; the pool will reconnect. |
| `Error::Server { code: "40001", .. }` | Yes — the whole transaction. |
| `Error::Server { code: "40P01", .. }` | Yes — the whole transaction. |
| `Error::Server { code: "57P03", .. }` | Yes, after a delay. |
| `Error::Auth(_)` / `UnsupportedAuth(_)` | No. Surface to operator. |
| `Error::Codec(_)` / `ColumnAlignment` / `SchemaMismatch` | No. Fix the code. |
| Other `Error::Server` | No by default; classify per SQLSTATE. |

## Next

For the codec inputs that produce `Error::Codec` / `SchemaMismatch`,
see [codecs.md](./codecs.md). For the `Config` / `PoolConfig` knobs
that produce `Error::Config`, see [configuration.md](./configuration.md).
