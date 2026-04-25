//! Tuple codecs: `(C1, C2, ...)` is itself a codec for the tuple of
//! component value types.
//!
//! Implemented via macro for arities 1 through 16. Each instance:
//! - encodes by calling each component's encoder in order, concatenating
//!   the produced parameter slots;
//! - decodes by slicing the column vector at component-`n_columns()`
//!   boundaries and calling each component's decoder in turn.
//!
//! `oids()` returns the concatenated OIDs but as a `'static` slice — so
//! we cache it lazily in a `OnceLock` per arity-shape. Without the
//! cache we'd need to allocate on every call.

use bytes::Bytes;

use super::{Decoder, Encoder};
use crate::error::{Error, Result};
use crate::types::Oid;

/// Concatenate static OID slices into a fresh `&'static [Oid]`.
///
/// `Box::leak` is the simple-and-correct option here: tuple codecs are
/// stateless unit-struct values whose `oids()` is called at query prepare
/// time, not per row. A `static OnceLock` inside this generic function
/// would *not* be monomorphized per type instantiation — every tuple
/// arity would share one cell — so we cannot use one without keying on
/// the actual codec types. Per-prepare leak is the pragmatic trade.
fn concat_static_oids(parts: &[&[Oid]]) -> &'static [Oid] {
    let total: usize = parts.iter().map(|p| p.len()).sum();
    let mut all: Vec<Oid> = Vec::with_capacity(total);
    for p in parts {
        all.extend_from_slice(p);
    }
    Box::leak(all.into_boxed_slice())
}

/// Helper used by the tuple macro: bounds-check that the column slice is
/// at least `expected` long, returning a clear error otherwise.
fn need_columns(columns: &[Option<Bytes>], expected: usize) -> Result<()> {
    if columns.len() < expected {
        return Err(Error::Codec(format!(
            "tuple decoder needs {expected} columns, got {}",
            columns.len()
        )));
    }
    Ok(())
}

/// Generate `Encoder` + `Decoder` impls for one tuple arity.
///
/// Each component is one quad of tokens: tuple index, codec type, value
/// type, and a fresh local binding name used inside `decode`. Declarative
/// macros can't paste a name with `$idx` so we pass the binding ident
/// explicitly.
macro_rules! tuple_codec {
    ( $( $idx:tt $C:ident $T:ident $v:ident )+ ) => {
        impl<$($C, $T),+> Encoder<($($T,)+)> for ($($C,)+)
        where
            $($C: Encoder<$T>,)+
        {
            fn encode(
                &self,
                value: &($($T,)+),
                params: &mut Vec<Option<Vec<u8>>>,
            ) -> Result<()> {
                $( self.$idx.encode(&value.$idx, params)?; )+
                Ok(())
            }
            fn oids(&self) -> &'static [Oid] {
                concat_static_oids(&[ $( self.$idx.oids() ),+ ])
            }
        }

        impl<$($C, $T),+> Decoder<($($T,)+)> for ($($C,)+)
        where
            $($C: Decoder<$T>,)+
        {
            #[allow(unused_assignments)] // last `offset += n` after the final component
            fn decode(&self, columns: &[Option<Bytes>]) -> Result<($($T,)+)> {
                need_columns(columns, <Self as Decoder<($($T,)+)>>::n_columns(self))?;
                let mut offset = 0_usize;
                $(
                    let n = self.$idx.n_columns();
                    let $v = self.$idx.decode(&columns[offset..offset + n])?;
                    offset += n;
                )+
                Ok(( $( $v, )+ ))
            }
            fn n_columns(&self) -> usize {
                0_usize $( + self.$idx.n_columns() )+
            }
            fn oids(&self) -> &'static [Oid] {
                concat_static_oids(&[ $( self.$idx.oids() ),+ ])
            }
        }
    };
}

tuple_codec!(0 C0 T0 v0);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8 9 C9 T9 v9);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8 9 C9 T9 v9 10 C10 T10 v10);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8 9 C9 T9 v9 10 C10 T10 v10 11 C11 T11 v11);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8 9 C9 T9 v9 10 C10 T10 v10 11 C11 T11 v11 12 C12 T12 v12);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8 9 C9 T9 v9 10 C10 T10 v10 11 C11 T11 v11 12 C12 T12 v12 13 C13 T13 v13);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8 9 C9 T9 v9 10 C10 T10 v10 11 C11 T11 v11 12 C12 T12 v12 13 C13 T13 v13 14 C14 T14 v14);
tuple_codec!(0 C0 T0 v0 1 C1 T1 v1 2 C2 T2 v2 3 C3 T3 v3 4 C4 T4 v4 5 C5 T5 v5 6 C6 T6 v6 7 C7 T7 v7 8 C8 T8 v8 9 C9 T9 v9 10 C10 T10 v10 11 C11 T11 v11 12 C12 T12 v12 13 C13 T13 v13 14 C14 T14 v14 15 C15 T15 v15);

/// The unit type encodes/decodes nothing and is the natural "no
/// parameters" / "no columns" codec. Used as the parameter codec for
/// queries that take no arguments.
impl Encoder<()> for () {
    fn encode(&self, _value: &(), _params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[]
    }
}

impl Decoder<()> for () {
    fn decode(&self, _columns: &[Option<Bytes>]) -> Result<()> {
        Ok(())
    }
    fn n_columns(&self) -> usize {
        0
    }
    fn oids(&self) -> &'static [Oid] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{bool, int4, text};

    #[test]
    fn pair_encode_concatenates_slots() {
        let codec = (int4, text);
        let mut params = Vec::new();
        codec
            .encode(&(7_i32, "hi".to_string()), &mut params)
            .unwrap();
        assert_eq!(params, vec![Some(b"7".to_vec()), Some(b"hi".to_vec())]);
    }

    #[test]
    fn pair_decode_splits_columns() {
        let codec = (int4, text);
        let cols = [
            Some(Bytes::from_static(b"42")),
            Some(Bytes::from_static(b"hi")),
        ];
        assert_eq!(codec.decode(&cols).unwrap(), (42_i32, "hi".to_string()));
    }

    #[test]
    fn triple_decode_three_columns() {
        let codec = (int4, text, bool);
        let cols = [
            Some(Bytes::from_static(b"1")),
            Some(Bytes::from_static(b"yo")),
            Some(Bytes::from_static(b"t")),
        ];
        assert_eq!(
            codec.decode(&cols).unwrap(),
            (1_i32, "yo".to_string(), true)
        );
    }

    #[test]
    fn unit_codec_consumes_nothing() {
        assert_eq!(().n_columns(), 0);
        let mut params = Vec::new();
        ().encode(&(), &mut params).unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn n_columns_sums_components() {
        let codec = (int4, (text, bool), int4);
        // Three components: 1 + 2 + 1 = 4 columns.
        assert_eq!(codec.n_columns(), 4);
    }
}
