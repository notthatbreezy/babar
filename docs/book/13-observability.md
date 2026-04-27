# 13. Observability

In this chapter we'll see what babar emits via `tracing` out of the
box, attach a subscriber, and pick the fields you want flowing into
your aggregator.

## Setup

```rust
use babar::codec::{int4, text};
use babar::query::Query;
use babar::{Config, Session};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    tracing_subscriber::fmt()                                          // type: Subscriber
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "babar=info".into()),
        )
        .with_target(false)
        .try_init()
        .ok();

    let session: Session = Session::connect(                           // type: Session
        Config::new("localhost", 5432, "postgres", "postgres")
            .password("postgres")
            .application_name("ch13-observability"),
    )
    .await?;

    let q: Query<(), (i32, String)> = Query::raw(
        "SELECT 1::int4, 'hello'::text",
        (),
        (int4, text),
    );
    let _ = session.query(&q, ()).await?;

    session.close().await?;
    Ok(())
}
```

## What babar emits

There is no babar-specific subscriber to register. Initialize any
`tracing` subscriber and you'll start seeing spans:

| Span | Where it fires | Useful fields |
|---|---|---|
| `db.connect` | `Session::connect` | `db.system`, `db.user`, `db.name`, `net.peer.name`, `net.peer.port` |
| `db.prepare` | `prepare_command` / `prepare_query` | `db.statement`, `db.operation` |
| `db.execute` | `session.execute`, `command.execute` | `db.statement`, `db.operation` |
| `db.transaction` | `session.transaction`, `tx.savepoint` | `db.operation` |

Field names follow OpenTelemetry's database semantic conventions, so
exporters (Jaeger, Tempo, Datadog APM, Honeycomb, …) understand them
without translation. `db.operation` is the first SQL keyword
(`SELECT`, `INSERT`, `BEGIN`, `SAVEPOINT`, …) — coarse but cheap to
group by.

## Picking a subscriber

| Subscriber | When to reach for it |
|---|---|
| `tracing_subscriber::fmt` | Local development, structured logs to stdout. |
| `tracing-bunyan-formatter` | JSON logs your aggregator already understands. |
| `tracing-opentelemetry` + an OTLP exporter | Distributed tracing alongside the rest of your services. |

The Axum example uses `tracing_subscriber::fmt` with an env filter:

```rust
tracing_subscriber::fmt()
    .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "babar=info".into()))
    .try_init()
    .ok();
```

That's enough to see span enter/exit lines for every connect, query,
and transaction — handy when something is stalling and you want to
know whether it's the pool, the prepare, or the server.

## Setting `application_name`

`Config::new(...).application_name("billing-svc")` is the cheapest
piece of observability babar offers. Postgres records it in
`pg_stat_activity.application_name`, so your DBA can see *which*
service is holding a long-running query open. Use a stable
service-level name; don't include a hostname or PID — the pool will
multiplex many connections from one process.

## What about metrics?

babar doesn't ship metrics directly — there's no built-in
`pool_acquire_latency_seconds` histogram, for example. You assemble
those at the boundary:

- Pool acquire latency: time `pool.acquire().await` yourself and feed
  it into `metrics::histogram!` (or whichever crate you use).
- Query latency: derive from the `db.execute` span duration via
  `tracing-opentelemetry`, or wrap your handlers in your service's
  metrics layer.
- Server-side stats (`pg_stat_statements`, `pg_stat_activity`): query
  them yourself with a periodic `Query` and push to your aggregator.
  babar gives you the round-trip; the policy is yours.

## What you can answer once this is wired up

- *"Which endpoint's `db.execute` p99 spiked at 14:32?"* — span
  histograms from your tracing backend.
- *"Was that an in-flight query or a connect-time stall?"* —
  `db.connect` vs `db.prepare` vs `db.execute` span breakdown.
- *"Which service held that connection open?"* — the
  `application_name` you set, surfaced by `pg_stat_activity`.

## You're done

That's the Book. From [Connecting](./01-connecting.md) to here, you
have the entire user-facing surface of babar — and a sense for how to
operate it in production.

For the precise types and methods, head to the
[Reference](../reference/codecs.md). For the *why* — design choices,
the background driver task, comparisons with other Rust Postgres
drivers — head to the [Explanation](../explanation/why-babar.md)
section.
