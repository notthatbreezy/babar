# 8. Structs, `impl`, and Rust-flavored OOP

Rust has object-oriented *pieces*, but it does not push you toward “everything is
a class.” In `babar` service code, the most useful OOP-flavored ideas are:

- structs that package related state
- `impl` blocks that attach focused behavior to a concrete type
- composition of small pieces instead of inheritance trees

That is enough to read real service code without importing a Java or Python class
model into Rust.

## babar anchor

The API tutorial and Axum example both use the same pattern:

```rust
#[derive(Clone)]
struct AppState {
    pool: Pool,
}

struct Settings {
    api_addr: SocketAddr,
    pg_host: String,
    pg_port: u16,
    pg_user: String,
    pg_password: String,
    pg_database: String,
}

impl Settings {
    fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        /* ... */
        # unimplemented!()
    }

    fn database_config(&self) -> Config {
        /* ... */
        # unimplemented!()
    }
}
```

This is a good Rust OOP anchor because it separates three concerns cleanly:

- `AppState` stores long-lived shared application state
- `Settings` stores configuration data
- `impl Settings` defines behavior that is specifically about `Settings`

## Structs package state; they do not hide it

In Rust, a struct is usually the answer to “which values should travel together?”

For the web-service path, that means:

- `AppState { pool }` because every handler needs access to the pool
- request/response structs such as `CreateWidget` and `Widget` because they define
  the shape of JSON at the HTTP boundary
- `Settings` because the connection and server configuration belong together

That is object-oriented in the sense that data has named structure. But it is not
class-heavy: the fields stay visible, ownership is still explicit, and behavior
can live either in an `impl` block or in free functions.

## What belongs in an `impl` block?

Use an `impl` block when the behavior is naturally about one type's job.

`Settings::from_env()` is a good example:

- it constructs a `Settings`
- it keeps environment parsing logic near the type it creates
- it gives callers a clear entry point: “ask `Settings` to build itself”

`Settings::database_config(&self)` is also a good method:

- it reads the fields already owned by `Settings`
- it produces another value derived from those fields
- the behavior is coherent even outside the rest of the app

That is the practical Rust rule: put methods where the type gives the behavior a
clear home.

## What should stay a free function?

Many operations in `babar` examples are clearer as free functions:

```rust
async fn create_widget(
    State(state): State<AppState>,
    Json(payload): Json<CreateWidget>,
) -> Result<(StatusCode, Json<Widget>), (StatusCode, String)> {
    /* ... */
}

async fn get_widget(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Widget>, (StatusCode, String)> {
    /* ... */
}
```

Why not make these methods on `AppState`?

- Axum wants handler functions with a specific extractor-driven shape
- the handler is about an HTTP route, not about `AppState` alone
- keeping it as a free function makes the boundary explicit: inputs come from the
  router and the request, not from a hidden receiver object

This is one of Rust's most important OOP lessons: methods are useful, but they
are not mandatory. If a free function is clearer, prefer the free function.

## Composition over inheritance in babar-style code

The `babar` docs lean on composition constantly:

- `AppState` contains a `Pool`
- handlers acquire a connection from that pool
- query values and command values are composed into the handler logic
- JSON types, SQL types, and configuration types stay separate

Nothing here needs a base `DatabaseService` class or a `WidgetController`
hierarchy. The pieces are combined because they work together, not because they
inherit from one another.

This is especially visible in the route setup:

```rust
let app = Router::new()
    .route("/healthz", get(healthz))
    .route("/widgets", get(list_widgets).post(create_widget))
    .route("/widgets/:id", get(get_widget))
    .with_state(AppState { pool });
```

`Router`, `AppState`, handlers, and `Pool` each do one job. The application is
built by wiring them together.

## Traits are part of Rust's OOP story too

Rust's object-oriented features are not limited to structs and methods. Traits
also matter because they let behavior stay abstract without forcing inheritance.

In this track, you already saw that with codecs:

- a type can implement the traits needed for database encoding/decoding
- the type does not need to inherit from a common database-row base class

So when people say Rust has “object-oriented features,” the useful version is:

- data in structs
- behavior in `impl` blocks
- shared capabilities in traits
- composition as the normal way to build larger systems

## Python comparison (explicitly optional)

If you are coming from Python, the trap is to look for a class every time you see
related data and behavior.

Rust-first correction:

- a `struct` is often just a named data shape
- an `impl` block is for behavior that truly belongs to that shape
- many route handlers and helper operations stay as free functions
- traits cover shared behavior more often than inheritance does

So the closest bridge is not “Rust classes.” It is “Rust lets you use some
object-oriented organization tools, but it keeps them narrower and more explicit.”

## Checkpoint

You are on solid ground if you can identify these three choices in the service
examples:

1. `AppState` is a struct because the pool needs to move through the router as
   one named unit.
2. `impl Settings` exists because configuration-loading behavior belongs to the
   `Settings` type.
3. `create_widget` and `get_widget` stay as free functions because they are route
   handlers, not methods that need a hidden receiver.

## Reflection prompts

- In the service examples, which data shapes are true domain objects, and which
  are just transport or configuration structs?
- If you turned every handler into a method on one giant application type, what
  would become less clear?
- Where does composition already do the job that inheritance might have done in
  another language?

## Read next

- [Iterators, closures, and functional style](09-iterators-closures-and-functional-style.md)
- [Building a web service](../book/11-web-service.md)
- [Postgres API from scratch](../tutorials/postgres-api-from-scratch.md)
