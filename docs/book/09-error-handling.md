# 9. Error handling

This chapter covers the `babar::Error` enum, classifying failures
by inspecting the variant directly, and pulling out the SQLSTATE codes
your retry logic actually wants.

If you want a Rust-first bridge for `?`, `match`, and translating database
failures at a service boundary, pair this with the optional companion chapter
[Error handling and service boundaries](../rust-learning/06-error-handling-and-service-boundaries.md).

## Setup

```rust
use babar::codec::{int4, text};
use babar::query::Command;
use babar::{Config, Error, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let session: Session = Session::connect(                          // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch09-errors"),
    )
    .await?;

    let create: Command<()> =
        Command::raw("CREATE TEMP TABLE err_demo (id int4 PRIMARY KEY, name text NOT NULL UNIQUE)");
    session.execute(&create, ()).await?;

    let insert: Command<(i32, String)> = Command::raw_with(
        "INSERT INTO err_demo (id, name) VALUES ($1, $2)",
        (int4, text),
    );
    session.execute(&insert, (1, "ada".into())).await?;

    // Second insert violates the UNIQUE constraint — classify it.
    match session.execute(&insert, (2, "ada".into())).await {
        Ok(_) => unreachable!(),
        Err(err) => match classify(&err) {                            // type: Failure
            Failure::Duplicate => println!("duplicate name; skipping"),
            Failure::ServerOther { code } => println!("server error {code}"),
            Failure::IoOrClosed => println!("connection died; retry later"),
            Failure::Bug => println!("our bug, not the server's: {err}"),
        },
    }

    session.close().await?;
    Ok(())
}

#[derive(Debug)]
enum Failure {
    Duplicate,
    ServerOther { code: String },
    IoOrClosed,
    Bug,
}

fn classify(err: &Error) -> Failure {
    match err {
        Error::Server { code, .. } if code == "23505" => Failure::Duplicate,
        Error::Server { code, .. } => Failure::ServerOther { code: code.clone() },
        Error::Io(_) | Error::Closed { .. } => Failure::IoOrClosed,
        _ => Failure::Bug,
    }
}
```

## The `babar::Error` enum, in one breath

There is no `Error::kind()` accessor. Classification is by `match` on
the variant:

| Variant | When you see it |
|---|---|
| `Error::Io(io::Error)` | Socket-level failure — DNS, TCP reset, TLS handshake. |
| `Error::Closed { sql, origin }` | Server hung up or the driver task shut down with an in-flight request. |
| `Error::Protocol(String)` | The server (or driver) sent a wire-protocol message that doesn't fit the state machine. Always a bug somewhere. |
| `Error::Auth(String)` | SCRAM rejected, password wrong, role can't log in. |
| `Error::UnsupportedAuth(String)` | Server asked for an auth method babar doesn't speak (e.g. `gss`, `sspi`). |
| `Error::Server { code, severity, message, detail, hint, position, sql, origin }` | `ErrorResponse` from Postgres. `code` is SQLSTATE — match on it. |
| `Error::Config(String)` | Configuration problem caught before any I/O. |
| `Error::Codec(String)` | An encoder or decoder rejected a value. |
| `Error::ColumnAlignment { expected, actual, sql, origin }` | Decoder column count ≠ server's `RowDescription`. |
| `Error::SchemaMismatch { position, expected_oid, actual_oid, column_name, sql, origin }` | Decoder OID ≠ server's column type. |
| `Error::Migration(MigrationError)` | The migrator's planning or apply step failed. |

That's eleven. They cover everything. You can build a small `classify`
function once per service, and call it everywhere.

## Why SQLSTATE matters more than the message

`Error::Server.message` is for humans. `Error::Server.code` (a
five-character SQLSTATE) is for code. A few you may see often:

| SQLSTATE | Class | Meaning |
|---|---|---|
| `23505` | `unique_violation` | Duplicate key. |
| `23503` | `foreign_key_violation` | Missing FK target. |
| `23502` | `not_null_violation` | NULL into a `NOT NULL` column. |
| `40001` | `serialization_failure` | Serializable transaction must retry. |
| `40P01` | `deadlock_detected` | Deadlock; retry the whole transaction. |
| `57014` | `query_canceled` | Statement timeout fired. |
| `57P01` | `admin_shutdown` | Server is going away. |

The full list is in [reference/errors.md](../reference/errors.md). For
a retry budget on serialization failures, match on `40001` and run
the transaction body again with backoff.

## `origin` and `sql` for diagnostics

Several variants carry `sql: Option<String>` and `origin:
Option<Origin>`. The `sql!` macro captures its callsite as an
`Origin`, so when an error fires from inside a fragment-built query,
the `Display` impl can point you back to the macro invocation —
file, line, column. Surface those in your logs and you'll spend a lot
less time bisecting which `INSERT` blew up.

## Translating to your service's error type

At the boundary of your application, fold `babar::Error` into your
domain error. The pattern from the Axum example is a good starting
shape:

```rust
fn db_error(err: babar::Error) -> (StatusCode, String) {
    match err {
        babar::Error::Server { code, .. } if code == "23505" => {
            (StatusCode::CONFLICT, "already exists".into())
        }
        babar::Error::Auth(_) | babar::Error::UnsupportedAuth(_) => {
            (StatusCode::UNAUTHORIZED, "auth failed".into())
        }
        other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}
```

## Next

- [Chapter 10: Custom codecs](./10-custom-codecs.md) shows how to write your own
  `Encoder<A>` / `Decoder<A>` for types babar doesn't know about out of the box.
- For the optional Rust-learning companion, see
  [Error handling and service boundaries](../rust-learning/06-error-handling-and-service-boundaries.md).
