# 1. Connecting

In this chapter we'll meet `Config`, `Session::connect`, and the
background driver task that keeps every call you make
cancellation-safe.

## Setup

```rust
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let cfg = Config::new("localhost", 5432, "postgres", "postgres")
        .password("postgres")
        .application_name("ch01-connecting")
        .connect_timeout(std::time::Duration::from_secs(5));

    let session: Session = Session::connect(cfg).await?;        // type: Session
    println!(
        "server_version = {}",
        session.params().get("server_version").unwrap_or("?"),
    );
    session.close().await?;
    Ok(())
}
```

## `Config` is a struct, not a string

`Config::new(host, port, user, database)` takes the four required
fields by position. Optional fields are added by chained methods —
`.password(...)`, `.application_name(...)`, `.connect_timeout(...)`,
TLS settings, and so on. Because `Config` is a plain struct you can
build it from any source you like (env vars, a config file, a
`clap::Parser`); babar deliberately doesn't ship a DSN parser or a
`Config::from_env()`. Connection details should be visible in code
review.

## What `Session::connect` actually does

`Session::connect(cfg)` opens one TCP connection to Postgres,
negotiates TLS if you asked for it, runs the SCRAM-SHA-256 handshake,
exchanges startup parameters, and hands you back a `Session`. From
that moment on, the `Session` is a thin handle: the *real* socket
ownership lives in a background Tokio task that the `Session` spawns.

That background task is the reason every public call on `Session` is
cancellation-safe. If you `tokio::select!` away from a query midway
through, the protocol stays in a consistent state — the driver task
finishes reading the in-flight messages even if you don't await the
result. The shape of the model is sketched in
[What makes babar babar](../explanation/what-makes-babar-babar.md#1-the-background-driver-task);
we dive into the details in
[explanation/driver-task.md](../explanation/driver-task.md).

## Reading server parameters

```rust
let v = session.params().get("server_version").unwrap_or("?");
let tz = session.params().get("TimeZone").unwrap_or("?");
println!("server_version={v}, TimeZone={tz}");
```

`session.params()` returns the `ParameterStatus` map Postgres sent
during startup. It's read-only and updated by the server when it
issues a `ParameterStatus` message.

## Closing politely

`session.close().await` sends a `Terminate` and waits for the driver
task to drain. If you drop the `Session` without calling `close`, the
background task is still cancelled cleanly — but `close` lets you
observe a final `Result` if the server objected to anything.

## Recovering when the server is unreachable

`Session::connect` returns `babar::Result<Session>`. The error is the
same `babar::Error` enum you'll meet in
[Chapter 9](./09-error-handling.md); for connection failures you'll
typically see `Error::Io(_)` (DNS, TCP, TLS) or `Error::Server {
code, .. }` (auth rejected, database missing). Inspect the variant
directly — there's no `Error::kind()` classifier.

## Next

[Chapter 2: Selecting](./02-selecting.md) walks through reading rows
back into typed Rust values.
