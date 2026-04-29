# Design principles

> See also: [Why babar](./why-babar.md), the
> [Book](../book/01-connecting.md).

This page collects the principles babar is built around. They are not
abstract — every one of them produces a concrete API choice you can
point at.

## 1. Typed at the boundary

Every public call carries the parameter and row types. `Query<P, R>`
and `Command<P>` are values, not phantom decorations on a string. That
means:

- The compiler can reject `query.bind((1, 2))` against a
  `Query<(i32, String), _>` long before any wire I/O.
- A new reader of your code can see `Query<(i64,), (Uuid, String)>`
  and know the column shape without running anything.
- Refactoring a column type is a typecheck -- use the compiler.

The codecs are values too — `int4`, `text`, `bool` — not associated
methods on a trait object you have to remember.

## 2. Async

Every `Session` is backed by a background task that owns the
`TcpStream`. All public API calls send messages to that task over
channels and await the reply. This is the foundation of babar's
[cancellation safety](./driver-task.md): if your `await` is cancelled,
the task still finishes the in-flight protocol exchange before
servicing the next request.

## 3. Native protocol

babar speaks the Postgres v3 wire protocol directly via
`postgres-protocol`. It does not wrap `libpq`; it does not call out to
a C library; it does not translate through a higher-level abstraction.
That means:

- Binary results by default.
- Extended-protocol prepared statements with parameter codecs.
- SCRAM-SHA-256 (and SCRAM-SHA-256-PLUS with channel binding over TLS).
- Binary `COPY FROM STDIN` as a first-class API.
- `RowDescription` is parsed, the OIDs are checked, and the
  `Decoder` is given the bytes — no string-to-string conversion, no
  magic re-parsing.

If Postgres ships a new wire-level capability, the work to expose it
in babar is *Postgres-shaped*, not *abstraction-shaped*.

## 4. Validate don't parse

We would rather fail in your test suite than in production at 3am.
That is the validate-early principle in operation. Concretely:

- Every codec advertises its OIDs. When `RowDescription` arrives,
  babar checks that each declared OID matches what the server is
  about to send. Mismatches surface as `Error::SchemaMismatch`
  carrying the *position*, the *expected* OID, and the *actual* OID
  — at prepare time, before any rows are decoded.
- Every decoder advertises its column count. If `RowDescription`
  advertises a different count, you get `Error::ColumnAlignment`
  immediately, again before any rows are processed.
- The `query!` macro can validate SQL against a live database when
  `BABAR_DATABASE_URL` is set for opt-in compile-time validation.
- The `typed_query!` macro is a narrower, opt-in inline-schema POC: it
  resolves a supported `SELECT` subset at macro expansion time without
  claiming generated schema modules, offline caches, or full SQL
  coverage.

The cost is one round-trip on each prepare. The benefit is that
schema drift surfaces as a Rust error at the boundary, with a
caret-rendered message pointing at the offending fragment, rather
than as a cryptic decode panic on row 47.

## 5. No unsafe

babar's source contains no `unsafe` blocks. The macro crate sets
`unsafe_code = "forbid"` and the core crate is held to the same line
in CI (Miri).

## 6. Minimal dependencies, small features

The default feature set is small. Codec families (`uuid`, `time`,
`chrono`, `json`, `numeric`, `postgis`, `pgvector`, …) are gated
behind cargo features so that a pool-and-`text` service does not
have to compile a `geo-types` dependency it will never use. The TLS
backend is selectable at compile time (`rustls` by default,
`native-tls` available). Reduce footprint, reduce blast radius,
reduce compile time.

## 7. Operability is the API

Pool, statement cache, and `tracing` spans are first-class citizens,
not afterthoughts. `Session::connect` emits a `db.connect` span;
prepares emit `db.prepare`; executes emit `db.execute`. The fields
are OpenTelemetry's database semantic conventions out of the box, so
your existing tracing backend already understands them. Setting
`application_name` on `Config` puts your service name in
`pg_stat_activity` for free. The point is not that babar provides a
metrics dashboard — it does not — but that the seams a production
team needs are deliberately exposed.

## Where to read next

- [The driver task](./driver-task.md) for the cancellation-safety
  story.
- [Comparisons](./comparisons.md) for the trade-offs against other
  Rust Postgres clients.
- [Book Chapter 9 — Error handling](../book/09-error-handling.md) for
  what `validate-early` looks like at runtime.
