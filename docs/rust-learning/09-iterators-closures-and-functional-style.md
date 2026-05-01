# 9. Iterators, closures, and functional style

By the time you reach `babar` service code, Rust often stops looking like
statement-by-statement imperative code and starts looking like a pipeline:

- get rows
- transform rows
- collect a result

This chapter explains that style without pretending every loop should become an
iterator chain. In Rust, the best question is not “can I make this look more
functional?” but “which form makes ownership and intent clearest here?”

## babar anchor

The clearest anchor is `list_widgets` from the Axum example:

```rust
let rows = conn
    .query(&select, (params.name, params.limit, params.offset))
    .await
    .map_err(db_error)?;

let widgets = rows
    .into_iter()
    .map(|(id, name)| Widget { id, name })
    .collect();
```

That tiny pipeline contains three core Rust ideas:

1. `into_iter()` consumes the vector of rows
2. `map(...)` transforms each row
3. `collect()` gathers the transformed items into a new collection

## Iterators turn “a collection” into “a sequence of items”

When `session.query` returns `Vec<Row>`, you often have a choice:

- loop over `&rows`
- turn `rows` into an iterator and build something new

The difference matters because the iterator method you pick says something about
ownership.

### Borrowing iteration

The quickstart example uses a plain borrowed loop:

```rust
for (id, name, active, note) in &active_rows {
    let note = note.as_deref().unwrap_or("(none)");
    println!("  id={id} name={name} active={active} note={note}");
}
```

`&active_rows` means:

- keep the vector
- borrow each row for reading
- do not consume the collection

This is perfect when you only need to inspect or print the data.

### Consuming iteration

The service example instead does this:

```rust
let widgets = rows
    .into_iter()
    .map(|(id, name)| Widget { id, name })
    .collect::<Vec<_>>();
```

`into_iter()` means:

- take ownership of `rows`
- move each row out of the vector
- build a new `Vec<Widget>`

That fits because the old row vector is no longer needed after the mapping step.

## Closures are small functions that can capture surrounding values

The `map` call above uses a closure:

```rust
|(id, name)| Widget { id, name }
```

You can read that as “for each row, make a `Widget`.”

A closure is often the shortest way to express a local transformation. In Rust,
the important extra question is: **what does the closure capture from its
surroundings?**

In the `Widget` mapping example, the closure only uses its input tuple, so there
is no interesting capture. But closures *can* capture local values, and when they
do, Rust cares whether they borrow or move those values.

That matters in service code because ownership rules do not disappear just because
the code looks functional.

## Functional style is common in row mapping

Here are the most common patterns you will see around `babar`:

### `map` for shape changes

Turn SQL rows into API structs:

```rust
let widgets = rows
    .into_iter()
    .map(|(id, name)| Widget { id, name })
    .collect::<Vec<_>>();
```

### `next` for one-row lookups

The book shows a compact one-row pattern:

```rust
let user = session
    .query(&user_by_id, UserById { id: 7 })
    .await?
    .into_iter()
    .next();
```

That says: “run the query, turn the vector into an iterator, and take the first
item if one exists.”

The web-service example spells the same idea a little more explicitly:

```rust
let Some((id, name)) = rows.into_iter().next() else {
    return Err((StatusCode::NOT_FOUND, format!("widget {id} not found")));
};
```

### `collect` for concrete output

Iterator adapters stay lazy until you ask for a concrete result. `collect()` is
the point where you decide what collection you actually want, often `Vec<_>`.

That is why the `list_widgets` handler reads naturally as:

1. fetch rows
2. transform rows
3. collect response objects

## When a plain `for` loop is better

Rust does not treat iterator chains as automatically more advanced or more
correct. Use a `for` loop when it is clearer.

The quickstart example is a good model:

```rust
for row in &rows {
    let n = session.execute(&insert, row.clone()).await?;
    println!("inserted {n} row(s) for id={}", row.0);
}
```

That loop is the right choice because each iteration:

- performs an async database call
- has a side effect
- benefits from being read step by step

An iterator chain would hide the control flow more than it would help.

## A practical reading rule

When you hit functional-looking Rust in `babar`, ask two questions:

1. Is this pipeline **borrowing** items or **consuming** them?
2. Is this pipeline clearer than the equivalent `for` loop for this job?

That rule gets you further than memorizing every iterator adapter up front.

## Python comparison (explicitly optional)

If you know Python, some of this may resemble comprehensions, `map`, or generator
pipelines. The important Rust-first differences are:

- `into_iter()` vs `iter()` makes ownership visible
- `collect()` makes the new collection boundary explicit
- closure capture rules matter because values can move, not just be referenced

So the bridge is useful, but incomplete. Rust iterator code is still shaped by
ownership and move semantics in ways Python does not surface.

## Checkpoint

You should now be able to read these patterns in `babar` without treating them as
magic:

1. `rows.into_iter().map(...).collect()` means consume rows, transform them, and
   build a new collection.
2. `rows.into_iter().next()` means consume the vector and take the first item if
   one exists.
3. `for row in &rows` means borrow the collection for inspection or stepwise work
   without consuming it.

## Reflection prompts

- In the `list_widgets` pipeline, what value gets moved, and what new value gets
  built?
- Why is a closure-based `map` a good fit for row-to-JSON transformation but a
  worse fit for the quickstart insert loop?
- When you see `into_iter()` in Rust, what ownership question should you ask
  immediately?

## Read next

- [Learn Rust with babar](index.md)
- [2. Selecting](../book/02-selecting.md)
- [11. Building a web service](../book/11-web-service.md)
