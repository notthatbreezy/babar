use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{braced, parenthesized, parse_macro_input, token, Error, Ident, Result, Token};

use super::lower;
use super::public_input::PublicSqlInput;
use super::resolver::{self, Nullability, SchemaCatalog, SchemaColumn, SchemaTable, SqlType};
use super::{TypedSqlError, TypedSqlErrorKind};

mod kw {
    syn::custom_keyword!(schema);
    syn::custom_keyword!(__babar_schema);
    syn::custom_keyword!(table);
}

pub(crate) fn expand_typed_query(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as TypedQueryInput);
    match compile_typed_query(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn compile_typed_query(input: TypedQueryInput) -> Result<proc_macro2::TokenStream> {
    let TypedQueryInput {
        source_kind,
        schema,
        sql,
    } = input;
    let catalog = schema.into_catalog()?;
    let parsed = sql.parse_select()?;
    let checked = resolver::resolve_select(&parsed.select, &catalog)
        .map_err(|err| sql.syn_error(decorate_error(source_kind, err)))?;
    let lowered = lower::lower_select(&parsed, &checked)
        .map_err(|err| sql.syn_error(decorate_error(source_kind, err)))?;
    Ok(lowered.emit_query_tokens())
}

struct TypedQueryInput {
    source_kind: SchemaSourceKind,
    schema: SchemaInput,
    sql: PublicSqlInput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaSourceKind {
    Inline,
    AuthoredBridge,
}

impl Parse for TypedQueryInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let source_kind = if input.peek(kw::schema) {
            input.parse::<kw::schema>()?;
            SchemaSourceKind::Inline
        } else if input.peek(kw::__babar_schema) {
            input.parse::<kw::__babar_schema>()?;
            SchemaSourceKind::AuthoredBridge
        } else {
            return Err(input.error("expected `schema = { ... }` before typed_query SQL"));
        };
        input.parse::<Token![=]>()?;
        let schema = input.parse()?;
        input.parse::<Token![,]>()?;
        let sql_tokens = trim_optional_trailing_comma(input.parse::<TokenStream2>()?);
        reject_extra_schema_argument(&sql_tokens, source_kind)?;
        let sql = PublicSqlInput::parse(sql_tokens)?;
        Ok(Self {
            source_kind,
            schema,
            sql,
        })
    }
}

fn decorate_error(source_kind: SchemaSourceKind, mut err: TypedSqlError) -> TypedSqlError {
    if source_kind == SchemaSourceKind::AuthoredBridge {
        match err.kind {
            TypedSqlErrorKind::Resolve => {
                err.message = format!("authored external schema lookup failed: {}", err.message);
            }
            TypedSqlErrorKind::Unsupported
                if err
                    .message
                    .contains("runtime lowering does not yet support SQL type") =>
            {
                err.message = format!(
                    "authored external schema declarations can express this type, but {}",
                    err.message
                );
            }
            _ => {}
        }
    }
    err
}

fn trim_optional_trailing_comma(tokens: TokenStream2) -> TokenStream2 {
    let mut tokens = tokens.into_iter().collect::<Vec<_>>();
    let trailing_comma = matches!(
        tokens.last(),
        Some(proc_macro2::TokenTree::Punct(punct)) if punct.as_char() == ','
    );
    if trailing_comma {
        tokens.pop();
    }
    tokens.into_iter().collect()
}

fn reject_extra_schema_argument(
    tokens: &TokenStream2,
    source_kind: SchemaSourceKind,
) -> Result<()> {
    let mut tokens = tokens.clone().into_iter();
    let Some(TokenTree::Ident(ident)) = tokens.next() else {
        return Ok(());
    };
    let name = ident_name(&ident);
    if name != "schema" && name != "__babar_schema" {
        return Ok(());
    }
    let Some(TokenTree::Punct(punct)) = tokens.next() else {
        return Ok(());
    };
    if punct.as_char() != '=' {
        return Ok(());
    }

    let message = match (source_kind, name.as_str()) {
        (SchemaSourceKind::AuthoredBridge, "schema") => {
            "schema-scoped authored `typed_query!` already supplies the external schema; inline `schema = { ... }` blocks cannot be mixed into this call"
        }
        (SchemaSourceKind::AuthoredBridge, "__babar_schema") => {
            "schema-scoped authored `typed_query!` already supplies its internal schema bridge; do not pass `__babar_schema = { ... }` manually"
        }
        (SchemaSourceKind::Inline, "__babar_schema") => {
            "cannot mix inline `schema = { ... }` with the authored external schema bridge `__babar_schema = { ... }` in one typed_query! call"
        }
        (SchemaSourceKind::Inline, "schema") => {
            "typed_query! accepts only one `schema = { ... }` block before the SQL input"
        }
        _ => unreachable!("schema argument name was validated above"),
    };

    Err(Error::new(ident.span(), message))
}

struct SchemaInput {
    brace_token: token::Brace,
    tables: Vec<SchemaTableInput>,
}

impl Parse for SchemaInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let content;
        let brace_token = braced!(content in input);
        let mut tables = Vec::new();
        while !content.is_empty() {
            tables.push(content.parse()?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        Ok(Self {
            brace_token,
            tables,
        })
    }
}

impl SchemaInput {
    fn into_catalog(self) -> Result<SchemaCatalog> {
        let mut seen_tables = HashMap::<String, proc_macro2::Span>::new();
        let mut tables = Vec::with_capacity(self.tables.len());

        for table in self.tables {
            let qualified_name = table.qualified_name();
            if let Some(previous) = seen_tables.insert(qualified_name.clone(), table.name.span()) {
                let mut err = Error::new(
                    table.name.span(),
                    format!("duplicate table `{qualified_name}` in typed_query schema"),
                );
                err.combine(Error::new(previous, "first defined here"));
                return Err(err);
            }

            let SchemaTableInput {
                schema_name,
                name,
                columns: table_columns,
            } = table;
            let display_name = qualified_name.clone();
            let schema_name = schema_name.as_ref().map(ident_name);
            let table_name = ident_name(&name);
            let mut seen_columns = HashMap::<String, proc_macro2::Span>::new();
            let mut columns = Vec::with_capacity(table_columns.len());
            for column in table_columns {
                let column_name = ident_name(&column.name);
                if let Some(previous) = seen_columns.insert(column_name.clone(), column.name.span())
                {
                    let mut err = Error::new(
                        column.name.span(),
                        format!(
                            "duplicate column `{column_name}` in typed_query schema table `{display_name}`"
                        ),
                    );
                    err.combine(Error::new(previous, "first defined here"));
                    return Err(err);
                }
                columns.push(column.into_schema_column()?);
            }

            tables.push(
                SchemaTable::new(schema_name.as_deref(), &table_name, columns)
                    .map_err(|err| Error::new(name.span(), err.to_string()))?,
            );
        }

        SchemaCatalog::new(tables).map_err(|err| Error::new(self.brace_token.span.open(), err))
    }
}

struct SchemaTableInput {
    schema_name: Option<Ident>,
    name: Ident,
    columns: Vec<SchemaColumnInput>,
}

impl SchemaTableInput {
    fn qualified_name(&self) -> String {
        match &self.schema_name {
            Some(schema) => format!("{}.{}", ident_name(schema), ident_name(&self.name)),
            None => ident_name(&self.name),
        }
    }
}

impl Parse for SchemaTableInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::table>()?;
        let first = Ident::parse_any(input)?;
        let (schema_name, name) = if input.peek(Token![.]) {
            input.parse::<Token![.]>()?;
            (Some(first), Ident::parse_any(input)?)
        } else {
            (None, first)
        };

        let content;
        braced!(content in input);
        let mut columns = Vec::new();
        while !content.is_empty() {
            columns.push(content.parse()?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }

        Ok(Self {
            schema_name,
            name,
            columns,
        })
    }
}

struct SchemaColumnInput {
    name: Ident,
    sql_type: SchemaColumnTypeInput,
}

impl SchemaColumnInput {
    fn into_schema_column(self) -> Result<SchemaColumn> {
        let (sql_type, nullability) = self.sql_type.resolve()?;
        let name = ident_name(&self.name);
        Ok(SchemaColumn::new(&name, sql_type, nullability))
    }
}

impl Parse for SchemaColumnInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        Ok(Self {
            name: Ident::parse_any(input)?,
            sql_type: {
                input.parse::<Token![:]>()?;
                input.parse()?
            },
        })
    }
}

enum SchemaColumnTypeInput {
    Base(Ident),
    Nullable { inner: Ident },
}

impl SchemaColumnTypeInput {
    fn resolve(self) -> Result<(SqlType, Nullability)> {
        match self {
            Self::Base(name) => Ok((resolve_sql_type(&name)?, Nullability::NonNull)),
            Self::Nullable { inner } => {
                let sql_type = resolve_sql_type(&inner)?;
                Ok((sql_type, Nullability::Nullable))
            }
        }
    }
}

impl Parse for SchemaColumnTypeInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name = Ident::parse_any(input)?;
        if name == "nullable" && input.peek(token::Paren) {
            let content;
            parenthesized!(content in input);
            let inner = Ident::parse_any(&content)?;
            if !content.is_empty() {
                return Err(content.error("expected exactly one SQL type inside nullable(...)"));
            }
            return Ok(Self::Nullable { inner });
        }
        Ok(Self::Base(name))
    }
}

fn resolve_sql_type(name: &Ident) -> Result<SqlType> {
    let resolved = ident_name(name);
    match resolved.as_str() {
        "bool" => Ok(SqlType::Bool),
        "bytea" => Ok(SqlType::Bytea),
        "varchar" => Ok(SqlType::Varchar),
        "text" => Ok(SqlType::Text),
        "int2" => Ok(SqlType::Int2),
        "int4" => Ok(SqlType::Int4),
        "int8" => Ok(SqlType::Int8),
        "float4" => Ok(SqlType::Float4),
        "float8" => Ok(SqlType::Float8),
        "uuid" => Ok(SqlType::Uuid),
        "date" => Ok(SqlType::Date),
        "time" => Ok(SqlType::Time),
        "timestamp" => Ok(SqlType::Timestamp),
        "timestamptz" => Ok(SqlType::Timestamptz),
        "json" => Ok(SqlType::Json),
        "jsonb" => Ok(SqlType::Jsonb),
        "numeric" => Ok(SqlType::Numeric),
        other => Err(Error::new(
            name.span(),
            format!(
                "unsupported typed_query schema type `{other}`; supported types are bool, bytea, varchar, text, int2, int4, int8, float4, float8, uuid, date, time, timestamp, timestamptz, json, jsonb, and numeric"
            ),
        )),
    }
}

fn ident_name(ident: &Ident) -> String {
    ident.unraw().to_string()
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse2;

    use super::TypedQueryInput;

    #[test]
    fn typed_query_accepts_trailing_comma_after_sql_tokens() {
        parse2::<TypedQueryInput>(quote! {
            schema = {
                table public.users {
                    id: int4,
                    name: text,
                },
            },
            SELECT users.id, users.name FROM users,
        })
        .expect("typed_query input parses with trailing comma");
    }

    #[test]
    fn typed_query_accepts_authored_schema_bridge() {
        parse2::<TypedQueryInput>(quote! {
            __babar_schema = {
                table public.users {
                    id: int4,
                    name: text,
                },
            },
            SELECT users.id, users.name FROM users WHERE users.id = $id
        })
        .expect("typed_query input parses with authored bridge");
    }

    #[test]
    fn typed_query_rejects_inline_schema_after_authored_bridge() {
        let err = match parse2::<TypedQueryInput>(quote! {
            __babar_schema = {
                table public.users {
                    id: int4,
                },
            },
            schema = {
                table public.users {
                    id: int4,
                },
            },
            SELECT users.id FROM users
        }) {
            Ok(_) => panic!("mixed schema arguments should be rejected"),
            Err(err) => err,
        };

        assert!(err.to_string().contains(
            "schema-scoped authored `typed_query!` already supplies the external schema"
        ));
    }

    #[test]
    fn typed_query_rejects_authored_bridge_after_inline_schema() {
        let err = match parse2::<TypedQueryInput>(quote! {
            schema = {
                table public.users {
                    id: int4,
                },
            },
            __babar_schema = {
                table public.users {
                    id: int4,
                },
            },
            SELECT users.id FROM users
        }) {
            Ok(_) => panic!("duplicate schema arguments should be rejected"),
            Err(err) => err,
        };

        assert!(err.to_string().contains(
            "cannot mix inline `schema = { ... }` with the authored external schema bridge"
        ));
    }
}
