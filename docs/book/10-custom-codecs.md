# 10. Custom codecs

In this chapter we'll go from "I want to read `widgets.id` as a
`uuid::Uuid`" to a working `Encoder<Uuid>` / `Decoder<Uuid>` pair, and
see when to reach for `#[derive(babar::Codec)]` instead of writing
the traits by hand.

## Setup

```rust
use babar::codec::{Decoder, Encoder};
use babar::types::Type;
use bytes::Bytes;
use uuid::Uuid;

const UUID_OID: u32 = 2950;

struct UuidCodec;

impl Encoder<Uuid> for UuidCodec {                                    // type: impl Encoder<Uuid>
    fn encode(&self, value: &Uuid, params: &mut Vec<Option<Vec<u8>>>) -> babar::Result<()> {
        params.push(Some(value.as_bytes().to_vec()));
        Ok(())
    }

    fn oids(&self) -> &'static [u32] { &[UUID_OID] }
    fn format_codes(&self) -> &'static [i16] { &[1] }                 // binary
}

impl Decoder<Uuid> for UuidCodec {                                    // type: impl Decoder<Uuid>
    fn decode(&self, columns: &[Option<Bytes>]) -> babar::Result<Uuid> {
        let bytes = columns[0]
            .as_ref()
            .ok_or_else(|| babar::Error::Codec("uuid: NULL".into()))?;
        let arr: [u8; 16] = bytes.as_ref().try_into()
            .map_err(|_| babar::Error::Codec("uuid: wrong length".into()))?;
        Ok(Uuid::from_bytes(arr))
    }

    fn n_columns(&self) -> usize { 1 }
    fn oids(&self) -> &'static [u32] { &[UUID_OID] }
    fn format_codes(&self) -> &'static [i16] { &[1] }
}

const UUID: UuidCodec = UuidCodec;
```

## What you have to implement

Both traits are generic over a Rust value type `A`. `Encoder<A>` turns
an `&A` into one or more parameter byte buffers; `Decoder<A>` turns
N column buffers back into an `A`.

The `Encoder<A>` methods (`format_codes` and `types` have sensible
defaults — implement them only when you need to override):

- `encode(&self, value, params)` — push exactly `oids().len()` entries
  onto `params`. `Some(bytes)` for a value, `None` for SQL `NULL`.
- `oids()` — the Postgres OIDs of the parameter slots, in order.
- `format_codes()` — `0` for text format, `1` for binary; defaults to
  text. Use binary for everything you can.
- `types()` — richer type metadata; default implementation derives
  this from `oids()`.

The `Decoder<A>` methods (`format_codes` and `types` again have
defaults you can usually skip):

- `decode(&self, columns)` — consume the first `n_columns()` entries
  of `columns` and produce an `A`.
- `n_columns()` — how many columns this decoder consumes.
- `oids()` — column OIDs, in order. `oids().len() == n_columns()`.
- `format_codes()` — same convention as the encoder.

The driver checks the top-level decoder's `n_columns()` against the
server's `RowDescription` for you; that's how you get
`Error::ColumnAlignment` instead of a panic when shapes don't line
up.

## Use it just like a built-in codec

```rust
use babar::query::Query;

let q: Query<(Uuid,), (Uuid, String)> = Query::raw(
    "SELECT id, name FROM widgets WHERE id = $1",
    (UUID,),
    (UUID, babar::codec::text),
);
```

Codec values compose: the tuple `(UUID, text)` is itself a
`Decoder<(Uuid, String)>`, because `Decoder<A>` is implemented for
tuples whose elements implement `Decoder<_>`.

## When to derive instead

If you have a Postgres composite type or a row-shaped struct, skip
the trait impls entirely and use `#[derive(babar::Codec)]`:

```rust
#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserRow {
    id: i32,
    name: String,
    note: Option<String>,
    #[pg(codec = "varchar")]
    handle: String,
}
```

The derive expands to an `Encoder<UserRow>` / `Decoder<UserRow>` pair
whose column order matches the struct. `#[pg(codec = "...")]` lets
you override the codec per field — useful when the column type is
`varchar` instead of `text`, for example. The generated codec is
exposed as `UserRow::CODEC` and works in `Command::raw`,
`Query::raw`, and `CopyIn::binary` exactly like any other.

The full example lives in `crates/core/examples/derive_codec.rs`.

## Tips you'll want before your first round-trip fails

- **Match the OID exactly**. If your `oids()` says `int4` (23) but the
  column is `int8` (20), the driver returns `Error::SchemaMismatch`
  with both OIDs. Look them up with `SELECT oid, typname FROM
  pg_type WHERE typname = 'uuid'`.
- **Binary first, text only as a last resort**. The binary
  representation is exact; the text representation involves Postgres'
  `IN`/`OUT` functions and locale settings.
- **Handle NULL explicitly**. A NULL column arrives as `None` in
  `columns`. If your type can't be NULL, decode it directly. If it
  can, expose a `nullable(...)` wrapper or use `Option<A>` from your
  caller.
- **`encode` errors are user errors, not panics**. Return
  `Err(Error::Codec(...))` for unrepresentable values rather than
  panicking — the driver propagates it cleanly.

## Next

[Chapter 11: Building a web service](./11-web-service.md) wires a
pool, custom codecs, and `tracing` together inside an Axum service.
