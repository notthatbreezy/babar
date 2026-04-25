//! Property-based roundtrip tests for the M1 primitive codecs.
//!
//! We exercise *encode → decode = identity* across the integer, float,
//! string, and bytea codecs at scale. Defaults to the proptest standard
//! 256 cases per property; CI runs nightly against `PROPTEST_CASES=2048`.

use bytes::Bytes;
use proptest::prelude::*;

use super::{
    bool as bool_codec, bpchar, bytea, float4, float8, int2, int4, int8, nullable, text, varchar,
    Decoder, Encoder,
};

/// Encode a value, then immediately decode the produced bytes back through
/// the same codec. The single produced slot is wrapped as a one-column row.
fn roundtrip<C, A>(codec: &C, v: &A) -> A
where
    C: Encoder<A> + Decoder<A>,
{
    let mut params = Vec::new();
    codec.encode(v, &mut params).expect("encode");
    assert_eq!(params.len(), 1, "primitive codecs produce exactly one slot");
    let bytes = params.into_iter().next().unwrap().expect("not NULL");
    codec.decode(&[Some(Bytes::from(bytes))]).expect("decode")
}

proptest! {
    #[test]
    fn int2_roundtrip(v in any::<i16>()) {
        prop_assert_eq!(roundtrip(&int2, &v), v);
    }

    #[test]
    fn int4_roundtrip(v in any::<i32>()) {
        prop_assert_eq!(roundtrip(&int4, &v), v);
    }

    #[test]
    fn int8_roundtrip(v in any::<i64>()) {
        prop_assert_eq!(roundtrip(&int8, &v), v);
    }

    #[test]
    fn bool_roundtrip(v in any::<bool>()) {
        prop_assert_eq!(roundtrip(&bool_codec, &v), v);
    }

    #[test]
    fn float8_roundtrip_finite(v in any::<f64>().prop_filter("finite", |x| x.is_finite())) {
        // Bit-exact equality: format!("{:?}") for floats is documented to
        // round-trip losslessly through parse::<f64>(), and we rely on that
        // for the encode path.
        let out = roundtrip(&float8, &v);
        prop_assert_eq!(out.to_bits(), v.to_bits());
    }

    #[test]
    fn float4_roundtrip_finite(v in any::<f32>().prop_filter("finite", |x| x.is_finite())) {
        let out = roundtrip(&float4, &v);
        prop_assert_eq!(out.to_bits(), v.to_bits());
    }

    #[test]
    fn text_roundtrip(s in any::<String>()) {
        prop_assert_eq!(roundtrip(&text, &s.clone()), s);
    }

    #[test]
    fn varchar_roundtrip(s in any::<String>()) {
        prop_assert_eq!(roundtrip(&varchar, &s.clone()), s);
    }

    #[test]
    fn bpchar_roundtrip(s in any::<String>()) {
        prop_assert_eq!(roundtrip(&bpchar, &s.clone()), s);
    }

    #[test]
    fn bytea_roundtrip(v in proptest::collection::vec(any::<u8>(), 0..512)) {
        prop_assert_eq!(roundtrip(&bytea, &v.clone()), v);
    }

    #[test]
    fn nullable_int4_roundtrip(v in proptest::option::of(any::<i32>())) {
        let codec = nullable(int4);
        let mut params = Vec::new();
        codec.encode(&v, &mut params).expect("encode");
        // Nullable produces exactly one slot regardless of presence.
        prop_assert_eq!(params.len(), 1);
        let cell: Option<Bytes> = params.into_iter().next().unwrap().map(Bytes::from);
        let decoded = codec.decode(&[cell]).expect("decode");
        prop_assert_eq!(decoded, v);
    }
}
