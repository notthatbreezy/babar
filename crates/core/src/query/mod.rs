//! Typed queries and commands plus the `Fragment` builder.
//!
//! The flow:
//!
//! ```text
//! Fragment<A>  ─────►  Command<A>          (no rows produced)
//!              ─►  Query<A, B>     (rows decoded as B)
//! ```
//!
//! `Fragment` holds SQL pieces and the encoder for the parameter tuple `A`.
//! `Command` and `Query` wrap a fragment with the statement shape you hand to
//! `Session::execute`, `Session::query`, or `Session::prepare_*`.

mod fragment;

pub use fragment::{
    push_bound_param, push_null_param, toggle, BoundStatement, Fragment, Origin, Toggle,
};

use std::sync::Arc;

use crate::codec::{Decoder, Encoder};
use crate::types::{Oid, Type};

/// A statement that returns rows. `A` is the parameter tuple, `B` is the
/// per-row output type produced by the decoder.
pub struct Query<A, B> {
    pub(crate) fragment: Fragment<A>,
    pub(crate) decoder: Arc<dyn Decoder<B> + Send + Sync>,
}

impl<A, B> Query<A, B> {
    /// Build a query directly from a raw SQL string, an encoder for the
    /// parameter tuple `A`, and a decoder for the row type `B`.
    ///
    /// SQL placeholders use Postgres' native `$1`, `$2`, ... numbering.
    /// The encoder is responsible for producing exactly that many param slots
    /// in the same order.
    pub fn raw<E, D>(sql: impl Into<String>, encoder: E, decoder: D) -> Self
    where
        E: Encoder<A> + Send + Sync + 'static,
        D: Decoder<B> + Send + Sync + 'static,
    {
        let n_params = encoder.oids().len();
        Self::new(
            Fragment::__from_parts(sql, encoder, n_params, None),
            decoder,
        )
    }

    /// Build from a [`Fragment`] plus a decoder.
    pub fn new<D>(fragment: Fragment<A>, decoder: D) -> Self
    where
        D: Decoder<B> + Send + Sync + 'static,
    {
        Self {
            fragment,
            decoder: Arc::new(decoder),
        }
    }

    /// Build from a [`Fragment`] plus a decoder.
    pub fn from_fragment<D>(fragment: Fragment<A>, decoder: D) -> Self
    where
        D: Decoder<B> + Send + Sync + 'static,
    {
        Self::new(fragment, decoder)
    }

    /// SQL text exactly as it will be sent to the server.
    pub fn sql(&self) -> &str {
        self.fragment.sql()
    }

    /// SQL text for a specific argument set, after optional typed-query inputs
    /// are applied.
    pub fn sql_for(&self, args: &A) -> crate::error::Result<String> {
        self.fragment.sql_for(args)
    }

    /// Macro callsite captured by [`crate::sql!`], when available.
    pub fn origin(&self) -> Option<Origin> {
        self.fragment.origin()
    }

    /// Postgres OIDs the encoder declares, in placeholder order.
    pub fn param_oids(&self) -> &'static [Oid] {
        self.fragment.param_oids()
    }

    /// Postgres type metadata the encoder declares, in placeholder order.
    pub fn param_types(&self) -> &'static [Type] {
        self.fragment.param_types()
    }

    /// Postgres OIDs the decoder expects, in column order.
    pub fn output_oids(&self) -> &'static [Oid] {
        self.decoder.oids()
    }

    /// Postgres type metadata the decoder expects, in column order.
    pub fn output_types(&self) -> &'static [Type] {
        self.decoder.types()
    }

    /// Number of columns the decoder expects.
    pub fn n_columns(&self) -> usize {
        self.decoder.n_columns()
    }

    /// The underlying SQL fragment and parameter codec.
    pub fn fragment(&self) -> &Fragment<A> {
        &self.fragment
    }
}

/// A statement that does not produce rows (DDL, `INSERT`/`UPDATE`/`DELETE`
/// without `RETURNING`). `A` is the parameter tuple.
pub struct Command<A> {
    pub(crate) fragment: Fragment<A>,
}

impl<A> Command<A> {
    /// Build a command directly from raw SQL and a parameter encoder.
    pub fn raw<E>(sql: impl Into<String>, encoder: E) -> Self
    where
        E: Encoder<A> + Send + Sync + 'static,
    {
        let n_params = encoder.oids().len();
        Self::new(Fragment::__from_parts(sql, encoder, n_params, None))
    }

    /// Build from a [`Fragment`].
    pub fn new(fragment: Fragment<A>) -> Self {
        Self { fragment }
    }

    /// Build from a [`Fragment`].
    pub fn from_fragment(fragment: Fragment<A>) -> Self {
        Self::new(fragment)
    }

    /// SQL text exactly as it will be sent to the server.
    pub fn sql(&self) -> &str {
        self.fragment.sql()
    }

    /// SQL text for a specific argument set, after optional typed-query inputs
    /// are applied.
    pub fn sql_for(&self, args: &A) -> crate::error::Result<String> {
        self.fragment.sql_for(args)
    }

    /// Macro callsite captured by [`crate::sql!`], when available.
    pub fn origin(&self) -> Option<Origin> {
        self.fragment.origin()
    }

    /// Postgres OIDs the encoder declares, in placeholder order.
    pub fn param_oids(&self) -> &'static [Oid] {
        self.fragment.param_oids()
    }

    /// Postgres type metadata the encoder declares, in placeholder order.
    pub fn param_types(&self) -> &'static [Type] {
        self.fragment.param_types()
    }

    /// The underlying SQL fragment and parameter codec.
    pub fn fragment(&self) -> &Fragment<A> {
        &self.fragment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{bool, int4, text};
    use crate::error::Result;
    use crate::types;
    use bytes::Bytes;

    #[derive(Clone, Copy)]
    struct DynamicGeometryCodec;

    impl Encoder<()> for DynamicGeometryCodec {
        fn encode(&self, _value: &(), params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
            params.push(Some(Vec::new()));
            Ok(())
        }

        fn oids(&self) -> &'static [types::Oid] {
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

        fn oids(&self) -> &'static [types::Oid] {
            &[0]
        }

        fn types(&self) -> &'static [Type] {
            &[types::GEOMETRY_TYPE]
        }
    }

    #[test]
    fn query_raw_carries_metadata() {
        let q: Query<(i32, String), (i32,)> = Query::raw(
            "SELECT id FROM t WHERE name = $1 AND id > $2",
            (int4, text),
            (int4,),
        );
        assert_eq!(q.sql(), "SELECT id FROM t WHERE name = $1 AND id > $2");
        assert_eq!(q.param_oids(), &[types::INT4, types::TEXT][..]);
        assert_eq!(q.output_oids(), &[types::INT4][..]);
        assert_eq!(q.n_columns(), 1);
        assert!(q.origin().is_none());
    }

    #[test]
    fn command_raw_carries_metadata() {
        let cmd: Command<(i32, core::primitive::bool)> =
            Command::raw("UPDATE t SET active = $2 WHERE id = $1", (int4, bool));
        assert_eq!(cmd.param_oids(), &[types::INT4, types::BOOL][..]);
        assert!(cmd.origin().is_none());
    }

    #[test]
    fn query_new_uses_fragment_sql_and_encoder() {
        let q: Query<((), i32), (i32, String)> =
            Fragment::lit("SELECT id, name FROM t WHERE id = ")
                .bind(int4)
                .with_origin(Origin::new("demo.rs", 1, 1))
                .query((int4, text));
        assert_eq!(q.sql(), "SELECT id, name FROM t WHERE id = $1");
        assert_eq!(q.param_oids(), &[types::INT4][..]);
        assert_eq!(q.output_oids(), &[types::INT4, types::TEXT][..]);
        assert_eq!(q.origin(), Some(Origin::new("demo.rs", 1, 1)));
    }

    #[test]
    fn command_new_wraps_fragment() {
        let cmd: Command<((), i32)> = Fragment::lit("DELETE FROM t WHERE id = ")
            .bind(int4)
            .with_origin(Origin::new("demo.rs", 2, 4))
            .command();
        assert_eq!(cmd.sql(), "DELETE FROM t WHERE id = $1");
        assert_eq!(cmd.param_oids(), &[types::INT4][..]);
        assert_eq!(cmd.origin(), Some(Origin::new("demo.rs", 2, 4)));
    }

    #[test]
    fn query_exposes_dynamic_type_metadata() {
        let q: Query<(), ()> = Query::raw(
            "SELECT $1::geometry",
            DynamicGeometryCodec,
            DynamicGeometryCodec,
        );

        assert_eq!(q.param_oids(), &[0]);
        assert_eq!(q.output_oids(), &[0]);
        assert_eq!(q.param_types(), &[types::GEOMETRY_TYPE]);
        assert_eq!(q.output_types(), &[types::GEOMETRY_TYPE]);
    }
}
