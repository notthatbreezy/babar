# Roadmap

> See also: [`MILESTONES.md`](https://github.com/notthatbreezy/babar/blob/main/MILESTONES.md)
> in the repository for the authoritative milestone list.

This page summarizes how babar's roadmap is organized, what is
currently in scope per milestone, and what has been *intentionally*
deferred so the surface area stays honest.

## How milestones work

`MILESTONES.md` (in the repo root) breaks development into
sequentially numbered milestones — `M0`, `M1`, … — each with:

- A scope statement (what the milestone covers).
- Concrete deliverables.
- A test policy (unit, integration, property-based, where relevant).
- Acceptance criteria — the milestone is *not done* until every box
  is checked and CI is green against every supported Postgres
  version.

The point is to keep "shipped" honest: a milestone you are inside is
work-in-progress; a milestone that is checked off ships exactly what
its acceptance list said it would.

## What's in (high level)

Across the early milestones, babar has shipped:

- Wire protocol foundation: framing, startup, parameter status,
  graceful shutdown, the [driver task](./driver-task.md).
- Authentication: cleartext, MD5, SCRAM-SHA-256, SCRAM-SHA-256-PLUS
  (channel binding over TLS).
- The typed core: `Session`, `Query<P, R>`, `Command<P>`,
  `Fragment<A>`, the `Encoder`/`Decoder` traits, and codec
  combinators (`nullable`, tuples, `array`, `range`, `multirange`).
- The primitive codec set and the optional codec families
  ([reference/codecs.md](../reference/codecs.md)).
- Prepared statements with a per-session cache, portal-backed
  streaming, and `prepare_command` / `prepare_query`.
- Closure-shaped transactions and savepoints.
- Binary `COPY FROM STDIN` for bulk ingest.
- Pool with health checks, idle timeouts, and lifetime caps.
- A library-first migration engine with advisory locking and
  checksums.
- TLS via `rustls` (default) or `native-tls`.
- `tracing` spans with OpenTelemetry semantic conventions.

For the day-to-day surface, the [Book](../book/01-connecting.md) is
the right entry point.

## What's deferred (and why)

Some things are deliberately *not* in babar — yet, or by design.
Calling them out here keeps the trade-offs visible.

| Capability | Status | Notes |
|---|---|---|
| `LISTEN` / `NOTIFY` | Deferred | A streaming-notifications API is on the roadmap but not yet shipped. Use a polling loop or a sidecar service in the meantime. |
| `COPY TO` (server → client) | Deferred | Only `COPY FROM STDIN` ingest is shipped. Read-side bulk export will land in a later milestone. |
| Text/CSV `COPY` | Deferred | Binary `COPY` is the supported path; text/CSV variants are tracked but not yet on the public surface. |
| Out-of-band cancellation | Deferred | `tokio::select!` and `Session::close` cover most cases; an explicit cancel-request channel is on the roadmap. |
| DSN parsing / `Config::from_env()` | By design | babar deliberately does not ship a DSN parser. `Config::new(host, port, user, db)` plus chained methods is the only configured path; build it from whichever source fits your service. |
| ORM / query DSL | By design | babar is a typed Postgres client, not an ORM. `Fragment<A>` and `sql!` give you composable SQL; row mapping is a `Decoder<R>`. |
| Multi-database backends | By design | babar is Postgres only. The wire protocol is the abstraction; we are not chasing MySQL or SQLite. |

## Where work is heading

The next-milestone work tends to be one of three shapes:

1. **Surface gaps in shipped Postgres capabilities** —
   `LISTEN`/`NOTIFY`, `COPY TO`, out-of-band cancel.
2. **Codec breadth** — more extension support, more `geo-types` shapes,
   more `time` / `chrono` round-trip cases.
3. **Operability polish** — metrics surfaces, more ergonomic
   `tracing` spans, statement-cache observability.

The authoritative list is `MILESTONES.md`. If something on this page
disagrees with the repo's `MILESTONES.md`, trust the repo.

## How to follow along

- The repo's `MILESTONES.md` and `CHANGELOG.md` track shipped work.
- GitHub issues and milestones map roughly to the same scheme.
- Pull requests are tagged with the milestone they belong to where
  applicable.

## Where to read next

- [Why babar](./why-babar.md) — the high-level pitch.
- [Comparisons](./comparisons.md) — honest trade-offs vs other
  Rust Postgres clients.
