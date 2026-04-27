# Roadmap

> See also: [`MILESTONES.md`](https://github.com/notthatbreezy/babar/blob/main/MILESTONES.md)
> in the repository for the authoritative milestone list.

This page summarizes how babar's roadmap is organized, what is
currently in scope per milestone, and what has been *intentionally*
deferred so the surface area stays honest.

Currently `babar` is in pre-Alpha -- I would not use it unless you want to contribute, find bugs, and improve its rust API. Work for the time being will be focused on stabilizing the developer API and identifying if there is a real desire for `babar`'s approach in the rust community.

## What's in now

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

## What's deferred

Some features are not currently in `babar` either due to time or choice. Currently missing features include:
 - A streaming-notifications API based on `LISTEN` / `NOTIFY` is on the roadmap but not yet shipped. Use a polling loop or a sidecar service in the meantime
 - DSN parsing / `Config::from_env()` and other connection APIs for convenience will be added as needs dictate 
 - ORM / query DSL is not currently planned, `babar` is focused on enabling writing SQL

## Where to read next

- [Why babar](./why-babar.md) — the high-level pitch.
- [Comparisons](./comparisons.md) — honest trade-offs vs other
  Rust Postgres clients.
