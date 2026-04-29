use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

use super::resolver::{CheckedParameter, CheckedProjection, CheckedSelect, Nullability, SqlType};
use super::{ParsedSql, Result, SourceSpan, TypedSqlError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredQuery {
    pub(crate) sql: String,
    pub(crate) parameters: Vec<LoweredParameter>,
    pub(crate) columns: Vec<LoweredColumn>,
}

impl LoweredQuery {
    pub(crate) fn parameter_codec_tokens(&self) -> TokenStream {
        tuple_codec_tokens(
            self.parameters
                .iter()
                .map(|parameter| parameter.codec)
                .collect(),
        )
    }

    pub(crate) fn row_codec_tokens(&self) -> TokenStream {
        tuple_codec_tokens(self.columns.iter().map(|column| column.codec).collect())
    }

    pub(crate) fn emit_query_tokens(&self) -> TokenStream {
        let sql = LitStr::new(&self.sql, Span::call_site());
        let params = self.parameter_codec_tokens();
        let row = self.row_codec_tokens();
        let n_params = self.parameters.len();

        quote! {{
            let __babar_fragment = ::babar::query::Fragment::__from_parts(
                #sql,
                #params,
                #n_params,
                ::core::option::Option::Some(::babar::query::Origin::new(
                    file!(),
                    line!(),
                    column!(),
                )),
            );
            ::babar::query::Query::from_fragment(__babar_fragment, #row)
        }}
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredParameter {
    pub(crate) logical_name: String,
    pub(crate) position: u32,
    pub(crate) sql_type: SqlType,
    pub(crate) nullability: Nullability,
    codec: RuntimeCodec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredColumn {
    pub(crate) label: String,
    pub(crate) sql_type: SqlType,
    pub(crate) nullability: Nullability,
    codec: RuntimeCodec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeCodec {
    Bool,
    Bytea,
    Varchar,
    Text,
    Int2,
    Int4,
    Int8,
    Float4,
    Float8,
    Nullable(BaseRuntimeCodec),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BaseRuntimeCodec {
    Bool,
    Bytea,
    Varchar,
    Text,
    Int2,
    Int4,
    Int8,
    Float4,
    Float8,
}

impl RuntimeCodec {
    fn tokens(self) -> TokenStream {
        let base = match self {
            Self::Bool => return quote! { ::babar::codec::bool },
            Self::Bytea => return quote! { ::babar::codec::bytea },
            Self::Varchar => return quote! { ::babar::codec::varchar },
            Self::Text => return quote! { ::babar::codec::text },
            Self::Int2 => return quote! { ::babar::codec::int2 },
            Self::Int4 => return quote! { ::babar::codec::int4 },
            Self::Int8 => return quote! { ::babar::codec::int8 },
            Self::Float4 => return quote! { ::babar::codec::float4 },
            Self::Float8 => return quote! { ::babar::codec::float8 },
            Self::Nullable(base) => base,
        };

        let inner = base.tokens();
        quote! { ::babar::codec::nullable(#inner) }
    }
}

impl BaseRuntimeCodec {
    fn tokens(self) -> TokenStream {
        match self {
            Self::Bool => quote! { ::babar::codec::bool },
            Self::Bytea => quote! { ::babar::codec::bytea },
            Self::Varchar => quote! { ::babar::codec::varchar },
            Self::Text => quote! { ::babar::codec::text },
            Self::Int2 => quote! { ::babar::codec::int2 },
            Self::Int4 => quote! { ::babar::codec::int4 },
            Self::Int8 => quote! { ::babar::codec::int8 },
            Self::Float4 => quote! { ::babar::codec::float4 },
            Self::Float8 => quote! { ::babar::codec::float8 },
        }
    }
}

pub(crate) fn lower_select(parsed: &ParsedSql, checked: &CheckedSelect) -> Result<LoweredQuery> {
    let parameters = checked
        .parameters
        .iter()
        .map(|parameter| lower_parameter(parsed, parameter))
        .collect::<Result<Vec<_>>>()?;
    let columns = checked
        .projections
        .iter()
        .map(lower_projection)
        .collect::<Result<Vec<_>>>()?;

    Ok(LoweredQuery {
        sql: parsed.source.canonical_sql.clone(),
        parameters,
        columns,
    })
}

fn lower_parameter(parsed: &ParsedSql, parameter: &CheckedParameter) -> Result<LoweredParameter> {
    let span = parsed
        .source
        .placeholders
        .entries()
        .iter()
        .find(|entry| entry.id == parameter.id)
        .and_then(|entry| entry.occurrences.first())
        .map(|occurrence| occurrence.original_span);
    let codec = lower_runtime_codec(parameter.sql_type, parameter.nullability, span)?;
    Ok(LoweredParameter {
        logical_name: parameter.name.clone(),
        position: parameter.slot,
        sql_type: parameter.sql_type,
        nullability: parameter.nullability,
        codec,
    })
}

fn lower_projection(projection: &CheckedProjection) -> Result<LoweredColumn> {
    let codec = lower_runtime_codec(
        projection.sql_type,
        projection.nullability,
        Some(projection.expr.span),
    )?;
    Ok(LoweredColumn {
        label: projection.output_name.clone(),
        sql_type: projection.sql_type,
        nullability: projection.nullability,
        codec,
    })
}

fn lower_runtime_codec(
    sql_type: SqlType,
    nullability: Nullability,
    span: Option<SourceSpan>,
) -> Result<RuntimeCodec> {
    let base = match sql_type {
        SqlType::Bool => BaseRuntimeCodec::Bool,
        SqlType::Bytea => BaseRuntimeCodec::Bytea,
        SqlType::Varchar => BaseRuntimeCodec::Varchar,
        SqlType::Text => BaseRuntimeCodec::Text,
        SqlType::Int2 => BaseRuntimeCodec::Int2,
        SqlType::Int4 => BaseRuntimeCodec::Int4,
        SqlType::Int8 => BaseRuntimeCodec::Int8,
        SqlType::Float4 => BaseRuntimeCodec::Float4,
        SqlType::Float8 => BaseRuntimeCodec::Float8,
        unsupported => {
            return Err(TypedSqlError::unsupported_with_optional_span(
                format!(
                    "typed_sql v1 runtime lowering does not yet support SQL type `{}`; supported lowered codecs are bool, bytea, varchar, text, int2, int4, int8, float4, and float8",
                    unsupported.name()
                ),
                span,
            ));
        }
    };

    Ok(match nullability {
        Nullability::NonNull => match base {
            BaseRuntimeCodec::Bool => RuntimeCodec::Bool,
            BaseRuntimeCodec::Bytea => RuntimeCodec::Bytea,
            BaseRuntimeCodec::Varchar => RuntimeCodec::Varchar,
            BaseRuntimeCodec::Text => RuntimeCodec::Text,
            BaseRuntimeCodec::Int2 => RuntimeCodec::Int2,
            BaseRuntimeCodec::Int4 => RuntimeCodec::Int4,
            BaseRuntimeCodec::Int8 => RuntimeCodec::Int8,
            BaseRuntimeCodec::Float4 => RuntimeCodec::Float4,
            BaseRuntimeCodec::Float8 => RuntimeCodec::Float8,
        },
        Nullability::Nullable => RuntimeCodec::Nullable(base),
    })
}

fn tuple_codec_tokens(codecs: Vec<RuntimeCodec>) -> TokenStream {
    let tokens = codecs
        .into_iter()
        .map(RuntimeCodec::tokens)
        .collect::<Vec<_>>();
    quote! { (#(#tokens,)*) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_sql::parse_select;
    use crate::typed_sql::resolver::{resolve_select, SchemaCatalog, SchemaColumn, SchemaTable};
    use crate::typed_sql::TypedSqlErrorKind;

    fn fixture_catalog() -> SchemaCatalog {
        SchemaCatalog::new(vec![
            SchemaTable::new(
                Some("public"),
                "users",
                vec![
                    SchemaColumn::new("id", SqlType::Int4, Nullability::NonNull),
                    SchemaColumn::new("score", SqlType::Float8, Nullability::Nullable),
                    SchemaColumn::new("name", SqlType::Text, Nullability::NonNull),
                ],
            )
            .expect("users table"),
            SchemaTable::new(
                Some("public"),
                "pets",
                vec![
                    SchemaColumn::new("id", SqlType::Int4, Nullability::NonNull),
                    SchemaColumn::new("owner_id", SqlType::Int4, Nullability::NonNull),
                    SchemaColumn::new("name", SqlType::Text, Nullability::NonNull),
                    SchemaColumn::new("deleted_at", SqlType::Timestamptz, Nullability::Nullable),
                ],
            )
            .expect("pets table"),
        ])
        .expect("catalog")
    }

    fn parse_resolve_and_lower(sql: &str) -> Result<LoweredQuery> {
        let parsed = parse_select(sql)?;
        let checked = resolve_select(&parsed.select, &fixture_catalog())?;
        lower_select(&parsed, &checked)
    }

    fn normalize_tokens(tokens: impl ToString) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn lowers_supported_select_into_runtime_query_layout() {
        let lowered = parse_resolve_and_lower(
            "SELECT u.id, p.name AS pet_name, u.score FROM users AS u LEFT JOIN pets AS p ON p.owner_id = u.id WHERE u.id = $id OR u.id = $id ORDER BY p.name LIMIT $limit OFFSET 4",
        )
        .expect("query lowers");

        assert_eq!(
            lowered.sql,
            "SELECT u.id, p.name AS pet_name, u.score FROM users AS u LEFT JOIN pets AS p ON p.owner_id = u.id WHERE u.id = $1 OR u.id = $1 ORDER BY p.name LIMIT $2 OFFSET 4"
        );
        assert_eq!(lowered.parameters.len(), 2);
        assert_eq!(lowered.parameters[0].logical_name, "id");
        assert_eq!(lowered.parameters[0].position, 1);
        assert_eq!(lowered.parameters[0].sql_type, SqlType::Int4);
        assert_eq!(lowered.parameters[1].logical_name, "limit");
        assert_eq!(lowered.parameters[1].position, 2);
        assert_eq!(lowered.parameters[1].sql_type, SqlType::Int8);

        assert_eq!(lowered.columns.len(), 3);
        assert_eq!(lowered.columns[0].label, "id");
        assert_eq!(lowered.columns[0].nullability, Nullability::NonNull);
        assert_eq!(lowered.columns[1].label, "pet_name");
        assert_eq!(lowered.columns[1].nullability, Nullability::Nullable);
        assert_eq!(lowered.columns[2].label, "score");
        assert_eq!(lowered.columns[2].nullability, Nullability::Nullable);

        assert_eq!(
            normalize_tokens(lowered.parameter_codec_tokens()),
            "(:: babar :: codec :: int4 , :: babar :: codec :: int8 ,)"
        );
        assert_eq!(
            normalize_tokens(lowered.row_codec_tokens()),
            "(:: babar :: codec :: int4 , :: babar :: codec :: nullable (:: babar :: codec :: text) , :: babar :: codec :: nullable (:: babar :: codec :: float8) ,)"
        );

        let tokens = lowered.emit_query_tokens().to_string();
        assert!(tokens.contains(":: babar :: query :: Query :: from_fragment"));
        assert!(tokens.contains("\"SELECT u.id, p.name AS pet_name, u.score FROM users AS u LEFT JOIN pets AS p ON p.owner_id = u.id WHERE u.id = $1 OR u.id = $1 ORDER BY p.name LIMIT $2 OFFSET 4\""));
    }

    #[test]
    fn rejects_projection_types_without_a_lowered_runtime_codec() {
        let err = parse_resolve_and_lower(
            "SELECT p.deleted_at FROM users AS u INNER JOIN pets AS p ON p.owner_id = u.id",
        )
        .expect_err("unsupported projection type should fail lowering");

        assert_eq!(err.kind, TypedSqlErrorKind::Unsupported);
        assert!(err
            .message
            .contains("runtime lowering does not yet support SQL type `timestamptz`"));
    }

    #[test]
    fn rejects_parameter_types_without_a_lowered_runtime_codec() {
        let err = parse_resolve_and_lower(
            "SELECT u.id FROM users AS u INNER JOIN pets AS p ON p.owner_id = u.id WHERE p.deleted_at = $deleted_at",
        )
        .expect_err("unsupported parameter type should fail lowering");

        assert_eq!(err.kind, TypedSqlErrorKind::Unsupported);
        assert!(err
            .message
            .contains("runtime lowering does not yet support SQL type `timestamptz`"));
        assert!(err.span.is_some());
    }
}
