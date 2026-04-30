# The typed-SQL macro pipeline

> See also: [What makes babar babar](./what-makes-babar-babar.md),
> [Design principles](./design-principles.md), and
> [Chapter 3: Parameterized commands](../book/03-parameterized-commands.md).

This page explains how babar's schema-aware typed-SQL macros turn authored schema
facts and SQL tokens into runtime `Query<P, R>` and `Command<P>` values.

It is aimed at readers who want the architecture of the feature, not just the
surface syntax.

## The pipeline at a glance

```text
schema! facts
    ↓
query! / command! input
    ↓
parse supported SQL subset
    ↓
infer parameter + row shapes
    ↓
generate Fragment + codecs
    ↓
emit Query<P, R> or Command<P>
    ↓
optional live verification for supported SELECT statements
```

The important point is that babar does not invent a second runtime model for the
macro path. The macros lower into the same statement values the rest of the API
uses.

## 1. `schema!` defines the facts the macros can rely on

`schema!` is the reusable front door. It records table, column, nullability, and
primary-key facts in Rust syntax and emits schema-scoped wrappers.

```rust
babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
            note: nullable(text),
        }
    }
}
```

Those facts are intentionally authored, explicit, and local to your Rust code.
The macro pipeline does not depend on generated schema files or an offline cache.

## 2. `query!` and `command!` parse a narrow SQL subset

The typed-SQL macros accept one statement at a time. Within that statement they
look for a constrained set of forms:

- `SELECT` projections and predicates for reads
- `INSERT ... VALUES`, `UPDATE ... WHERE`, and `DELETE ... WHERE` for writes
- named placeholders such as `$id`
- explicit optional forms such as `$value?` and `(...)?` in supported positions

The subset stays small on purpose. babar is not trying to accept arbitrary SQL
and partially reinterpret it. When a statement is outside the supported forms,
the expected move is to use `Query::raw`, `Query::raw_with`, `Command::raw`, or
`Command::raw_with`.

## 3. Placeholder names become parameter shapes

Named placeholders are collected, deduplicated, and ordered into the generated
parameter encoder.

That lets the macro infer a single Rust value shape for the statement. If the SQL
uses `$id` and `$name`, the generated statement expects a Rust value that can
encode those fields in that order. In practice that usually means either:

- a struct with matching field names
- a tuple shape that matches the inferred parameter ordering

The docs recommend structs because they make the SQL-to-Rust correspondence
obvious at the call site.

## 4. Projections become row decoders

For `query!`, the selected columns and schema facts are turned into a decoder for
`R`.

That decoder carries:

- the expected column count
- the expected Postgres OIDs in column order
- the decode logic for the Rust row shape

This matters because the macro result is not “typed text.” It is a `Query<P, R>`
value with a generated decoder, ready for prepare-time validation and runtime row
decoding.

## 5. Lowering targets `Fragment`, `Query`, and `Command`

At runtime, babar executes `Fragment<A>`, `Query<A, B>`, and `Command<A>` values.
The macro path lowers into those same types.

Conceptually, the generated code builds:

- SQL text that Postgres can execute
- an encoder for the inferred parameter shape
- a decoder for the inferred row shape, if the statement returns rows
- origin metadata so errors can point back to the macro call site

That shared runtime model is why schema-aware macros and raw builders can coexist
cleanly. They are different authoring surfaces for the same execution layer.

## 6. Live verification is optional and scoped

If `BABAR_DATABASE_URL` or `DATABASE_URL` is set during macro expansion,
supported schema-aware `SELECT` statements can be checked against a live Postgres
server.

That verification confirms, for the supported path:

- schema facts line up with the database
- placeholders have the expected types
- projected columns match the inferred row shape

If the environment variable is absent, the macro still emits the same runtime
statement value. Verification is an extra check, not a different API mode.

## 7. Why babar keeps one compiler for the public typed-SQL surface

The public typed-SQL story is easier to teach and easier to reason about when it
has one pipeline:

- `schema!` provides facts
- `query!` and `command!` consume those facts
- the result is a `Query<P, R>` or `Command<P>`

That keeps the mental model stable across:

- inline schema examples
- schema-scoped wrappers
- optional live verification
- runtime execution and prepare-time checks

The lower-level tools still matter, but they are intentionally separate layers,
not alternate public compilers that need different explanations.

## 8. When to drop below the macro layer

Use the raw and fragment layers when one of these is true:

- the statement is outside the schema-aware subset
- you want explicit codec control
- you need fragment composition with `sql!`
- you are doing bootstrap or infrastructure work where authored schema facts are
  not the right abstraction

That is not a failure case. It is part of the design: the macro layer handles the
common typed-SQL path, and the raw layer remains explicit for everything else.

## Reading the pipeline from the outside in

If you are evaluating babar's architecture, the macro pipeline says three things
about the project:

1. authored schema facts are a first-class input
2. typed SQL lowers into ordinary runtime values instead of a separate execution
   system
3. unsupported statements stay explicit instead of being hidden behind partial
   emulation

Those choices are what let babar keep a greenfield, Postgres-shaped typed-SQL
story without turning the macro surface into an unbounded SQL compiler.
