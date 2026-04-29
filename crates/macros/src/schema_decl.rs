use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{braced, parenthesized, parse_macro_input, Error, Ident, Result, Token, Visibility};

mod kw {
    syn::custom_keyword!(table);
}

pub(crate) fn expand_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SchemaModuleInput);
    match compile_schema_module(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn compile_schema_module(input: SchemaModuleInput) -> Result<TokenStream2> {
    let SchemaModuleInput { vis, name, tables } = input;
    let mut seen_tables = HashMap::<String, proc_macro2::Span>::new();
    let mut seen_modules = HashMap::<String, proc_macro2::Span>::new();
    let mut table_modules = Vec::with_capacity(tables.len());
    let mut table_defs = Vec::with_capacity(tables.len());

    for table in tables {
        let qualified_name = table.qualified_name();
        if let Some(previous) = seen_tables.insert(qualified_name.clone(), table.name.span()) {
            let mut err = Error::new(
                table.name.span(),
                format!("duplicate table `{qualified_name}` in authored schema module"),
            );
            err.combine(Error::new(previous, "first defined here"));
            return Err(err);
        }

        let symbol_name = ident_name(&table.name);
        if let Some(previous) = seen_modules.insert(symbol_name.clone(), table.name.span()) {
            let mut err = Error::new(
                table.name.span(),
                format!(
                    "duplicate authored table symbol `{symbol_name}`; table modules use unqualified table names"
                ),
            );
            err.combine(Error::new(previous, "first defined here"));
            return Err(err);
        }

        let module_ident = table.name.clone();
        table_modules.push(table.expand()?);
        table_defs.push(quote! { #module_ident::DEF });
    }

    Ok(quote! {
        #vis mod #name {
            #( #table_modules )*

            pub const TABLES: &[::babar::schema::TableDef] = &[#(#table_defs),*];
            pub const SCHEMA: ::babar::schema::SchemaDef =
                ::babar::schema::SchemaDef::new(TABLES);
        }
    })
}

struct SchemaModuleInput {
    vis: Visibility,
    name: Ident,
    tables: Vec<SchemaTableInput>,
}

impl Parse for SchemaModuleInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let vis = input.parse()?;
        input.parse::<Token![mod]>()?;
        let name = Ident::parse_any(input)?;
        let content;
        braced!(content in input);

        let mut tables = Vec::new();
        while !content.is_empty() {
            tables.push(content.parse()?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }

        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected trailing tokens in schema!"));
        }

        Ok(Self { vis, name, tables })
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

    fn expand(self) -> Result<TokenStream2> {
        let Self {
            schema_name,
            name,
            columns,
        } = self;
        let table_name = ident_name(&name);
        let schema_name_tokens = schema_name.as_ref().map_or_else(
            || quote! { ::core::option::Option::None },
            |schema| {
                let value = ident_name(schema);
                let literal = syn::LitStr::new(&value, schema.span());
                quote! { ::core::option::Option::Some(#literal) }
            },
        );
        let table_name_literal = syn::LitStr::new(&table_name, name.span());
        let module_ident = name;
        let marker_ident = format_ident!("Table", span = module_ident.span());
        let mut seen_columns = HashMap::<String, proc_macro2::Span>::new();
        let mut column_items = Vec::with_capacity(columns.len());
        let mut column_defs = Vec::with_capacity(columns.len());

        for column in columns {
            let column_name = ident_name(&column.name);
            if let Some(previous) = seen_columns.insert(column_name.clone(), column.name.span()) {
                let mut err = Error::new(
                    column.name.span(),
                    format!(
                        "duplicate column `{column_name}` in authored schema table `{table_name}`"
                    ),
                );
                err.combine(Error::new(previous, "first defined here"));
                return Err(err);
            }

            let const_ident = format_ident!(
                "{}",
                column_name.to_ascii_uppercase(),
                span = column.name.span()
            );
            column_items.push(column.expand_symbol(&marker_ident, &const_ident)?);
            column_defs.push(column.expand_definition()?);
        }

        Ok(quote! {
            pub mod #module_ident {
                pub enum #marker_ident {}

                pub const TABLE: ::babar::schema::TableRef<#marker_ident> =
                    ::babar::schema::TableRef::new(#schema_name_tokens, #table_name_literal);

                #(#column_items)*

                pub const COLUMNS: &[::babar::schema::ColumnDef] = &[#(#column_defs),*];
                pub const DEF: ::babar::schema::TableDef =
                    ::babar::schema::TableDef::new(#schema_name_tokens, #table_name_literal, COLUMNS);
            }
        })
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
    fn expand_symbol(&self, marker_ident: &Ident, const_ident: &Ident) -> Result<TokenStream2> {
        let column_ident = &self.name;
        let column_name = ident_name(column_ident);
        let column_name_literal = syn::LitStr::new(&column_name, column_ident.span());
        let sql_type = self.sql_type.sql_type_tokens()?;
        let nullability = self.sql_type.nullability_tokens();
        let semantics = self.sql_type.semantics_tokens();

        Ok(quote! {
            pub const #const_ident: ::babar::schema::Column<#marker_ident> =
                ::babar::schema::Column::with_semantics(
                    TABLE,
                    #column_name_literal,
                    #sql_type,
                    #nullability,
                    #semantics,
                );

            pub const fn #column_ident() -> ::babar::schema::Column<#marker_ident> {
                #const_ident
            }
        })
    }

    fn expand_definition(&self) -> Result<TokenStream2> {
        let column_name = ident_name(&self.name);
        let column_name_literal = syn::LitStr::new(&column_name, self.name.span());
        let sql_type = self.sql_type.sql_type_tokens()?;
        let nullability = self.sql_type.nullability_tokens();
        let semantics = self.sql_type.semantics_tokens();

        Ok(quote! {
            ::babar::schema::ColumnDef::with_semantics(
                #column_name_literal,
                #sql_type,
                #nullability,
                #semantics,
            )
        })
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

#[derive(Clone, Copy)]
enum SchemaFieldNullability {
    NonNull,
    Nullable,
}

#[derive(Clone, Copy)]
enum SchemaFieldSemantics {
    Ordinary,
    PrimaryKey,
}

struct SchemaColumnTypeInput {
    sql_type: Ident,
    nullability: SchemaFieldNullability,
    semantics: SchemaFieldSemantics,
}

impl SchemaColumnTypeInput {
    fn sql_type_tokens(&self) -> Result<TokenStream2> {
        Ok(match ident_name(&self.sql_type).as_str() {
            "bool" => quote! { ::babar::schema::SqlType::BOOL },
            "bytea" => quote! { ::babar::schema::SqlType::BYTEA },
            "varchar" => quote! { ::babar::schema::SqlType::VARCHAR },
            "text" => quote! { ::babar::schema::SqlType::TEXT },
            "int2" => quote! { ::babar::schema::SqlType::INT2 },
            "int4" => quote! { ::babar::schema::SqlType::INT4 },
            "int8" => quote! { ::babar::schema::SqlType::INT8 },
            "float4" => quote! { ::babar::schema::SqlType::FLOAT4 },
            "float8" => quote! { ::babar::schema::SqlType::FLOAT8 },
            "uuid" => quote! { ::babar::schema::SqlType::UUID },
            "date" => quote! { ::babar::schema::SqlType::DATE },
            "time" => quote! { ::babar::schema::SqlType::TIME },
            "timestamp" => quote! { ::babar::schema::SqlType::TIMESTAMP },
            "timestamptz" => quote! { ::babar::schema::SqlType::TIMESTAMPTZ },
            "json" => quote! { ::babar::schema::SqlType::JSON },
            "jsonb" => quote! { ::babar::schema::SqlType::JSONB },
            "numeric" => quote! { ::babar::schema::SqlType::NUMERIC },
            other => {
                return Err(Error::new(
                    self.sql_type.span(),
                    format!(
                        "unsupported authored schema type `{other}`; supported types are bool, bytea, varchar, text, int2, int4, int8, float4, float8, uuid, date, time, timestamp, timestamptz, json, jsonb, and numeric"
                    ),
                ))
            }
        })
    }

    fn nullability_tokens(&self) -> TokenStream2 {
        match self.nullability {
            SchemaFieldNullability::NonNull => {
                quote! { ::babar::schema::Nullability::NonNull }
            }
            SchemaFieldNullability::Nullable => {
                quote! { ::babar::schema::Nullability::Nullable }
            }
        }
    }

    fn semantics_tokens(&self) -> TokenStream2 {
        match self.semantics {
            SchemaFieldSemantics::Ordinary => {
                quote! { ::babar::schema::ColumnSemantics::Ordinary }
            }
            SchemaFieldSemantics::PrimaryKey => {
                quote! { ::babar::schema::ColumnSemantics::PrimaryKey }
            }
        }
    }
}

impl Parse for SchemaColumnTypeInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let parsed = parse_column_type(input)?;
        if matches!(parsed.semantics, SchemaFieldSemantics::PrimaryKey)
            && matches!(parsed.nullability, SchemaFieldNullability::Nullable)
        {
            return Err(Error::new(
                parsed.sql_type.span(),
                "primary_key(...) columns cannot be nullable",
            ));
        }
        Ok(parsed)
    }
}

fn parse_column_type(input: ParseStream<'_>) -> Result<SchemaColumnTypeInput> {
    let name = Ident::parse_any(input)?;
    if !input.peek(syn::token::Paren) {
        return Ok(SchemaColumnTypeInput {
            sql_type: name,
            nullability: SchemaFieldNullability::NonNull,
            semantics: SchemaFieldSemantics::Ordinary,
        });
    }

    let content;
    parenthesized!(content in input);
    let mut inner = parse_column_type(&content)?;
    if !content.is_empty() {
        return Err(content.error("expected exactly one schema type inside marker(...)"));
    }

    match ident_name(&name).as_str() {
        "nullable" => {
            if matches!(inner.nullability, SchemaFieldNullability::Nullable) {
                return Err(Error::new(name.span(), "duplicate nullable(...) marker"));
            }
            inner.nullability = SchemaFieldNullability::Nullable;
            Ok(inner)
        }
        "primary_key" | "pk" => {
            if matches!(inner.semantics, SchemaFieldSemantics::PrimaryKey) {
                return Err(Error::new(name.span(), "duplicate primary_key(...) marker"));
            }
            inner.semantics = SchemaFieldSemantics::PrimaryKey;
            Ok(inner)
        }
        other => Err(Error::new(
            name.span(),
            format!(
                "unsupported authored schema field marker `{other}`; supported markers are nullable(...), primary_key(...), and pk(...)"
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

    use super::SchemaModuleInput;

    #[test]
    fn schema_module_accepts_multiple_tables_and_markers() {
        parse2::<SchemaModuleInput>(quote! {
            pub mod app_schema {
                table public.users {
                    id: primary_key(int4),
                    name: text,
                    deleted_at: nullable(timestamptz),
                },
                table posts {
                    id: pk(int8),
                    author_id: int4,
                },
            }
        })
        .expect("schema module parses");
    }
}
