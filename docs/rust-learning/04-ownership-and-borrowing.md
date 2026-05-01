# 4. Ownership and borrowing around queries

Ownership is the first Rust topic that feels genuinely different for many readers. The good news is that you do not need the whole theory to read `babar` examples well. You mainly need to notice who owns a value, who only borrows it, and why a clone sometimes appears right before async work.

## babar anchor

Keep these anchors nearby:

- `crates/core/examples/quickstart.rs`
- [Your first query](../getting-started/first-query.md)
- [1. Connecting](../book/01-connecting.md)

## Start with the most important distinction: owner versus borrower

In quickstart, `main` creates a `Session` and then passes borrowed access into `run`:

```rust
let session = match Session::connect(cfg).await {
    Ok(s) => s,
    Err(e) => {
        eprintln!("connect failed: {e}");
        return ExitCode::from(1);
    }
};

if let Err(e) = run(&session).await {
    eprintln!("workflow failed: {e}");
    let _ = session.close().await;
    return ExitCode::from(1);
}
```

`main` owns the `session` binding. `run(&session)` only borrows it.

That is a common `babar` pattern:

- one part of the program owns the connection handle
- helper functions borrow `&Session` so they can use it without taking it away

## Borrowing is why so many method calls take `&...`

Inside `run`, the code calls methods like this:

```rust
let n = session.execute(&insert, row.clone()).await?;
let active_rows = session.query(&select, (true,)).await?;
```

Two kinds of borrowing are visible:

- `session` is `&Session`, so the function uses the handle without owning it
- `&insert` and `&select` borrow the statement values instead of consuming them

That makes the statement values reusable. You can execute the same `Command` multiple times because the call borrows the command definition and consumes only the argument value.

## Why `row.clone()` appears in quickstart

This is the first ownership line that often looks mysterious:

```rust
for row in &rows {
    let n = session.execute(&insert, row.clone()).await?;
    println!("inserted {n} row(s) for id={}", row.0);
}
```

The array `rows` owns each tuple. The loop iterates over `&rows`, so `row` is only a borrowed reference to one tuple.

But `execute` needs an owned argument value of type:

```rust
(i32, String, bool, Option<String>)
```

Because the tuple contains owned data such as `String`, the borrowed `row` cannot just be moved out of the array. `row.clone()` creates a fresh owned tuple to send into the command.

That is not a random Rust ritual. It is the code making ownership explicit.

## Borrow when reusing, move when handing work off

A practical reading rule for early `babar` code is:

- borrowed values (`&session`, `&insert`, `&rows`) are being reused or inspected
- moved values (`NewUser { ... }`, `(true,)`, or `row.clone()`) are being handed to a call that takes ownership of the argument

That rule is not the full language, but it is a very strong start.

## The `Session` handle owns less than you might think

[Chapter 1: Connecting](../book/01-connecting.md) explains an important subtlety: the `Session` value you hold is a handle, while the actual socket lives in a background driver task.

For this chapter, the practical takeaway is:

- your binding owns the handle value
- borrowed `&Session` references let other functions use that handle
- the deeper connection machinery is still controlled in one place

That design is why `babar` can let many tasks share one session handle without pretending the underlying socket has many owners.

## What to ask whenever a value crosses `.await`

You do not need lifetime jargon yet. Ask this instead:

1. Is this call borrowing the value or taking ownership of it?
2. If the call needs ownership, am I done using the original value?
3. If I still need it later, should I borrow it or clone it first?

Those three questions explain a lot of the surface shape of `babar` examples.

## Python comparison (optional)

A Python reader already knows that names point at objects. Rust is not unique because it has references. Rust feels different because the rules about aliasing, mutation, and handoff across function boundaries are made visible in the code instead of being mostly runtime conventions.

That extra visibility is exactly why the examples show `&Session`, `&insert`, and `row.clone()` instead of silently doing whatever seems convenient.

## Checkpoint

From quickstart, classify each of these as mostly **borrow**, **move**, or **clone to create a new owned value**:

- `run(&session)`
- `session.query(&select, (true,))`
- `session.execute(&insert, row.clone())`
- `for row in &rows`

If you can do that, you are already reading ownership well enough for the opening docs.

## Reflection prompts

- In a `babar` example, which value owns the database connection handle and which functions only borrow access to it?
- Why might code clone a `String`-carrying value before sending it into database work instead of reusing the original binding directly?
- What ownership question should you ask any time a value crosses an `.await` boundary?

## Read next

- [Async/await and the driver task mental model](05-async-await-and-the-driver-task.md)
- [1. Connecting](../book/01-connecting.md)
