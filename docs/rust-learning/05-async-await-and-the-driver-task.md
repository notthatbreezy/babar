# 5. Async/await and the driver task mental model

This chapter is an explanation-first stop in the learning track. Its job is not
to teach every Tokio API. Its job is to make `babar`'s async code readable: when
you see `Session::connect(cfg).await?`, `pool.acquire().await?`, or
`conn.query(&select, args).await?`, you should know what kind of waiting is
happening and why the connection still stays in a valid state.

## babar anchor

Start with these product docs:

- [1. Connecting](../book/01-connecting.md)
- [11. Building a web service](../book/11-web-service.md)
- [The background driver task](../explanation/driver-task.md)
- [Postgres API from scratch](../tutorials/postgres-api-from-scratch.md)

Those pages show the same shape at three scales:

1. one `Session` connecting to Postgres
2. one background task owning that connection
3. one service using a `Pool` so many requests can await database work safely

## The shortest useful async model

In Rust, an `async fn` does not run immediately. Calling it creates a **future**:
a value that describes work which can make progress later. The work actually
advances only when an async runtime such as Tokio polls that future.

That is why the docs keep pairing async functions with `.await`:

```rust
let session = Session::connect(cfg).await?;
let pool = Pool::new(cfg, PoolConfig::new().max_size(8)).await?;
let conn = pool.acquire().await?;
let rows = conn.query(&select, (id,)).await?;
```

Each `.await` marks a point where the current function may pause because it
needs outside progress:

- Postgres must answer the startup handshake
- the pool must hand out a live connection
- the server must execute the SQL and send rows back

The important mental model is simple: **async is how Rust represents waiting for
I/O without blocking the whole thread**.

## What `#[tokio::main]` is doing for the examples

The examples in [1. Connecting](../book/01-connecting.md),
[11. Building a web service](../book/11-web-service.md), and
[Postgres API from scratch](../tutorials/postgres-api-from-scratch.md) all use a
Tokio entry point:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // ...
}
```

That attribute creates the runtime that polls futures for you. Without it, the
compiler would accept neither the async `main` nor the `.await` calls inside it.

You do not need runtime internals to read `babar` docs. You do need one rule:
if the code is waiting on network work, it will usually be async.

## Why `Session::connect` returns a handle instead of exposing the socket

[1. Connecting](../book/01-connecting.md) says that `Session::connect` gives you
a `Session`, but the real socket ownership moves into a background Tokio task.
That design becomes clearer once you separate **handle** from **owner**:

- `Session` is the handle your code clones, passes around, and calls methods on
- the driver task is the owner that reads and writes the Postgres socket

From the outside, a `Session` call feels like:

1. package a request
2. send it to the driver task
3. await the reply

From the inside, the driver task keeps one serial conversation with Postgres.
That matters because one connection cannot safely interleave multiple
request/response exchanges at random.

## Why the driver task exists

The deeper explanation in [The background driver task](../explanation/driver-task.md)
gives two main reasons, and both are practical.

### 1. Cancellation stays safe

If a future waiting on a database call gets dropped, `babar` does not abandon
the Postgres protocol halfway through a message. The driver task finishes the
in-flight exchange on the socket, then moves to the next clean boundary.

That is what the docs mean by **cancellation-safe**. The future you hold is not
the socket itself. It is a request waiting for the driver task's answer.

### 2. One connection still supports concurrent callers

Multiple Tokio tasks can all call into the same `Session`. They are not all
writing to the socket directly. They send commands through the driver's channel,
and the driver processes them in arrival order.

That is a useful distinction:

- **concurrent callers**: many tasks may submit work
- **serial wire protocol**: one connection still speaks to Postgres in order

So `Session` gives you safe sharing of one connection handle, while
[Pool](../book/06-pooling.md) gives you true parallelism across multiple
connections.

## Reading the application flow in the web-service example

The web-service chapter shows the async story at application level:

```rust
async fn create_widget(
    State(state): State<AppState>,
    Json(payload): Json<CreateWidget>,
) -> Result<(StatusCode, Json<Widget>), (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_http)?;
    conn.execute(&insert, (payload.id, payload.name.clone()))
        .await
        .map_err(db_http)?;
    Ok((StatusCode::CREATED, Json(Widget { id: payload.id, name: payload.name })))
}
```

Read it in this order:

1. the handler is async because both HTTP work and database work may wait
2. `pool.acquire().await` may pause until a connection is available
3. `conn.execute(...).await` may pause until Postgres finishes the command
4. while this handler is waiting, Tokio can run other tasks

The point is not “async syntax looks modern”. The point is that one service can
keep handling network-bound work without dedicating one blocked OS thread to
each waiting request.

## `Session` versus `Pool` in one sentence each

- Use **`Session`** when you want one connection and want to understand the
  driver-task model directly.
- Use **`Pool`** when your application may have many overlapping requests and
  should borrow a connection per operation or per handler.

The learning progression across the docs is deliberate:

- [1. Connecting](../book/01-connecting.md) teaches one connection
- [The background driver task](../explanation/driver-task.md) explains why that
  connection is modelled as a handle plus task
- [11. Building a web service](../book/11-web-service.md) shows why real
  services usually step up to a pool

## Python comparison (optional)

If you know Python's `async def`, the surface shape will look familiar. The
Rust-first difference is that Rust futures are ordinary values with strict
ownership rules. They do nothing until a runtime polls them, and the compiler
still checks which values may cross an `.await` point safely.

## Checkpoint

Before moving on, make sure you can answer these without looking back:

- Which lines in the connecting and web-service examples are waiting on network
  progress rather than doing plain CPU work?
- Why is dropping an awaited database future not the same as abandoning the
  socket protocol halfway through?
- When would you reach for a `Pool` instead of sharing one `Session`?

## Read next

- [Error handling and service boundaries](06-error-handling-and-service-boundaries.md)
- [1. Connecting](../book/01-connecting.md)
- [11. Building a web service](../book/11-web-service.md)
- [The background driver task](../explanation/driver-task.md)
