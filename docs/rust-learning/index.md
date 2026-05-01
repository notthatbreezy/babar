# Learn Rust with babar

This optional track is for readers who want to learn Rust by reading real `babar` code and docs examples instead of detouring through toy applications. The opening sequence is aimed at readers who know how to program already and now need a trustworthy way to read Rust in context.

## Opening guided sequence

These first five chapters are meant to be read in order:

1. [Read a babar program](01-reading-a-babar-program.md)
2. [Syntax and control flow in context](02-syntax-and-control-flow.md)
3. [Types, structs, and `Result`](03-types-structs-and-results.md)
4. [Ownership and borrowing around queries](04-ownership-and-borrowing.md)
5. [Async/await and the driver task mental model](05-async-await-and-the-driver-task.md)

Together they answer the first questions most new Rust readers hit when they open `babar` for the first time:

- What parts of this file are data shapes versus I/O?
- How do the statement types describe the database boundary?
- Why do `&`, `clone()`, `Option<T>`, `.await`, and `?` appear so often?
- What async mental model is enough to keep reading without studying runtime internals yet?

## Follow-on chapters

The remaining chapters deepen that foundation without turning this docs site into a general Rust textbook:

6. [Error handling and service boundaries](06-error-handling-and-service-boundaries.md)
7. [Traits, generics, and codecs](07-traits-generics-and-codecs.md)
8. [Structs, `impl`, and Rust-flavored OOP](08-structs-impls-and-rust-oop.md)
9. [Iterators, closures, and functional style](09-iterators-closures-and-functional-style.md)

## How the guided pages are framed

Each chapter in this section follows the same pattern:

1. **babar anchor** — start from a real `babar` example, docs page, or code path.
2. **Rust-first explanation** — explain the Rust concept directly in the context of that anchor.
3. **Python comparison (optional)** — include a clearly labeled comparison only when it closes a real gap for Python-fluent readers.
4. **Checkpoint / reflection** — end with a small self-check so you can tell whether the Rust idea is starting to stick.

## Start here, then branch outward

If you want the shortest path through the opening sequence, read the five chapters above and keep these source anchors open in a second tab:

- [Your first query](../getting-started/first-query.md)
- [1. Connecting](../book/01-connecting.md)
- [2. Selecting](../book/02-selecting.md)
- `crates/core/examples/quickstart.rs`

After that, jump outward only when you need more depth:

- [9. Error handling](../book/09-error-handling.md)
- [The background driver task](../explanation/driver-task.md)
- [Postgres API from scratch](../tutorials/postgres-api-from-scratch.md)
