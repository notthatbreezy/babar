# 3. Types, structs, and `Result`

Now that the syntax is less intimidating, the next step is learning to name the value shapes. In `babar`, that mostly means understanding structs, tuples, `Option<T>`, `Query<A, B>`, `Command<A>`, and `Result<T, E>`.

## babar anchor

This chapter stays close to:

- [Your first query](../getting-started/first-query.md)
- [2. Selecting](../book/02-selecting.md)
- `crates/core/examples/quickstart.rs`

## `Query<A, B>` and `Command<A>` describe the boundary

The most important `babar` types are worth reading literally:

- `Command<A>` — send an `A` into SQL; no result rows come back
- `Query<A, B>` — send an `A` into SQL; each returned row decodes into `B`

From the selecting chapter:

```rust
let active_users: Query<ActiveUsers, UserSummary> = app_schema::query!(
    SELECT users.id, users.name, users.active
    FROM users
    WHERE users.active = $active
    ORDER BY users.id
);
```

You can read this as a contract:

- call the query with an `ActiveUsers` value
- get back `Vec<UserSummary>`

That is the main reason the docs keep showing the type names. They are not decoration; they describe the database round-trip.

## Named structs are the clearest default

The getting-started guide uses one named struct for both the insert and row shape
because the field sets are identical:

```rust
#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct User {
    id: i32,
    name: String,
}
```

This is usually the best first choice in docs because field names carry meaning at the call site.

```rust
session.execute(
    &insert,
    User {
        id: 1,
        name: "Ada".to_string(),
    },
).await?;
```

A reader can see immediately what each value means.

## Tuples are fine when the shape is small and local

Quickstart shows the same idea with tuples:

```rust
let insert: Command<(i32, String, core::primitive::bool, Option<String>)> = ...;
```

That is still a real type, just a positional one.

Use the docs' examples as a rough guide:

- prefer **structs** when the fields have business meaning you will keep talking about
- prefer **tuples** when the shape is small, local, and obvious from nearby SQL

That is why the book often starts with structs, even when a tuple would compile.

## `Option<T>` means “this may be absent”

In the selecting chapter, a nullable column becomes an optional Rust field:

```rust
struct UserNote {
    id: i32,
    note: Option<String>,
}
```

`Option<String>` does not mean “string with a special empty value.” It means one of two cases is present in the type:

- `Some(String)`
- `None`

That is a perfect fit for SQL nullability because the possibility of absence stays visible in the Rust type.

## `Result<T, E>` means success or failure is part of the API

The first-query guide uses this signature:

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
```

Read it in pieces:

- `main` is async
- if it succeeds, it returns `()`
- if it fails, it returns a `babar::Error` through the alias `babar::Result<()>`

`()` is Rust's unit type: “there is no interesting success payload here.” That makes sense for a function whose job is to perform side effects such as connect, execute SQL, print rows, and shut down cleanly.

## The first useful reading of `?`

You do not need the full error-handling chapter yet to read this:

```rust
let rows: Vec<UserRow> = session.query(&users, ()).await?;
```

At the opening-sequence level, `?` means:

1. wait for the query result
2. if it is an error, stop this function and return that error upward
3. if it is a success, unwrap the success value and keep going

That reading is enough to follow almost every early `babar` example.

## Why `derive(babar::Codec)` keeps appearing

For this chapter, you only need the short version: the derive tells `babar` how to encode or decode that Rust shape at the database boundary.

You do **not** need to know the trait machinery yet. Just notice that structs used with `Query` and `Command` usually carry the derive because they are part of the SQL contract.

## Python comparison (optional)

A Rust struct can feel superficially like a Python `dataclass`, but the important difference is that the field types, optionality, cloning behavior, and trait derivations are part of the core contract, not light metadata layered on afterward.

That is why the docs keep treating these definitions as central design choices.

## Checkpoint

Before moving on, make sure you can classify each of these without hesitation:

- `NewUser` — named struct used as a parameter shape
- `UserRow` — named struct used as a row shape
- `Option<String>` — a field that may be absent
- `Query<ActiveUsers, UserSummary>` — typed read statement
- `babar::Result<()>` — fallible return type with no success payload beyond “it worked”

## Reflection prompts

- In the current `babar` examples, where would a named struct help more than a tuple, and why?
- What does `Option<String>` communicate about a database column that plain `String` does not?
- When you see `babar::Result<()>`, what work completed successfully if the function returns `Ok(())`?

## Read next

- [Ownership and borrowing around queries](04-ownership-and-borrowing.md)
- [Your first query](../getting-started/first-query.md)
