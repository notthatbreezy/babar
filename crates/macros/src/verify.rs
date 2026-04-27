use std::env;
use std::fmt;
use std::str::FromStr;

use postgres::error::ErrorPosition;
use postgres::{Client, NoTls, Statement};
use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Expr, ExprCall, ExprGroup, ExprParen, ExprPath};

const BABAR_DATABASE_URL_ENV: &str = "BABAR_DATABASE_URL";
const DATABASE_URL_ENV: &str = "DATABASE_URL";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerificationConfig {
    pub(crate) source: ConfigSource,
    pub(crate) database_url: String,
}

impl VerificationConfig {
    pub(crate) fn discover() -> Result<Option<Self>, VerificationError> {
        for source in [ConfigSource::BabarDatabaseUrl, ConfigSource::DatabaseUrl] {
            let Some(raw) = env::var_os(source.env_var()) else {
                continue;
            };
            let value = raw.into_string().map_err(|_| {
                VerificationError::configuration(format!(
                    "{} contains non-UTF-8 data; compile-time SQL verification requires UTF-8 connection strings",
                    source.env_var()
                ))
            })?;
            let value = value.trim();
            if value.is_empty() {
                return Err(VerificationError::configuration(format!(
                    "{} is set but empty",
                    source.env_var()
                )));
            }
            postgres::Config::from_str(value).map_err(|err| {
                VerificationError::configuration(format!(
                    "{} does not contain a valid PostgreSQL connection string: {err}",
                    source.env_var()
                ))
            })?;
            return Ok(Some(Self {
                source,
                database_url: value.to_owned(),
            }));
        }

        Ok(None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigSource {
    BabarDatabaseUrl,
    DatabaseUrl,
}

impl ConfigSource {
    pub(crate) fn env_var(self) -> &'static str {
        match self {
            Self::BabarDatabaseUrl => BABAR_DATABASE_URL_ENV,
            Self::DatabaseUrl => DATABASE_URL_ENV,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatementMetadata {
    pub(crate) params: Vec<TypeMetadata>,
    pub(crate) columns: Vec<ColumnMetadata>,
}

impl StatementMetadata {
    pub(crate) fn from_statement(statement: &Statement) -> Self {
        Self {
            params: statement
                .params()
                .iter()
                .map(TypeMetadata::from_type)
                .collect(),
            columns: statement
                .columns()
                .iter()
                .map(|column| ColumnMetadata {
                    name: column.name().to_owned(),
                    type_: TypeMetadata::from_type(column.type_()),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeMetadata {
    pub(crate) oid: u32,
    pub(crate) name: String,
}

impl TypeMetadata {
    fn from_type(ty: &postgres::types::Type) -> Self {
        Self {
            oid: ty.oid(),
            name: ty.name().to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnMetadata {
    pub(crate) name: String,
    pub(crate) type_: TypeMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclaredType {
    pub(crate) oid: u32,
    pub(crate) display: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Probe {
    config: VerificationConfig,
}

impl Probe {
    pub(crate) fn discover() -> Result<Option<Self>, VerificationError> {
        VerificationConfig::discover().map(|config| config.map(|config| Self { config }))
    }

    pub(crate) fn config(&self) -> &VerificationConfig {
        &self.config
    }

    pub(crate) fn describe(&self, sql: &str) -> Result<StatementMetadata, VerificationError> {
        let mut client = Client::connect(&self.config.database_url, NoTls).map_err(|err| {
            VerificationError::connection(format!(
                "failed to connect using {}: {err}",
                self.config.source.env_var()
            ))
        })?;
        let statement = client.prepare(sql).map_err(|err| {
            VerificationError::sql(format!(
                "failed to prepare SQL against {}: {}",
                self.config.source.env_var(),
                format_postgres_error(&err)
            ))
        })?;
        Ok(StatementMetadata::from_statement(&statement))
    }
}

pub(crate) fn parse_declared_types(expr: &Expr) -> Result<Vec<DeclaredType>, CodecDslError> {
    parse_declared_types_inner(strip_expr(expr))
}

fn parse_declared_types_inner(expr: &Expr) -> Result<Vec<DeclaredType>, CodecDslError> {
    match expr {
        Expr::Tuple(tuple) => {
            let mut types = Vec::new();
            for elem in &tuple.elems {
                types.extend(parse_declared_types_inner(strip_expr(elem))?);
            }
            Ok(types)
        }
        _ => Ok(vec![parse_declared_scalar(expr)?]),
    }
}

fn parse_declared_scalar(expr: &Expr) -> Result<DeclaredType, CodecDslError> {
    let expr = strip_expr(expr);
    if let Some(path) = as_path(expr) {
        let Some(ident) = path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
        else {
            return Err(CodecDslError::new(
                expr.span(),
                "unsupported empty codec path in verifiable codec DSL",
            ));
        };
        let (oid, name) = match ident.as_str() {
            "int2" => (21, "int2"),
            "int4" => (23, "int4"),
            "int8" => (20, "int8"),
            "bool" => (16, "bool"),
            "text" => (25, "text"),
            "varchar" => (1043, "varchar"),
            "bytea" => (17, "bytea"),
            _ => {
                return Err(CodecDslError::new(
                    expr.span(),
                    format!(
                        "unsupported verifiable codec `{ident}`; expected int2, int4, int8, bool, text, varchar, bytea, nullable(...), or tuples of these"
                    ),
                ))
            }
        };
        return Ok(DeclaredType {
            oid,
            display: name.to_owned(),
        });
    }

    let Expr::Call(ExprCall { func, args, .. }) = expr else {
        return Err(CodecDslError::new(
            expr.span(),
            "unsupported verifiable codec DSL; expected a codec path, nullable(...), or a tuple of these",
        ));
    };
    let Some(path) = as_path(func) else {
        return Err(CodecDslError::new(
            expr.span(),
            "unsupported verifiable codec DSL; expected nullable(...) around a supported codec",
        ));
    };
    let is_nullable = path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "nullable");
    if !is_nullable {
        return Err(CodecDslError::new(
            expr.span(),
            "unsupported verifiable codec call; only nullable(...) is supported",
        ));
    }
    if args.len() != 1 {
        return Err(CodecDslError::new(
            expr.span(),
            "nullable(...) in verifiable codec DSL must have exactly one argument",
        ));
    }
    let inner = parse_declared_scalar(args.first().expect("nullable args length checked"))?;
    Ok(DeclaredType {
        oid: inner.oid,
        display: format!("nullable({})", inner.display),
    })
}

pub(crate) fn verify_param_metadata(
    actual: &[TypeMetadata],
    expected: &[DeclaredType],
) -> Result<(), VerificationError> {
    verify_shape(actual, expected, ShapeKind::Parameter)
}

pub(crate) fn verify_row_metadata(
    actual: &[ColumnMetadata],
    expected: &[DeclaredType],
) -> Result<(), VerificationError> {
    if actual.len() != expected.len() {
        return Err(VerificationError::schema(format!(
            "row column count mismatch: SQL returns {} column(s) but the declared decoder expects {}",
            actual.len(),
            expected.len(),
        )));
    }

    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        if actual.type_.oid != expected.oid {
            return Err(VerificationError::schema(format!(
                "row column {} `{}` has database type {} (oid {}) but the declared decoder expects {} (oid {})",
                index + 1,
                actual.name,
                actual.type_.name,
                actual.type_.oid,
                expected.display,
                expected.oid,
            )));
        }
    }

    Ok(())
}

pub(crate) fn verify_statement_against_probe(
    sql: &str,
    params: &[DeclaredType],
    rows: Option<&[DeclaredType]>,
) -> Result<(), VerificationError> {
    let Some(probe) = Probe::discover()? else {
        return Ok(());
    };

    let statement = probe.describe(sql)?;
    verify_param_metadata(&statement.params, params)?;
    if let Some(rows) = rows {
        verify_row_metadata(&statement.columns, rows)?;
    }
    Ok(())
}

fn verify_shape(
    actual: &[TypeMetadata],
    expected: &[DeclaredType],
    kind: ShapeKind,
) -> Result<(), VerificationError> {
    if actual.len() != expected.len() {
        return Err(VerificationError::schema(format!(
            "{} count mismatch: SQL uses {} {} but the declared codec DSL expects {}",
            kind.label(),
            actual.len(),
            kind.count_noun(),
            expected.len(),
        )));
    }

    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        if actual.oid != expected.oid {
            return Err(VerificationError::schema(format!(
                "{} {} has database type {} (oid {}) but the declared codec DSL expects {} (oid {})",
                kind.label(),
                kind.item_label(index),
                actual.name,
                actual.oid,
                expected.display,
                expected.oid,
            )));
        }
    }

    Ok(())
}

fn strip_expr(mut expr: &Expr) -> &Expr {
    loop {
        expr = match expr {
            Expr::Paren(ExprParen { expr, .. }) => expr,
            Expr::Group(ExprGroup { expr, .. }) => expr,
            _ => return expr,
        };
    }
}

fn as_path(expr: &Expr) -> Option<&ExprPath> {
    let Expr::Path(path) = strip_expr(expr) else {
        return None;
    };
    Some(path)
}

fn format_postgres_error(err: &postgres::Error) -> String {
    let Some(db_error) = err.as_db_error() else {
        return err.to_string();
    };

    let mut message = db_error.message().to_owned();
    if let Some(detail) = db_error.detail() {
        message.push_str("; ");
        message.push_str(detail);
    }
    if let Some(hint) = db_error.hint() {
        message.push_str("; hint: ");
        message.push_str(hint);
    }
    if let Some(position) = db_error.position() {
        match position {
            ErrorPosition::Original(position) => {
                message.push_str(&format!("; position {position}"));
            }
            ErrorPosition::Internal { position, query } => {
                message.push_str(&format!("; internal position {position} in `{query}`"));
            }
        }
    }
    message.push_str(&format!("; SQLSTATE {}", db_error.code().code()));
    message
}

#[derive(Clone, Copy)]
enum ShapeKind {
    Parameter,
}

impl ShapeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Parameter => "parameter",
        }
    }

    fn count_noun(self) -> &'static str {
        match self {
            Self::Parameter => "parameter(s)",
        }
    }

    fn item_label(self, index: usize) -> String {
        match self {
            Self::Parameter => format!("${}", index + 1),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CodecDslError {
    span: Span,
    message: String,
}

impl CodecDslError {
    fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn into_syn_error(self) -> syn::Error {
        syn::Error::new(self.span, self.message)
    }
}

impl fmt::Display for CodecDslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerificationError {
    kind: VerificationErrorKind,
    message: String,
}

impl VerificationError {
    fn configuration(message: impl Into<String>) -> Self {
        Self {
            kind: VerificationErrorKind::Configuration,
            message: message.into(),
        }
    }

    fn connection(message: impl Into<String>) -> Self {
        Self {
            kind: VerificationErrorKind::Connection,
            message: message.into(),
        }
    }

    fn sql(message: impl Into<String>) -> Self {
        Self {
            kind: VerificationErrorKind::Sql,
            message: message.into(),
        }
    }

    fn schema(message: impl Into<String>) -> Self {
        Self {
            kind: VerificationErrorKind::Schema,
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn into_syn_error(self, span: Span) -> syn::Error {
        syn::Error::new(span, self.to_string())
    }
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "compile-time SQL verification {}: {}",
            self.kind.label(),
            self.message
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerificationErrorKind {
    Configuration,
    Connection,
    Sql,
    Schema,
}

impl VerificationErrorKind {
    fn label(self) -> &'static str {
        match self {
            Self::Configuration => "configuration error",
            Self::Connection => "connection error",
            Self::Sql => "SQL error",
            Self::Schema => "schema mismatch",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Mutex;

    use super::*;
    use syn::parse_quote;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn discovery_prefers_babar_database_url() {
        with_env(
            &[
                (
                    BABAR_DATABASE_URL_ENV,
                    Some("postgres://babar@localhost/babar"),
                ),
                (
                    DATABASE_URL_ENV,
                    Some("postgres://fallback@localhost/fallback"),
                ),
            ],
            || {
                let config = VerificationConfig::discover()
                    .expect("config discovery succeeds")
                    .expect("config is present");
                assert_eq!(config.source, ConfigSource::BabarDatabaseUrl);
                assert_eq!(config.database_url, "postgres://babar@localhost/babar");
            },
        );
    }

    #[test]
    fn blank_database_url_is_configuration_error() {
        with_env(&[(BABAR_DATABASE_URL_ENV, Some("   "))], || {
            let err = VerificationConfig::discover().expect_err("blank env should fail");
            assert_eq!(
                err,
                VerificationError::configuration("BABAR_DATABASE_URL is set but empty"),
            );
        });
    }

    #[test]
    fn parse_verifiable_codec_dsl_flattens_tuples() {
        let codecs = parse_declared_types(&parse_quote!((int4, nullable(text), (bytea, varchar))))
            .expect("codec DSL parses");
        assert_eq!(
            codecs,
            vec![
                DeclaredType {
                    oid: 23,
                    display: "int4".into(),
                },
                DeclaredType {
                    oid: 25,
                    display: "nullable(text)".into(),
                },
                DeclaredType {
                    oid: 17,
                    display: "bytea".into(),
                },
                DeclaredType {
                    oid: 1043,
                    display: "varchar".into(),
                },
            ],
        );
    }

    #[test]
    fn parameter_type_mismatch_reports_slot() {
        let err = verify_param_metadata(
            &[TypeMetadata {
                oid: 25,
                name: "text".into(),
            }],
            &[DeclaredType {
                oid: 23,
                display: "int4".into(),
            }],
        )
        .expect_err("type mismatch should fail");
        assert_eq!(
            err.to_string(),
            "compile-time SQL verification schema mismatch: parameter $1 has database type text (oid 25) but the declared codec DSL expects int4 (oid 23)"
        );
    }

    #[test]
    fn row_count_mismatch_reports_both_counts() {
        let err = verify_row_metadata(
            &[ColumnMetadata {
                name: "id".into(),
                type_: TypeMetadata {
                    oid: 23,
                    name: "int4".into(),
                },
            }],
            &[],
        )
        .expect_err("row count mismatch should fail");
        assert_eq!(
            err.to_string(),
            "compile-time SQL verification schema mismatch: row column count mismatch: SQL returns 1 column(s) but the declared decoder expects 0"
        );
    }

    fn with_env<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let saved: Vec<_> = vars
            .iter()
            .map(|(key, _)| ((*key).to_owned(), env::var_os(key)))
            .collect();

        for (key, value) in vars {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }

        let result = panic::catch_unwind(AssertUnwindSafe(f));

        for (key, value) in saved {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }

        match result {
            Ok(value) => value,
            Err(payload) => panic::resume_unwind(payload),
        }
    }
}
