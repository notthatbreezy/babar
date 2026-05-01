# 6. Error handling and service boundaries

This chapter is the companion to the async chapter. Once you can read the
waiting points, the next skill is reading the failure paths. In `babar`, that
usually means understanding three moves:

1. propagate an error with `?`
2. inspect an error with `match`
3. translate a low-level error into an application-facing one at the boundary

## babar anchor

Keep these pages open while reading:

- [9. Error handling](../book/09-error-handling.md)
- [11. Building a web service](../book/11-web-service.md)
- [1. Connecting](../book/01-connecting.md)
- [Error catalog](../reference/errors.md)

Together they show the whole path from connection setup to HTTP response.

## Start with the function signature

Rust makes failure visible in the type signature. In the `babar` docs you will
see shapes such as:

```rust
async fn main() -> babar::Result<()> { /* ... */ }
async fn initialize(pool: &Pool) -> babar::Result<()> { /* ... */ }
fn db_http(err: babar::Error) -> (StatusCode, String) { /* ... */ }
```

Read them literally:

- `babar::Result<()>` means “this function may fail with `babar::Error`”
- `Result<T, (StatusCode, String)>` means “this handler either returns success
  data or an HTTP-facing error payload”
- `()` means “successful completion matters, but there is no extra success value”

The signature already tells you where responsibility for the error currently
lives.

## What `?` means in `babar` code

In both [1. Connecting](../book/01-connecting.md) and
[9. Error handling](../book/09-error-handling.md), the `?` operator appears on
async database calls:

```rust
let session = Session::connect(cfg).await?;
session.execute(&create, ()).await?;
let pool = Pool::new(cfg, PoolConfig::new().max_size(8)).await?;
```

`?` is short for a very specific decision:

- if the call succeeded, keep going with the success value
- if the call failed, return that error from the current function immediately

So when you read `await?`, split it into two questions:

1. what outside work are we waiting for?
2. if that work fails, who is responsible next?

That makes `?` much less mysterious. It is not swallowing errors. It is
propagating them on purpose.

## When to use `?` and when to use `match`

Use `?` when the current function is **not** the place that adds meaning.

Use `match` when the current function **is** the place that must classify or
translate the error.

That is why the error-handling chapter includes:

```rust
match session.execute(&insert, (2, "ada".into())).await {
    Ok(_) => unreachable!(),
    Err(err) => match classify(&err) {
        Failure::Duplicate => println!("duplicate name; skipping"),
        Failure::ServerOther { code } => println!("server error {code}"),
        Failure::IoOrClosed => println!("connection died; retry later"),
        Failure::Bug => println!("our bug, not the server's: {err}"),
    },
}
```

The code is not just asking “did this fail?”. It is asking “what kind of
failure is this, and what should the application do next?”.

## `babar::Error` is an enum, so classification is explicit

[9. Error handling](../book/09-error-handling.md) stresses one rule: there is no
`Error::kind()` convenience classifier. You inspect the variant directly.

That is good Rust practice for a learner to notice. It means the library is not
hiding the shape of failure from you. A few useful buckets are:

- `Error::Io(_)` and `Error::Closed { .. }` for transport-level trouble
- `Error::Auth(_)` and `Error::UnsupportedAuth(_)` for authentication failures
- `Error::Server { code, .. }` for Postgres server errors
- `Error::Config(_)` for setup mistakes caught before I/O
- `Error::Codec(_)`, `Error::SchemaMismatch { .. }`, and
  `Error::ColumnAlignment { .. }` for typed-data mismatches

This is one of the biggest Rust differences from exception-heavy code: the error
cases are normal values, and pattern matching is the standard way to reason
about them.

## Why SQLSTATE is the stable boundary

When Postgres rejects a statement, `Error::Server { code, message, .. }` carries
both a machine-friendly code and a human-friendly message. The docs tell you to
match the **SQLSTATE** instead of the message text:

```rust
Error::Server { code, .. } if code == "23505" => Failure::Duplicate
```

That is more reliable because:

- SQLSTATE is designed for programmatic handling
- messages are written for humans and may vary in wording
- your service logic usually cares about categories such as duplicate key,
  foreign-key violation, timeout, or retryable transaction failure

Use the [Error catalog](../reference/errors.md) when you need the wider table of
codes; use the book chapter when you need the mental model.

## Service boundaries are where translation happens

The web-service chapter shows the next layer up:

```rust
fn db_http(err: babar::Error) -> (StatusCode, String) {
    match err {
        babar::Error::Server { code, .. } if code == "23505" => {
            (StatusCode::CONFLICT, "already exists".into())
        }
        babar::Error::Server { code, .. } if code == "23503" => {
            (StatusCode::UNPROCESSABLE_ENTITY, "foreign key violation".into())
        }
        other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}
```

This is an important application-flow boundary:

- below the boundary, code talks in driver/database terms
- at the boundary, code translates that into HTTP or domain terms
- above the boundary, callers should not need to understand `babar::Error`

That translation is where application meaning appears. A duplicate key becomes a
conflict. A missing foreign-key target becomes an unprocessable request. A pool
timeout may become service unavailable.

## Separate operational detail from client-facing meaning

[11. Building a web service](../book/11-web-service.md) explicitly warns against
returning every raw database error straight to the client. That is a good rule
for two reasons:

1. low-level messages may leak implementation detail
2. clients usually need stable application meaning, not driver internals

A practical reading rule:

- **logs and diagnostics** may include the detailed `babar::Error`
- **client responses** should usually expose a smaller, domain-appropriate shape

## Pool errors and database errors are related, but not identical

Service code often handles two fallible steps in sequence:

```rust
let conn = state.pool.acquire().await.map_err(pool_http)?;
let rows = conn.query(&select, (id,)).await.map_err(db_http)?;
```

That split is worth noticing:

- `PoolError` answers “could the application obtain a usable connection?”
- `babar::Error` answers “what went wrong while speaking to Postgres?”

Both are database-adjacent, but they are not the same boundary. Good service
code usually keeps that distinction clear.

## Python comparison (optional)

If you come from Python, Rust error values can feel like exceptions made
explicit. The Rust-first lesson is stricter than that: failure paths are part of
the function type, propagation is visible in `?`, and classification is usually
done with `match`, not by catching a broad exception late.

## Checkpoint

Try to answer these from the docs examples:

- When a `babar` call ends with `await?`, which function has agreed to handle
  the error next?
- Why is `code == "23505"` a better service boundary than checking whether an
  error message contains the word “duplicate”?
- In the Axum example, which failures should stay in logs, and which should be
  translated into stable HTTP meanings?

## Read next

- [Traits, generics, and codecs](07-traits-generics-and-codecs.md)
- [9. Error handling](../book/09-error-handling.md)
- [11. Building a web service](../book/11-web-service.md)
- [Error catalog](../reference/errors.md)
