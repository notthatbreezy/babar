//! `nullable(codec)` lifts a codec into one that round-trips
//! `Option<A>` — `None` ↔ SQL `NULL`.

use bytes::Bytes;

use super::{Decoder, Encoder};
use crate::error::Result;
use crate::types::{Oid, Type};

/// Codec wrapper that maps `None` to SQL `NULL` and `Some(v)` to the
/// inner codec's encoding of `v`.
///
/// Construct with [`nullable`].
#[derive(Debug, Clone, Copy)]
pub struct Nullable<C>(pub(crate) C);

/// Lift `codec` into a codec for `Option<A>`. `Option::None` round-trips
/// as SQL `NULL`.
///
/// ```
/// use babar::codec::{int4, nullable};
///
/// // nullable_int4: Codec<Option<i32>>
/// let nullable_int4 = nullable(int4);
/// ```
pub fn nullable<C>(codec: C) -> Nullable<C> {
    Nullable(codec)
}

impl<C, A> Encoder<Option<A>> for Nullable<C>
where
    C: Encoder<A>,
{
    fn encode(&self, value: &Option<A>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        match value {
            None => {
                // One slot per OID the inner encoder declares — we don't
                // know what shape the inner encoder produces beyond
                // count, so push that many NULL slots.
                for _ in 0..self.0.oids().len() {
                    params.push(None);
                }
                Ok(())
            }
            Some(v) => self.0.encode(v, params),
        }
    }
    fn oids(&self) -> &'static [Oid] {
        self.0.oids()
    }
    fn types(&self) -> &'static [Type] {
        self.0.types()
    }
    fn format_codes(&self) -> &'static [i16] {
        self.0.format_codes()
    }
}

impl<C, A> Decoder<Option<A>> for Nullable<C>
where
    C: Decoder<A>,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Option<A>> {
        // If every column slot the inner decoder consumes is NULL, we
        // surface None. Otherwise we delegate. (For primitive codecs
        // this is one column; for composite codecs all-None means the
        // whole composite is NULL.)
        let n = self.0.n_columns();
        if columns[..n].iter().all(Option::is_none) {
            Ok(None)
        } else {
            self.0.decode(columns).map(Some)
        }
    }
    fn n_columns(&self) -> usize {
        self.0.n_columns()
    }
    fn oids(&self) -> &'static [Oid] {
        self.0.oids()
    }
    fn types(&self) -> &'static [Type] {
        self.0.types()
    }
    fn format_codes(&self) -> &'static [i16] {
        self.0.format_codes()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::codec::{int4, text};
    use crate::error::Result;
    use crate::types::{self, Type};

    #[derive(Clone, Copy)]
    struct DynamicGeometryCodec;

    impl Encoder<()> for DynamicGeometryCodec {
        fn encode(&self, _value: &(), params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
            params.push(Some(Vec::new()));
            Ok(())
        }

        fn oids(&self) -> &'static [Oid] {
            &[0]
        }

        fn types(&self) -> &'static [Type] {
            &[types::GEOMETRY_TYPE]
        }
    }

    impl Decoder<()> for DynamicGeometryCodec {
        fn decode(&self, _columns: &[Option<Bytes>]) -> Result<()> {
            Ok(())
        }

        fn n_columns(&self) -> usize {
            1
        }

        fn oids(&self) -> &'static [Oid] {
            &[0]
        }

        fn types(&self) -> &'static [Type] {
            &[types::GEOMETRY_TYPE]
        }
    }

    #[test]
    fn nullable_int4_encode_some() {
        let codec = nullable(int4);
        let mut params = Vec::new();
        codec.encode(&Some(42_i32), &mut params).unwrap();
        // Binary format: 4 bytes big-endian.
        assert_eq!(params, vec![Some(42_i32.to_be_bytes().to_vec())]);
    }

    #[test]
    fn nullable_int4_encode_none() {
        let codec = nullable(int4);
        let mut params = Vec::new();
        codec.encode(&None::<i32>, &mut params).unwrap();
        assert_eq!(params, vec![None]);
    }

    #[test]
    fn nullable_int4_decode_null() {
        let codec = nullable(int4);
        assert_eq!(codec.decode(&[None]).unwrap(), None);
    }

    #[test]
    fn nullable_int4_decode_present() {
        let codec = nullable(int4);
        assert_eq!(
            codec.decode(&[Some(Bytes::from_static(b"42"))]).unwrap(),
            Some(42_i32)
        );
    }

    #[test]
    fn nullable_text_decode_empty_string_is_not_null() {
        let codec = nullable(text);
        let got = codec.decode(&[Some(Bytes::from_static(b""))]).unwrap();
        assert_eq!(got, Some(String::new()));
    }

    #[test]
    fn nullable_preserves_dynamic_type_metadata() {
        let codec = nullable(DynamicGeometryCodec);
        assert_eq!(Encoder::oids(&codec), &[0]);
        assert_eq!(Encoder::types(&codec), &[types::GEOMETRY_TYPE]);
    }
}
