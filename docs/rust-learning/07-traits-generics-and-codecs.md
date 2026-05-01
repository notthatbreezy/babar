# 7. Traits, generics, and codecs

Once you move past the first `session.query(&query, args).await?`, `babar` starts
showing you more of Rust's real shape: generic types, trait-based capabilities,
and codecs that connect Rust values to Postgres values. This chapter explains
those ideas from the `babar` side first, then names the Rust concepts underneath.

## babar anchor

Start with the standard typed query shape from
[`2. Selecting`](../book/02-selecting.md):

```rust
#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct ActiveUsers {
    active: bool,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserSummary {
    id: i32,
    name: String,
    active: bool,
}

let active_users: Query<ActiveUsers, UserSummary> = app_schema::query!(
    SELECT users.id, users.name, users.active
    FROM users
    WHERE users.active = $active
    ORDER BY users.id
);

let rows: Vec<UserSummary> = session
    .query(&active_users, ActiveUsers { active: true })
    .await?;
```

That single type, `Query<ActiveUsers, UserSummary>`, already tells you a lot:

- `ActiveUsers` is the parameter shape the query accepts
- `UserSummary` is the row shape the query produces
- `session.query` turns one into the other by way of Postgres

Rust uses **generics** here because `babar` wants one query API that works for
many parameter and row shapes without erasing the types.

## Generics: one query type, many data shapes

In `babar`, a query is not “just a SQL string.” It is a value whose type records
the Rust shapes on both sides of the round-trip.

From the reader's point of view:

- `Command<Params>` means “a command that accepts this parameter shape”
- `Query<Params, Row>` means “a query that accepts this parameter shape and
  decodes rows into this row shape”

The quickstart example shows the same pattern with tuples instead of structs:

```rust
let insert: Command<(i32, String, bool, Option<String>)> =
    quickstart_schema::command!(
        INSERT INTO quickstart (id, name, active, note)
        VALUES ($id, $name, $active, $note)
    );

let select: Query<(bool,), (i32, String, bool, Option<String>)> =
    quickstart_schema::query!(
        SELECT quickstart.id, quickstart.name, quickstart.active, quickstart.note
        FROM quickstart
        WHERE quickstart.active = $active
        ORDER BY quickstart.id
    );
```

That is still the same idea:

- the command is generic over one parameter type
- the query is generic over a parameter type and a row type
- tuples and structs are both valid choices if their codec support exists

Use structs when names make the code easier to read. Use tuples when the shape is
small and positional. `babar` supports both because the generic API only cares
about the *capability* to encode or decode the shape.

## Traits: capability contracts, not inheritance trees

Rust traits answer a specific question: **what can this type do?**

For `babar`, the key capabilities are encoding and decoding database values. The
custom codec chapter shows that directly:

```rust
impl Encoder<Uuid> for UuidCodec {
    fn encode(&self, value: &Uuid, params: &mut Vec<Option<Vec<u8>>>) -> babar::Result<()> {
        params.push(Some(value.as_bytes().to_vec()));
        Ok(())
    }
}

impl Decoder<Uuid> for UuidCodec {
    fn decode(&self, columns: &[Option<bytes::Bytes>]) -> babar::Result<Uuid> {
        /* ... */
        # unimplemented!()
    }
}
```

Read those impls as:

- `UuidCodec` knows how to **encode** a `Uuid` into Postgres parameters
- `UuidCodec` knows how to **decode** a `Uuid` from Postgres columns

That is the Rust trait mental model that matters here. `UuidCodec` is not
becoming a subtype of anything. It is declaring that it satisfies a capability
contract.

This is why traits fit `babar` so naturally:

- a query needs something that can encode parameters
- a query needs something that can decode rows
- the concrete Rust value can vary, as long as the required trait contract exists

## Why `#[derive(babar::Codec)]` matters

Most application code should not implement `Encoder` and `Decoder` by hand. It
should derive `babar::Codec` on normal Rust structs:

```rust
#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct DemoUser {
    id: i32,
    name: String,
}
```

That derive is important because it makes an ordinary Rust type usable at the SQL
boundary:

- as `Command<DemoUser>` input
- as `Query<_, DemoUser>` output
- as part of larger typed query/command contracts

In other words, a codec is not a side feature bolted on after the fact. It is
the mechanism that lets `babar` keep SQL parameters and rows fully typed.

## Derive first, manual codecs second

Use this rule of thumb in `babar` code:

1. **Derive `babar::Codec`** for normal row-shaped or parameter-shaped structs
2. **Use tuples** for tiny positional shapes when names would add noise
3. **Write `Encoder` / `Decoder` impls manually** only when you need a type that
   `babar` does not already know how to map

That is the progression you see across the docs:

- [`2. Selecting`](../book/02-selecting.md) uses derived structs for application rows
- [`10. Custom codecs`](../book/10-custom-codecs.md) shows the lower-level trait
  implementation when the built-in mapping is not enough

## Where generics stay helpful in service code

The web-service docs keep using the same generic idea even when the surrounding
code gets more realistic:

```rust
let insert: Command<(i32, String)> =
    service_schema::command!(INSERT INTO widgets (id, name) VALUES ($id, $name));

let select: Query<(i32,), (i32, String)> = service_schema::query!(
    SELECT widgets.id, widgets.name
    FROM widgets
    WHERE widgets.id = $widget_id
);
```

The handler code does not need to re-explain SQL decoding each time, because the
generic type already says what shape goes in and what shape comes out.

That is the real win: the generic API is compact at the call site, but it keeps
the data contract visible.

## Python comparison (explicitly optional)

If you are coming from Python, keep the comparison narrow:

- **Traits are not base classes.** They are closer to explicit capability
  contracts than to inheritance.
- **Generics are not duck typing.** `Query<Params, Row>` states the expected
  shapes up front instead of waiting until runtime.
- **Derived codecs are not hidden serializers.** In `babar`, they are part of the
  type-level contract between Rust and Postgres.

Useful bridge sentence: Python often asks “does this object behave correctly at
runtime?” Rust often asks “have we made the required behavior explicit in the
type system?”

## Checkpoint

If this chapter clicked, you should be able to explain each of these without
running code:

1. In `Query<Params, Row>`, `Params` describes the bound input shape and `Row`
   describes the decoded output shape.
2. `#[derive(babar::Codec)]` makes a normal struct usable at the SQL boundary.
3. `Encoder<A>` and `Decoder<A>` are trait-based capability contracts, not an
   inheritance hierarchy.

## Reflection prompts

- When would a named struct be clearer than a tuple for a `Command<Params>` or
  `Query<Params, Row>`?
- Why is “this type can be encoded/decoded” a better mental model than “this type
  belongs to a class hierarchy” for `babar`?
- If you had to support a new Postgres type tomorrow, would you reach for
  `#[derive(babar::Codec)]` first or a manual trait impl first, and why?

## Read next

- [Structs, `impl`, and Rust-flavored OOP](08-structs-impls-and-rust-oop.md)
- [10. Custom codecs](../book/10-custom-codecs.md)
- [The typed-SQL macro pipeline](../explanation/typed-sql-macro-pipeline.md)
