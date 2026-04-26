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
//! `Command` and `Query` are user-facing values you hand to `Session::execute`
//! or `Session::query`.

mod fragment;

pub use fragment::{Fragment, Origin};

use std::sync::Arc;

use crate::codec::{Decoder, Encoder};
use crate::types::Oid;

/// A statement that returns rows. `A` is the parameter tuple, `B` is the
/// per-row output type produced by the decoder.
pub struct Query<A, B> {
    pub(crate) sql: String,
    pub(crate) encoder: Arc<dyn Encoder<A> + Send + Sync>,
    pub(crate) decoder: Arc<dyn Decoder<B> + Send + Sync>,
    pub(crate) origin: Option<Origin>,
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
        Self {
            sql: sql.into(),
            encoder: Arc::new(encoder),
            decoder: Arc::new(decoder),
            origin: None,
        }
    }

    /// Build from a [`Fragment`] plus a decoder.
    pub fn from_fragment<D>(fragment: Fragment<A>, decoder: D) -> Self
    where
        D: Decoder<B> + Send + Sync + 'static,
    {
        Self {
            sql: fragment.sql,
            encoder: fragment.encoder,
            decoder: Arc::new(decoder),
            origin: fragment.origin,
        }
    }

    /// SQL text exactly as it will be sent to the server.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Macro callsite captured by [`crate::sql!`], when available.
    pub fn origin(&self) -> Option<Origin> {
        self.origin
    }

    /// Postgres OIDs the encoder declares, in placeholder order.
    pub fn param_oids(&self) -> &'static [Oid] {
        self.encoder.oids()
    }

    /// Postgres OIDs the decoder expects, in column order.
    pub fn output_oids(&self) -> &'static [Oid] {
        self.decoder.oids()
    }

    /// Number of columns the decoder expects.
    pub fn n_columns(&self) -> usize {
        self.decoder.n_columns()
    }
}

/// A statement that does not produce rows (DDL, `INSERT`/`UPDATE`/`DELETE`
/// without `RETURNING`). `A` is the parameter tuple.
pub struct Command<A> {
    pub(crate) sql: String,
    pub(crate) encoder: Arc<dyn Encoder<A> + Send + Sync>,
    pub(crate) origin: Option<Origin>,
}

impl<A> Command<A> {
    /// Build a command directly from raw SQL and a parameter encoder.
    pub fn raw<E>(sql: impl Into<String>, encoder: E) -> Self
    where
        E: Encoder<A> + Send + Sync + 'static,
    {
        Self {
            sql: sql.into(),
            encoder: Arc::new(encoder),
            origin: None,
        }
    }

    /// Build from a [`Fragment`].
    pub fn from_fragment(fragment: Fragment<A>) -> Self {
        Self {
            sql: fragment.sql,
            encoder: fragment.encoder,
            origin: fragment.origin,
        }
    }

    /// SQL text exactly as it will be sent to the server.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Macro callsite captured by [`crate::sql!`], when available.
    pub fn origin(&self) -> Option<Origin> {
        self.origin
    }

    /// Postgres OIDs the encoder declares, in placeholder order.
    pub fn param_oids(&self) -> &'static [Oid] {
        self.encoder.oids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{bool, int4, text};
    use crate::types;

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
    fn query_from_fragment_uses_fragment_sql_and_encoder() {
        let f = Fragment::lit("SELECT id, name FROM t WHERE id = ")
            .bind(int4)
            .with_origin(Origin::new("demo.rs", 1, 1));
        let q: Query<((), i32), (i32, String)> = Query::from_fragment(f, (int4, text));
        assert_eq!(q.sql(), "SELECT id, name FROM t WHERE id = $1");
        assert_eq!(q.param_oids(), &[types::INT4][..]);
        assert_eq!(q.output_oids(), &[types::INT4, types::TEXT][..]);
        assert_eq!(q.origin(), Some(Origin::new("demo.rs", 1, 1)));
    }
}
