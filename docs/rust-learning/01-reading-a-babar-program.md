# 1. Read a babar program

The first goal is not to understand every Rust rule. It is to look at a small `babar` example and say which parts are data shapes, which parts do networked work, and which details can wait until a second pass.

## babar anchor

Keep these open while reading:

- [Your first query](../getting-started/first-query.md)
- `crates/core/examples/quickstart.rs`

The examples are small enough to read end to end, but real enough to show the shapes that keep appearing across the rest of the docs.

## Start by sorting the file into four kinds of lines

When you open the first-query example, do not read it top to bottom as one undifferentiated block. Sort the lines into four jobs.

### 1. Data-shape lines

These lines describe Rust values that will cross the database boundary:

```rust
#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct NewUser {
    id: i32,
    name: String,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserRow {
    id: i32,
    name: String,
}
```

At first read, the most important fact is simple: `NewUser` is the value shape sent into SQL, and `UserRow` is the value shape read back out. You do **not** need to understand every derive right away. It is enough to notice that these structs make the database boundary explicit.

### 2. Schema and statement lines

These lines define reusable SQL-facing values:

```rust
babar::schema! {
    mod app_schema {
        table demo_users {
            id: primary_key(int4),
            name: text,
        }
    }
}

let insert: Command<NewUser> =
    app_schema::command!(INSERT INTO demo_users (id, name) VALUES ($id, $name));

let users: Query<(), UserRow> = app_schema::query!(
    SELECT demo_users.id, demo_users.name
    FROM demo_users
    ORDER BY demo_users.id
);
```

You can read this as: authored schema facts live in one place, then typed statement values are built from them.

- `Command<NewUser>` means “run SQL with a `NewUser` value; no rows come back.”
- `Query<(), UserRow>` means “run SQL with no input parameters; each row decodes into `UserRow`.”

That is already enough to follow the high-level flow.

### 3. Networked work

These are the lines that actually talk to Postgres and therefore return futures or results:

```rust
let session: Session = Session::connect(cfg).await?;
session.execute(&create, ()).await?;
session.execute(&insert, NewUser { id: 1, name: "Ada".to_string() }).await?;
let rows: Vec<UserRow> = session.query(&users, ()).await?;
session.close().await?;
```

A good first-pass rule is: lines with `.await` are the I/O boundary unless you learn otherwise.

### 4. Local Rust control flow

Everything else is mostly local orchestration:

```rust
for row in &rows {
    println!("id={} name={}", row.id, row.name);
}
```

That loop is not a database concept. It is ordinary Rust code handling values that already came back from the query.

## A useful first-pass reading strategy

For the opening sequence, read each `babar` example in this order:

1. Find the structs and tuple types.
2. Find the `Query<_, _>` and `Command<_>` values.
3. Find the `.await` calls.
4. Only then read the helper details around them.

That order keeps you from getting stuck on syntax that matters less than the overall shape.

## What you can safely defer on day one

A new Rust reader often tries to solve too many puzzles at once. In these opening `babar` examples, you can safely defer all of this until later:

- the exact macro-expansion details of `schema!`, `query!`, and `command!`
- the full meaning of every derive attribute
- the internals of Tokio and the driver task

You still need to notice that those features exist. You just do not need to master them before you can read the happy path.

## Python comparison (optional)

If you come from Python, the quickest bridge is this: the file still has recognizable program structure — imports, data shapes, function boundaries, setup, I/O, and output — but Rust makes more of that structure explicit in types and function signatures.

Use that comparison to get oriented, then come back to the Rust names. The point of this track is to become fluent in the Rust-shaped version of the program, not to translate every line back into Python.

## Checkpoint

If you can answer these four prompts from the first-query example, you are reading the file the right way:

- Which structs describe values crossing the SQL boundary?
- Which statement returns rows, and which one does not?
- Which lines must wait on network progress?
- Which block only formats already-decoded Rust values?

## Reflection prompts

- In the first `babar` example, which lines define data shapes and which lines perform networked work?
- Where does the example return or propagate a `Result`, and what does that tell you about failure boundaries?
- Which two details can you safely defer until later so you can keep reading the example end to end?

## Read next

- [Syntax and control flow in context](02-syntax-and-control-flow.md)
- [Your first query](../getting-started/first-query.md)
