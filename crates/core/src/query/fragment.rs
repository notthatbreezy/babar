//! `Fragment<A>`: SQL pieces + encoder for the parameter tuple.
//!
//! See [`super`] for the high-level mental model. `Fragment` is the reusable
//! builder; call [`Fragment::query`] or [`Fragment::command`] when you want the
//! final typed statement wrapper.

use std::fmt::Write as _;
use std::sync::Arc;

use bytes::Bytes;

use super::{Command, Query};
use crate::codec::{Decoder, Encoder};
use crate::error::Result;
use crate::types::{Oid, Type};

/// Source location captured for a macro-built fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Origin {
    file: &'static str,
    line: u32,
    column: u32,
}

impl Origin {
    /// Create a new source origin.
    pub const fn new(file: &'static str, line: u32, column: u32) -> Self {
        Self { file, line, column }
    }

    /// Source file path.
    pub const fn file(self) -> &'static str {
        self.file
    }

    /// 1-based source line.
    pub const fn line(self) -> u32 {
        self.line
    }

    /// 1-based source column.
    pub const fn column(self) -> u32 {
        self.column
    }
}

#[doc(hidden)]
pub struct BoundStatement {
    pub sql: String,
    pub params: Vec<Option<Vec<u8>>>,
    pub param_types: Vec<Type>,
    pub param_formats: Vec<i16>,
}

impl BoundStatement {
    #[doc(hidden)]
    pub fn new(
        sql: String,
        params: Vec<Option<Vec<u8>>>,
        param_types: Vec<Type>,
        param_formats: Vec<i16>,
    ) -> Self {
        Self {
            sql,
            params,
            param_types,
            param_formats,
        }
    }
}

#[doc(hidden)]
pub fn push_bound_param<C, A>(
    codec: &C,
    value: &A,
    params: &mut Vec<Option<Vec<u8>>>,
    param_types: &mut Vec<Type>,
    param_formats: &mut Vec<i16>,
) -> Result<()>
where
    C: Encoder<A>,
{
    codec.encode(value, params)?;
    param_types.extend_from_slice(codec.types());
    param_formats.extend_from_slice(codec.format_codes());
    Ok(())
}

#[doc(hidden)]
pub fn push_null_param<C, A>(
    codec: &C,
    params: &mut Vec<Option<Vec<u8>>>,
    param_types: &mut Vec<Type>,
    param_formats: &mut Vec<i16>,
) where
    C: Encoder<A>,
{
    for _ in 0..codec.oids().len() {
        params.push(None);
    }
    param_types.extend_from_slice(codec.types());
    param_formats.extend_from_slice(codec.format_codes());
}

#[doc(hidden)]
pub trait DynamicStatement<A>: Send + Sync {
    fn bind(&self, args: &A) -> Result<BoundStatement>;
}

impl<A, F> DynamicStatement<A> for F
where
    F: Fn(&A) -> Result<BoundStatement> + Send + Sync,
{
    fn bind(&self, args: &A) -> Result<BoundStatement> {
        self(args)
    }
}

/// Hidden zero-slot encoder used by proc macros for optional group toggles.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Toggle;

#[doc(hidden)]
#[allow(non_upper_case_globals)]
pub const toggle: Toggle = Toggle;

impl Encoder<bool> for Toggle {
    fn encode(&self, _value: &bool, _params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[]
    }

    fn types(&self) -> &'static [Type] {
        &[]
    }
}

/// SQL with embedded parameter placeholders, parameterized by the tuple
/// of parameter values it expects.
pub struct Fragment<A> {
    pub(crate) sql: String,
    pub(crate) encoder: Arc<dyn Encoder<A> + Send + Sync>,
    pub(crate) n_params: usize,
    pub(crate) origin: Option<Origin>,
    pub(crate) dynamic: Option<Arc<dyn DynamicStatement<A>>>,
}

impl<A> Clone for Fragment<A> {
    fn clone(&self) -> Self {
        Self {
            sql: self.sql.clone(),
            encoder: Arc::clone(&self.encoder),
            n_params: self.n_params,
            origin: self.origin,
            dynamic: self.dynamic.as_ref().map(Arc::clone),
        }
    }
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
            origin: None,
            dynamic: None,
        }
    }
}

impl<A> Fragment<A> {
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
}

impl<A: 'static> Fragment<A> {
    /// Append a parameter placeholder backed by `codec`. The new
    /// fragment's parameter type is the previous parameter tuple paired
    /// with the codec's value type — i.e., a left-leaning pair.
    ///
    /// Most callers should let type inference carry this builder type and
    /// finish with [`Fragment::query`] or [`Fragment::command`]. The
    /// [`crate::sql!`] macro builds fragments with flatter tuple types.
    ///
    /// ```
    /// use babar::codec::{int4, text};
    /// use babar::query::Fragment;
    ///
    /// let q = Fragment::lit("SELECT ")
    ///     .bind(int4)
    ///     .append_lit(" || ")
    ///     .bind(text)
    ///     .query((text,));
    ///
    /// assert_eq!(q.sql(), "SELECT $1 || $2");
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
            origin: self.origin,
            dynamic: None,
        }
    }

    /// Concatenate two fragments. The right fragment's `$N` placeholders
    /// are renumbered to start one past the left fragment's count, and
    /// its encoder is paired with the left's.
    ///
    /// The resulting parameter tuple is `(A, B)` — Skunk-style nesting.
    /// Most callers should keep building with type inference and only name
    /// the final [`Query`] or [`Command`] type.
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
            origin: self.origin.or(other.origin),
            dynamic: None,
        }
    }
}

impl<A> Fragment<A> {
    /// Finish building a row-returning statement.
    pub fn query<B, D>(self, decoder: D) -> Query<A, B>
    where
        D: Decoder<B> + Send + Sync + 'static,
    {
        Query::new(self, decoder)
    }

    /// Finish building a command that returns only an affected-row count.
    pub fn command(self) -> Command<A> {
        Command::new(self)
    }

    /// SQL exactly as it will be sent.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub(crate) fn sql_for(&self, args: &A) -> Result<String> {
        Ok(self.bind_runtime(args)?.sql)
    }

    pub(crate) fn bind_runtime(&self, args: &A) -> Result<BoundStatement> {
        if let Some(dynamic) = &self.dynamic {
            dynamic.bind(args)
        } else {
            let mut params = Vec::with_capacity(self.encoder.oids().len());
            self.encoder.encode(args, &mut params)?;
            Ok(BoundStatement::new(
                self.sql.clone(),
                params,
                self.encoder.types().to_vec(),
                self.encoder.format_codes().to_vec(),
            ))
        }
    }

    /// Postgres OIDs the encoder declares, in placeholder order.
    pub fn param_oids(&self) -> &'static [Oid] {
        self.encoder.oids()
    }

    /// Postgres type metadata the encoder declares, in placeholder order.
    pub fn param_types(&self) -> &'static [Type] {
        self.encoder.types()
    }

    /// Number of parameter placeholders this fragment carries.
    pub fn n_params(&self) -> usize {
        self.n_params
    }

    /// Source location captured when the fragment was macro-expanded.
    pub fn origin(&self) -> Option<Origin> {
        self.origin
    }

    /// Set or replace the fragment's source origin.
    #[must_use]
    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Construct a fragment from already-numbered SQL and an encoder.
    #[doc(hidden)]
    pub fn __from_parts<E>(
        sql: impl Into<String>,
        encoder: E,
        n_params: usize,
        origin: Option<Origin>,
    ) -> Self
    where
        E: Encoder<A> + Send + Sync + 'static,
    {
        Self {
            sql: sql.into(),
            encoder: Arc::new(encoder),
            n_params,
            origin,
            dynamic: None,
        }
    }

    /// Construct a fragment whose final SQL depends on the runtime arguments.
    ///
    /// These fragments remain executable, but callers must not assume they are
    /// generically preparable because the concrete SQL shape is only known once
    /// arguments are bound.
    #[doc(hidden)]
    pub fn __from_dynamic_parts<E, D>(
        sql: impl Into<String>,
        encoder: E,
        n_params: usize,
        origin: Option<Origin>,
        dynamic: D,
    ) -> Self
    where
        E: Encoder<A> + Send + Sync + 'static,
        D: DynamicStatement<A> + 'static,
    {
        Self {
            sql: sql.into(),
            encoder: Arc::new(encoder),
            n_params,
            origin,
            dynamic: Some(Arc::new(dynamic)),
        }
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
    fn types(&self) -> &'static [Type] {
        let mut all: Vec<Type> = self.head.types().to_vec();
        all.extend_from_slice(self.tail.types());
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
    fn types(&self) -> &'static [Type] {
        let mut all: Vec<Type> = self.head.types().to_vec();
        all.extend_from_slice(self.tail.types());
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
    fn types(&self) -> &'static [Type] {
        (**self).types()
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
    fn types(&self) -> &'static [Type] {
        (**self).types()
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
        assert!(f.origin().is_none());
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
    fn query_and_command_wrap_fragment() {
        let fragment = Fragment::lit("SELECT * FROM t WHERE id = ")
            .bind(int4)
            .with_origin(Origin::new("demo.rs", 1, 1));
        let query: Query<((), i32), (i32,)> = fragment.clone().query((int4,));
        let command: Command<((), i32)> = fragment.command();
        assert_eq!(query.sql(), "SELECT * FROM t WHERE id = $1");
        assert_eq!(command.sql(), "SELECT * FROM t WHERE id = $1");
        assert_eq!(query.origin(), Some(Origin::new("demo.rs", 1, 1)));
        assert_eq!(command.origin(), Some(Origin::new("demo.rs", 1, 1)));
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
    fn origin_survives_bind_and_prefers_left_on_plus() {
        let left = Fragment::lit("SELECT ")
            .bind(int4)
            .with_origin(Origin::new("left.rs", 10, 2));
        let right = Fragment::lit(" WHERE x = ")
            .bind(int4)
            .with_origin(Origin::new("right.rs", 20, 4));
        let combined = left.plus(right);
        assert_eq!(combined.origin(), Some(Origin::new("left.rs", 10, 2)));
    }

    #[test]
    fn renumber_handles_multidigit_and_no_match() {
        assert_eq!(renumber_placeholders("nothing here", 5), "nothing here");
        assert_eq!(renumber_placeholders("$1, $2, $10", 3), "$4, $5, $13");
        assert_eq!(renumber_placeholders("$1$2", 1), "$2$3");
    }
}
