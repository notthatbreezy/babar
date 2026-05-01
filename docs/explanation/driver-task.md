# The driver task

> See also: [Book Chapter 1 — Connecting](../book/01-connecting.md),
> [Design principles](./design-principles.md), and the optional Rust-learning
> companion [Async/await and the driver task mental model](../rust-learning/05-async-await-and-the-driver-task.md).

Every `Session` in babar is backed by a single background task that
owns the underlying `TcpStream`. This page explains what that task is,
what it does, and why it exists.

If you want the shortest Rust-first mental model before reading the deeper
architecture details here, start with
[Async/await and the driver task mental model](../rust-learning/05-async-await-and-the-driver-task.md)
and then return to this page.

## Shape of the model

When you call `Session::connect`, babar:

1. Opens the TCP connection and runs the startup + auth handshake.
2. Spawns a background task (`tokio::spawn`) and gives it the read
   half and write half of the now-authenticated stream.
3. Hands you back a `Session` value that holds an
   `mpsc::Sender<Command>` — the channel into the driver task — plus
   a small amount of cached server state (parameters, backend keys).

Every public call on `Session` — `query`, `execute`, `prepare_query`,
`prepare_command`, `transaction`, `copy_in`, `close` — translates to a `Command` enum
sent over that channel. Each `Command` carries a `oneshot::Sender`
for its reply. The driver task pulls commands off the inbox, performs
the protocol exchange against the server, and replies on the
`oneshot`.

There is exactly one task per connection. The `mpsc` channel is the
single point of serialization for everything that talks to that
socket.

## Why a task

Postgres' wire protocol is asynchronous in the *responses-arrive-as-
they-arrive* sense, but it is rigorously serial in the *one
request/response sequence at a time per connection* sense. You cannot
interleave two `Bind`/`Execute`/`Sync` cycles on the same socket —
the server's responses are in order and any client that pipelines them
must consume the responses in order too.

If the public API directly wrote and read on the socket, every public
call would need to lock against every other public call, and Tokio
cancellation would tear half-finished protocol exchanges apart.
Instead, babar puts the protocol state machine inside the task, and
the public API becomes "send a `Command`, await the reply." The cost
of an extra `mpsc` hop buys two large benefits: cancellation safety and concurrency on a single connection.

## Cancellation safety

If you `tokio::select!` on `session.execute(&cmd, args)` and the other
branch wins, the future you abandon is just a `oneshot::Receiver`
being dropped. The driver task notices the receiver is gone *only after
it finishes the in-flight `Execute`/`Sync` cycle* — it never abandons
the protocol mid-message. The next command waiting in the `mpsc`
inbox runs after a clean protocol boundary.

That's what we mean when we say every public call in babar is
cancellation-safe. You don't need to hold the future to its end.

## Concurrency on one connection

You can spawn many tasks all calling into the same `Session`. They
all hit the same `mpsc` channel; the driver task processes them in
arrival order. Throughput is bounded by the connection, not by an
arbitrary lock policy. Pipelining multiple short queries against one
session is reasonable; if you need true concurrency, that's what the
[`Pool`](../book/06-pooling.md) is for.

## What lives on the task

The driver task owns:

- The `TcpStream` halves and an `oneshot` per pending request.
- The framing buffer (writes to `tx_buf`, reads chunked frames).
- Parameter status updates as the server announces them.
- The internal prepared-statement cache.

It explicitly does *not* own:

- User-level types like `Query<P, R>` — those live in your code.
- The `Pool`, which is a layer above sessions.
- Codec implementations — codecs run on the calling task; the
  driver task only deals in `Vec<Option<Bytes>>` columns.

## Shutdown

`Session::close()` sends a `Close` command, waits for the
acknowledgement, and joins the task. Dropping a `Session` without
calling `close()` causes the `mpsc::Sender` to be dropped; the driver
task notices, sends `Terminate`, and exits cleanly. There is no
detached task that outlives the `Session` value.

## Why not async fn directly on the socket?

Two reasons.

First, *cancellation correctness*. If `Session::execute` were a plain
`async fn` writing and reading on the socket, abandoning that future
mid-`Execute` would leave the connection desynchronized — half a
message sent, no `Sync` paired, the server still responding to the
last frame. There is no clean way to recover from that without
closing the connection. The driver-task model means the future is
*just* a `oneshot::Receiver`, and abandoning it does not endanger
anything.

Second, *single-writer guarantees*. Postgres' protocol benefits from
write coalescing (a `Parse`/`Bind`/`Execute`/`Sync` is one
`writev` of small frames). With one task owning the writer, that
coalescing is trivial; with many tasks, it requires either locks or
a lock-free SPSC ring per worker — and at that point you've
re-invented the driver task with extra steps.

## Where to read next

- [Async/await and the driver task mental model](../rust-learning/05-async-await-and-the-driver-task.md)
  — the optional Rust-learning companion.
- [Book Chapter 6 — Pooling](../book/06-pooling.md) — for the layer
  above the driver task.
- [Book Chapter 13 — Observability](../book/13-observability.md) — for
  the spans the driver task emits.
- [Design principles](./design-principles.md) — for why this fits the
  rest of babar's shape.
