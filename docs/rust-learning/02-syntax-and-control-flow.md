# 2. Syntax and control flow in context

This chapter introduces Rust syntax only in the forms that show up immediately in `babar`: bindings, blocks, method chains, `match`, `if let`, `for`, and the small tuples or structs passed into queries.

## babar anchor

Use these anchors together:

- `crates/core/examples/quickstart.rs`
- [1. Connecting](../book/01-connecting.md)
- [2. Selecting](../book/02-selecting.md)

The goal is not to memorize the whole language. It is to make the common `babar` surface readable.

## `let` means “name a value”

In the quickstart example, most bindings look like this:

```rust
let host = std::env::var("PGHOST").unwrap_or_else(|_| "localhost".into());
let cfg = Config::new(&host, port, &user, &database)
    .password(password)
    .application_name("babar-quickstart");
```

Two early habits matter here:

- `let` creates a binding.
- bindings are immutable unless you write `let mut`.

That default fits many `babar` examples because connection config, statement values, and decoded rows are usually created once and then read, not repeatedly mutated.

## Method chains read top to bottom

`Config::new(...).password(...).application_name(...)` is ordinary Rust method chaining. Read it as “start with a `Config`, then refine it step by step.”

That pattern appears throughout `babar` because builder-style setup is clearer than hiding connection details inside one opaque string.

## Blocks use braces, and many control-flow forms are expressions

Rust uses braces for blocks, but the deeper idea is that blocks often produce values. You can see that in the quickstart connection handling:

```rust
let session = match Session::connect(cfg).await {
    Ok(s) => s,
    Err(e) => {
        eprintln!("connect failed: {e}");
        return ExitCode::from(1);
    }
};
```

`match` is not just branching syntax. It is an expression whose branches must line up to produce a sensible overall result.

In this case:

- the `Ok(s)` branch yields the `Session`
- the `Err(e)` branch logs and returns early from `main`

That is why the whole `match` can sit on the right-hand side of `let session = ...`.

## `if let` handles one pattern without writing a full `match`

A few lines later, quickstart uses a narrower form:

```rust
if let Err(e) = run(&session).await {
    eprintln!("workflow failed: {e}");
    let _ = session.close().await;
    return ExitCode::from(1);
}
```

Read `if let` as “if this value matches one pattern I care about, run this block.”

Here the code only needs custom handling for the error case. The success case does nothing special, so a full `match` would be noisier.

## `for` loops usually iterate by borrowing first

The query-processing loop in quickstart is:

```rust
for row in &rows {
    let n = session.execute(&insert, row.clone()).await?;
    println!("inserted {n} row(s) for id={}", row.0);
}
```

The syntactic detail to notice is `&rows`.

That means “iterate over borrowed references to each element” instead of moving the array itself. The ownership reason for `row.clone()` comes in the next chapter; for now, the syntax takeaway is that Rust makes the borrowed-versus-moved choice visible.

## Tuples can be compact, but field names disappear

Quickstart uses a tuple type for the insert command:

```rust
let insert: Command<(i32, String, core::primitive::bool, Option<String>)> = ...;
```

Tuple syntax is compact and useful for local examples. But tuple fields are positional (`row.0`, `row.1`, and so on), which is why longer-lived docs examples often prefer named structs such as `NewUser` or `UserSummary`.

## A tiny syntax map for the opening docs

When you see these forms in `babar`, read them like this:

- `fn name(...) -> T` — a function returning `T`
- `async fn name(...) -> T` — a function whose body contains async work
- `Type::name(...)` — an associated function or constructor-style call
- `value.method(...)` — a method call on a value
- `Enum::Variant(...)` or `Ok(...)` / `Err(...)` — an enum variant being constructed or matched
- `?` — if this result is an error, return early from the current function

That last item matters enough to get its own chapter later. For now, just recognize it as control flow.

## Python comparison (optional)

The closest Python bridge is that the control-flow ideas are familiar — branch, loop, early return, configure an object through calls — but Rust puts more meaning into expressions and types than indentation alone can carry.

Use the bridge to orient yourself. Then keep naming the Rust construct you are seeing: binding, method chain, `match`, borrowed iteration, tuple field.

## Checkpoint

Try reading this line by line without translating it into English prose first:

```rust
let rows: Vec<UserSummary> = session
    .query(&active_users, ActiveUsers { active: true })
    .await?;
```

You should be able to say:

- what binding is being created
- which method is being called
- which argument is a named struct literal
- where async waiting happens
- where early-return-on-error can happen

## Reflection prompts

- Which control-flow forms in the quickstart example are there to handle success versus failure?
- Why is an immutable `let` binding usually the default in these `babar` examples?
- What information is carried by a chained builder call that would often be hidden in Python keyword arguments or dynamic configuration objects?

## Read next

- [Types, structs, and `Result`](03-types-structs-and-results.md)
- [2. Selecting](../book/02-selecting.md)
