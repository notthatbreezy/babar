//! `Fragment<A>`: SQL pieces + encoder for the parameter tuple.
//!
//! See [`super`] for the high-level mental model. Each `bind` extends
//! the parameter tuple by appending a new element on the right, in
//! Skunk-style left-leaning pairs.

use std::fmt::Write as _;
use std::sync::Arc;

use bytes::Bytes;

use crate::codec::{Decoder, Encoder};
use crate::error::Result;
use crate::types::Oid;

/// SQL with embedded parameter placeholders, parameterized by the tuple
/// of parameter values it expects.
pub struct Fragment<A> {
    pub(crate) sql: String,
    pub(crate) encoder: Arc<dyn Encoder<A> + Send + Sync>,
    pub(crate) n_params: usize,
}

impl Fragment<()> {
    /// Start a fragment from a literal SQL string. The string is sent to
    /// the server unchanged; do not embed `$N` placeholders here — use
    /// [`Self::bind`] to add them.
    pub fn lit(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            encoder: Arc::new(()),
            n_params: 0,
        }
    }
}

impl<A> Fragment<A>
where
    A: Send + Sync + 'static,
{
    /// Append literal SQL to the fragment without consuming it. Useful
    /// between `bind` calls when you need text in the middle.
    ///
    /// ```
    /// use babar::codec::int4;
    /// use babar::query::Fragment;
    ///
    /// let f = Fragment::lit("SELECT * FROM t WHERE id = ")
    ///     .bind(int4)
    ///     .append_lit(" AND active");
    /// // SQL: "SELECT * FROM t WHERE id = $1 AND active"
    /// ```
    #[must_use]
    pub fn append_lit(mut self, sql: &str) -> Self {
        self.sql.push_str(sql);
        self
    }

    /// Append a parameter placeholder backed by `codec`. The new
    /// fragment's parameter type is the previous parameter tuple paired
    /// with the codec's value type — i.e., a left-leaning pair.
    ///
    /// ```
    /// use babar::codec::{int4, text};
    /// use babar::query::Fragment;
    ///
    /// // Fragment<((((), i32), String))> — three nested pairs.
    /// let _ = Fragment::lit("SELECT ")
    ///     .bind(int4)
    ///     .append_lit(" || ")
    ///     .bind(text);
    /// ```
    #[must_use]
    pub fn bind<C, X>(mut self, codec: C) -> Fragment<(A, X)>
    where
        C: Encoder<X> + Decoder<X> + Send + Sync + 'static,
        X: Send + Sync + 'static,
    {
        self.n_params += 1;
        let _ = write!(self.sql, "${}", self.n_params);
        let new_encoder = AppendOne {
            head: self.encoder,
            tail: codec,
        };
        Fragment {
            sql: self.sql,
            encoder: Arc::new(new_encoder),
            n_params: self.n_params,
        }
    }

    /// Concatenate two fragments. The right fragment's `$N` placeholders
    /// are renumbered to start one past the left fragment's count, and
    /// its encoder is paired with the left's.
    ///
    /// The resulting parameter tuple is `(A, B)` — Skunk-style nesting.
    /// If you want a flat tuple of all participants, build it via
    /// repeated `bind` instead, or wait for the M3 `sql!` macro.
    #[must_use]
    pub fn plus<B>(self, other: Fragment<B>) -> Fragment<(A, B)>
    where
        B: Send + Sync + 'static,
    {
        let renumbered = renumber_placeholders(&other.sql, self.n_params);
        let combined_sql = self.sql + &renumbered;
        let combined = AppendChain {
            head: self.encoder,
            tail: other.encoder,
        };
        Fragment {
            sql: combined_sql,
            encoder: Arc::new(combined),
            n_params: self.n_params + other.n_params,
        }
    }

    /// SQL exactly as it will be sent.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Number of parameter placeholders this fragment carries.
    pub fn n_params(&self) -> usize {
        self.n_params
    }
}

// --- Internal encoder shapes --------------------------------------------

/// Encoder produced by `Fragment::bind` — runs the head encoder over the
/// `&A` part of `&(A, X)`, then runs `tail` over the `&X` part.
struct AppendOne<H, T> {
    head: H,
    tail: T,
}

impl<H, T, A, X> Encoder<(A, X)> for AppendOne<H, T>
where
    H: Encoder<A> + Send + Sync,
    T: Encoder<X> + Send + Sync,
{
    fn encode(&self, value: &(A, X), params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        self.head.encode(&value.0, params)?;
        self.tail.encode(&value.1, params)
    }
    fn oids(&self) -> &'static [Oid] {
        let mut all: Vec<Oid> = self.head.oids().to_vec();
        all.extend_from_slice(self.tail.oids());
        Box::leak(all.into_boxed_slice())
    }
    fn format_codes(&self) -> &'static [i16] {
        let mut all: Vec<i16> = self.head.format_codes().to_vec();
        all.extend_from_slice(self.tail.format_codes());
        Box::leak(all.into_boxed_slice())
    }
}

/// Encoder produced by `Fragment::plus` — runs the head over `&A`, the
/// tail over `&B`.
struct AppendChain<H, T> {
    head: H,
    tail: T,
}

impl<H, T, A, B> Encoder<(A, B)> for AppendChain<H, T>
where
    H: Encoder<A> + Send + Sync,
    T: Encoder<B> + Send + Sync,
{
    fn encode(&self, value: &(A, B), params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        self.head.encode(&value.0, params)?;
        self.tail.encode(&value.1, params)
    }
    fn oids(&self) -> &'static [Oid] {
        let mut all: Vec<Oid> = self.head.oids().to_vec();
        all.extend_from_slice(self.tail.oids());
        Box::leak(all.into_boxed_slice())
    }
    fn format_codes(&self) -> &'static [i16] {
        let mut all: Vec<i16> = self.head.format_codes().to_vec();
        all.extend_from_slice(self.tail.format_codes());
        Box::leak(all.into_boxed_slice())
    }
}

/// `Arc<dyn Encoder<A>>` is itself an `Encoder<A>` via deref.
impl<A, T: Encoder<A> + ?Sized> Encoder<A> for Arc<T> {
    fn encode(&self, value: &A, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        (**self).encode(value, params)
    }
    fn oids(&self) -> &'static [Oid] {
        (**self).oids()
    }
    fn format_codes(&self) -> &'static [i16] {
        (**self).format_codes()
    }
}

/// Same blanket for Decoder.
impl<A, T: Decoder<A> + ?Sized> Decoder<A> for Arc<T> {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<A> {
        (**self).decode(columns)
    }
    fn n_columns(&self) -> usize {
        (**self).n_columns()
    }
    fn oids(&self) -> &'static [Oid] {
        (**self).oids()
    }
    fn format_codes(&self) -> &'static [i16] {
        (**self).format_codes()
    }
}

/// Walk SQL and rewrite `$N` placeholders by adding `offset` to each.
///
/// Naive scan; doesn't try to be clever about quoted strings or comments
/// because user-built fragments shouldn't contain those between
/// placeholders. The M3 macro will own placeholder generation entirely.
fn renumber_placeholders(sql: &str, offset: usize) -> String {
    if offset == 0 {
        return sql.to_string();
    }
    let mut out = String::with_capacity(sql.len() + 4);
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let n: usize = std::str::from_utf8(&bytes[i + 1..j])
                .expect("ascii digits")
                .parse()
                .expect("ascii digits parse");
            let _ = write!(out, "${}", n + offset);
            i = j;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{bool, int4, text};

    #[test]
    fn lit_carries_text_unchanged() {
        let f = Fragment::lit("SELECT 1");
        assert_eq!(f.sql(), "SELECT 1");
        assert_eq!(f.n_params(), 0);
    }

    #[test]
    fn bind_appends_placeholder_and_encoder() {
        let f = Fragment::lit("SELECT * WHERE id = ").bind(int4);
        assert_eq!(f.sql(), "SELECT * WHERE id = $1");
        assert_eq!(f.n_params(), 1);
        let mut params = Vec::new();
        f.encoder.encode(&((), 7_i32), &mut params).unwrap();
        // int4 binary: 4 bytes big-endian.
        assert_eq!(params, vec![Some(7_i32.to_be_bytes().to_vec())]);
    }

    #[test]
    fn three_binds_chain_left() {
        let f = Fragment::lit("INSERT (")
            .bind(int4)
            .append_lit(", ")
            .bind(text)
            .append_lit(", ")
            .bind(bool)
            .append_lit(")");
        assert_eq!(f.sql(), "INSERT ($1, $2, $3)");
        assert_eq!(f.n_params(), 3);
        // Three nested pairs: (((), i32), String) then ((..), bool).
        let v: ((((), i32), String), bool) = ((((), 1_i32), "two".to_string()), true);
        let mut params: Vec<Option<Vec<u8>>> = Vec::new();
        f.encoder.encode(&v, &mut params).unwrap();
        assert_eq!(
            params,
            vec![
                Some(1_i32.to_be_bytes().to_vec()),
                Some(b"two".to_vec()),
                Some(vec![1_u8]), // bool binary: 0x01
            ]
        );
    }

    #[test]
    fn plus_renumbers_placeholders() {
        let left = Fragment::lit("SELECT ").bind(int4);
        let right = Fragment::lit(" WHERE x = ").bind(int4);
        let combined = left.plus(right);
        assert_eq!(combined.sql(), "SELECT $1 WHERE x = $2");
        assert_eq!(combined.n_params(), 2);
    }

    #[test]
    fn renumber_handles_multidigit_and_no_match() {
        assert_eq!(renumber_placeholders("nothing here", 5), "nothing here");
        assert_eq!(renumber_placeholders("$1, $2, $10", 3), "$4, $5, $13");
        assert_eq!(renumber_placeholders("$1$2", 1), "$2$3");
    }
}
