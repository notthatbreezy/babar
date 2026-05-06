use std::collections::{BTreeMap, BTreeSet, HashMap};

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{braced, parenthesized, token, Error, Ident, Result, Token, Type};

use super::lower;
use super::lower::LoweredQuery;
use super::parse_typed_sql_source;
use super::public_input::PublicSqlInput;
use super::resolver::{
    self, CheckedSelect, CheckedStatement, CheckedStatementBody, Nullability, SchemaCatalog,
    SchemaColumn, SchemaTable, SqlType,
};
use super::{
    ParsedExpr, ParsedSql, StatementKind, StatementResultKind, TypedSqlError, TypedSqlErrorKind,
};
use crate::verify::{
    declared_type_for_sql_type, verify_typed_statement_against_probe, ReferencedColumn,
    ReferencedTable,
};

mod kw {
    syn::custom_keyword!(params);
    syn::custom_keyword!(row);
    syn::custom_keyword!(schema);
    syn::custom_keyword!(__babar_schema);
    syn::custom_keyword!(table);
}

pub(crate) fn expand_query(input: TokenStream) -> TokenStream {
    let input = match syn::parse::<TypedQueryInput>(input) {
        Ok(input) => input,
        Err(err) => {
            return rewrite_entrypoint_error(err, MacroEntrypoint::Query)
                .into_compile_error()
                .into()
        }
    };
    match compile_typed_query(input, MacroEntrypoint::Query) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

pub(crate) fn expand_command(input: TokenStream) -> TokenStream {
    let input = match syn::parse::<TypedQueryInput>(input) {
        Ok(input) => input,
        Err(err) => {
            return rewrite_entrypoint_error(err, MacroEntrypoint::Command)
                .into_compile_error()
                .into()
        }
    };
    match compile_typed_query(input, MacroEntrypoint::Command) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MacroEntrypoint {
    Query,
    Command,
}

impl MacroEntrypoint {
    fn name(self) -> &'static str {
        match self {
            Self::Query => "query!",
            Self::Command => "command!",
        }
    }
}

fn compile_typed_query(
    input: TypedQueryInput,
    entrypoint: MacroEntrypoint,
) -> Result<proc_macro2::TokenStream> {
    let front_door = input.into_front_door()?;
    let parsed = front_door.parse_with(parse_typed_sql_source)?;
    let checked = resolver::resolve_statement(&parsed.statement, front_door.catalog())
        .map_err(|err| front_door.syn_error(err))?;
    enforce_entrypoint_contract(&front_door, &checked, entrypoint)?;
    let lowered =
        lower::lower_statement(&parsed, &checked).map_err(|err| front_door.syn_error(err))?;
    if let (CheckedStatementBody::Select(select), Some(parsed_select)) =
        (&checked.body, parsed.select.as_ref())
    {
        verify_live_schema(&front_door, &parsed, select, &lowered)
            .map_err(|err| front_door.sql.syn_error_message(err))?;
        debug_assert_eq!(parsed_select.projections.len(), select.projections.len());
    }
    Ok(match checked.result {
        StatementResultKind::Rows => emit_query_tokens(&front_door, &lowered)?,
        StatementResultKind::Command => emit_command_tokens(&front_door, &lowered)?,
    })
}

fn emit_query_tokens(
    front_door: &TypedSqlFrontDoor,
    lowered: &LoweredQuery,
) -> Result<TokenStream2> {
    if lowered.is_dynamic() && !matches!(front_door.selections.params, TypedSqlSelection::Default) {
        return Err(front_door.sql.syn_error_message(
            "`params = ...` selection is not yet supported for typed SQL statements with optional placeholders or toggle groups",
        ));
    }

    let infer_params = matches!(front_door.selections.params, TypedSqlSelection::Infer);
    let infer_row = matches!(front_door.selections.row, TypedSqlSelection::Infer);
    enforce_strict_row_selection(front_door, lowered)?;

    if !infer_params && !infer_row {
        return Ok(lowered.emit_query_tokens_with(
            selection_parameter_codec_tokens(&front_door.selections.params, lowered)?,
            selection_row_codec_tokens(&front_door.selections.row, lowered)?,
        ));
    }

    emit_query_selector(lowered, &front_door.selections)
}

fn emit_command_tokens(
    front_door: &TypedSqlFrontDoor,
    lowered: &LoweredQuery,
) -> Result<TokenStream2> {
    if !matches!(front_door.selections.row, TypedSqlSelection::Default) {
        return Err(front_door
            .sql
            .syn_error_message("`row = Type` is only supported for row-producing typed SQL"));
    }
    if lowered.is_dynamic() && !matches!(front_door.selections.params, TypedSqlSelection::Default) {
        return Err(front_door.sql.syn_error_message(
            "`params = ...` selection is not yet supported for typed SQL statements with optional placeholders or toggle groups",
        ));
    }

    if matches!(front_door.selections.params, TypedSqlSelection::Infer) {
        emit_command_selector(lowered, &front_door.selections.params)
    } else {
        Ok(
            lowered.emit_command_tokens_with(selection_parameter_codec_tokens(
                &front_door.selections.params,
                lowered,
            )?),
        )
    }
}

fn emit_query_selector(
    lowered: &LoweredQuery,
    selections: &TypedSqlSelections,
) -> Result<TokenStream2> {
    let mut generics = Vec::new();
    let mut bounds = Vec::new();
    let selected_params_type = match &selections.params {
        TypedSqlSelection::Default => lowered.parameter_type_tokens(),
        TypedSqlSelection::Explicit(ty) => {
            let ty = ty.as_ref();
            quote::quote! { #ty }
        }
        TypedSqlSelection::Infer => {
            generics.push(quote::quote! { __BabarArgs });
            bounds.push(quote::quote! { __BabarArgs: ::babar::__private::StaticCodec });
            quote::quote! { __BabarArgs }
        }
    };
    let selected_row_type = match &selections.row {
        TypedSqlSelection::Default => lowered.row_type_tokens(),
        TypedSqlSelection::Explicit(ty) => {
            let ty = ty.as_ref();
            quote::quote! { #ty }
        }
        TypedSqlSelection::Infer => {
            generics.push(quote::quote! { __BabarRow });
            bounds.push(quote::quote! { __BabarRow: ::babar::__private::StaticCodec });
            quote::quote! { __BabarRow }
        }
    };
    let selected_params_codec = match &selections.params {
        TypedSqlSelection::Infer => {
            quote::quote! { <__BabarArgs as ::babar::__private::StaticCodec>::codec() }
        }
        _ => selection_parameter_codec_tokens(&selections.params, lowered)?,
    };
    let selected_row_codec = match &selections.row {
        TypedSqlSelection::Infer => {
            quote::quote! { <__BabarRow as ::babar::__private::StaticCodec>::codec() }
        }
        _ => selection_row_codec_tokens(&selections.row, lowered)?,
    };
    let selected_expr = lowered.emit_query_tokens_with(selected_params_codec, selected_row_codec);
    let impl_generics = if generics.is_empty() {
        quote::quote! {}
    } else {
        quote::quote! { <#(#generics,)*> }
    };
    let where_clause = if bounds.is_empty() {
        quote::quote! {}
    } else {
        quote::quote! { where #(#bounds,)* }
    };

    Ok(quote::quote! {{
        trait __BabarTypedSqlSelection {
            fn __babar_select() -> Self;
        }

        impl #impl_generics __BabarTypedSqlSelection for ::babar::query::Query<#selected_params_type, #selected_row_type> #where_clause {
            fn __babar_select() -> Self {
                #selected_expr
            }
        }

        fn __babar_select<T>() -> T
        where
            T: __BabarTypedSqlSelection,
        {
            T::__babar_select()
        }

        __babar_select()
    }})
}

fn emit_command_selector(
    lowered: &LoweredQuery,
    selection: &TypedSqlSelection,
) -> Result<TokenStream2> {
    let (impl_generics, where_clause, selected_type, selected_codec) = match selection {
        TypedSqlSelection::Infer => (
            quote::quote! { <__BabarArgs> },
            quote::quote! { where __BabarArgs: ::babar::__private::StaticCodec },
            quote::quote! { __BabarArgs },
            quote::quote! { <__BabarArgs as ::babar::__private::StaticCodec>::codec() },
        ),
        TypedSqlSelection::Default => (
            quote::quote! {},
            quote::quote! {},
            lowered.parameter_type_tokens(),
            lowered.parameter_codec_tokens(),
        ),
        TypedSqlSelection::Explicit(ty) => {
            let ty = ty.as_ref();
            (
                quote::quote! {},
                quote::quote! {},
                quote::quote! { #ty },
                explicit_parameter_codec_tokens(ty, lowered)?,
            )
        }
    };
    let selected_expr = lowered.emit_command_tokens_with(selected_codec);

    Ok(quote::quote! {{
        trait __BabarTypedSqlSelection {
            fn __babar_select() -> Self;
        }

        impl #impl_generics __BabarTypedSqlSelection for ::babar::query::Command<#selected_type> #where_clause {
            fn __babar_select() -> Self {
                #selected_expr
            }
        }

        fn __babar_select<T>() -> T
        where
            T: __BabarTypedSqlSelection,
        {
            T::__babar_select()
        }

        __babar_select()
    }})
}

fn selection_parameter_codec_tokens(
    selection: &TypedSqlSelection,
    lowered: &LoweredQuery,
) -> Result<TokenStream2> {
    match selection {
        TypedSqlSelection::Default | TypedSqlSelection::Infer => {
            Ok(lowered.parameter_codec_tokens())
        }
        TypedSqlSelection::Explicit(ty) => explicit_parameter_codec_tokens(ty.as_ref(), lowered),
    }
}

fn selection_row_codec_tokens(
    selection: &TypedSqlSelection,
    lowered: &LoweredQuery,
) -> Result<TokenStream2> {
    match selection {
        TypedSqlSelection::Default | TypedSqlSelection::Infer => Ok(lowered.row_codec_tokens()),
        TypedSqlSelection::Explicit(ty) => explicit_row_codec_tokens(ty.as_ref(), lowered),
    }
}

fn explicit_parameter_codec_tokens(ty: &Type, lowered: &LoweredQuery) -> Result<TokenStream2> {
    let pattern_fields = lowered
        .parameters
        .iter()
        .map(|parameter| {
            let field_ident = field_ident(&parameter.logical_name);
            quote::quote! { #field_ident: _ }
        })
        .collect::<Vec<_>>();
    let encode_fields = lowered
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let field_ident = field_ident(&parameter.logical_name);
            let field_value = quote::format_ident!("__babar_field_{index}");
            let codec =
                lower::runtime_codec_tokens(parameter.sql_type, parameter.nullability, None)
                    .expect("lowered parameter contract should already support runtime codecs");
            Ok(quote::quote! {
                let #field_value = &value.#field_ident;
                ::babar::codec::Encoder::encode(&#codec, #field_value, params)?;
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let tuple_codecs = lowered
        .parameters
        .iter()
        .map(|parameter| {
            Ok(
                lower::runtime_codec_tokens(parameter.sql_type, parameter.nullability, None)
                    .expect("lowered parameter contract should already support runtime codecs"),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let tuple_types = lowered
        .parameters
        .iter()
        .map(|parameter| {
            Ok(
                lower::runtime_value_type_tokens(parameter.sql_type, parameter.nullability, None)
                    .expect(
                        "lowered parameter contract should already support runtime value types",
                    ),
            )
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(quote::quote! {{
        #[derive(Clone, Copy, Debug)]
        struct __BabarSelectedParamsCodec;

        impl ::babar::codec::Encoder<#ty> for __BabarSelectedParamsCodec {
            fn encode(
                &self,
                value: &#ty,
                params: &mut ::std::vec::Vec<::core::option::Option<::std::vec::Vec<u8>>>,
            ) -> ::babar::Result<()> {
                fn __babar_require_exact_fields(#ty { #(#pattern_fields,)* }: #ty) {}
                let _ = __babar_require_exact_fields as fn(#ty);
                #(#encode_fields)*
                ::core::result::Result::Ok(())
            }

            fn oids(&self) -> &'static [::babar::types::Oid] {
                let codec = (#(#tuple_codecs,)*);
                <_ as ::babar::codec::Encoder<(#(#tuple_types,)*)>>::oids(&codec)
            }

            fn format_codes(&self) -> &'static [i16] {
                let codec = (#(#tuple_codecs,)*);
                <_ as ::babar::codec::Encoder<(#(#tuple_types,)*)>>::format_codes(&codec)
            }
        }

        __BabarSelectedParamsCodec
    }})
}

fn explicit_row_codec_tokens(ty: &Type, lowered: &LoweredQuery) -> Result<TokenStream2> {
    let decode_fields = lowered
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let field_value = quote::format_ident!("__babar_field_{index}");
            let codec = lower::runtime_codec_tokens(column.sql_type, column.nullability, None)
                .expect("lowered row contract should already support runtime codecs");
            Ok(quote::quote! {
                let #field_value = ::babar::codec::Decoder::decode(&#codec, &columns[#index..#index + 1])?;
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let struct_fields = lowered
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let field_ident = field_ident(&column.label);
            let field_value = quote::format_ident!("__babar_field_{index}");
            quote::quote! { #field_ident: #field_value }
        })
        .collect::<Vec<_>>();
    let tuple_codecs = lowered
        .columns
        .iter()
        .map(|column| {
            Ok(
                lower::runtime_codec_tokens(column.sql_type, column.nullability, None)
                    .expect("lowered row contract should already support runtime codecs"),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let tuple_types = lowered
        .columns
        .iter()
        .map(|column| {
            Ok(
                lower::runtime_value_type_tokens(column.sql_type, column.nullability, None)
                    .expect("lowered row contract should already support runtime value types"),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let n_columns = lowered.columns.len();

    Ok(quote::quote! {{
        #[derive(Clone, Copy, Debug)]
        struct __BabarSelectedRowCodec;

        impl ::babar::codec::Decoder<#ty> for __BabarSelectedRowCodec {
            fn decode(
                &self,
                columns: &[::core::option::Option<::babar::__private::Bytes>],
            ) -> ::babar::Result<#ty> {
                #(#decode_fields)*
                ::core::result::Result::Ok(#ty { #(#struct_fields,)* })
            }

            fn n_columns(&self) -> usize {
                #n_columns
            }

            fn oids(&self) -> &'static [::babar::types::Oid] {
                let codec = (#(#tuple_codecs,)*);
                <_ as ::babar::codec::Decoder<(#(#tuple_types,)*)>>::oids(&codec)
            }

            fn format_codes(&self) -> &'static [i16] {
                let codec = (#(#tuple_codecs,)*);
                <_ as ::babar::codec::Decoder<(#(#tuple_types,)*)>>::format_codes(&codec)
            }
        }

        __BabarSelectedRowCodec
    }})
}

fn enforce_strict_row_selection(
    front_door: &TypedSqlFrontDoor,
    lowered: &LoweredQuery,
) -> Result<()> {
    if matches!(front_door.selections.row, TypedSqlSelection::Default) {
        return Ok(());
    }

    let mut seen = BTreeSet::new();
    let duplicates = lowered
        .columns
        .iter()
        .filter_map(|column| (!seen.insert(column.label.clone())).then_some(column.label.clone()))
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        return Ok(());
    }

    Err(front_door.sql.syn_error_message(format!(
        "strict row selection requires unique final output names before matching a row struct; duplicate output names: {}",
        duplicates.join(", ")
    )))
}

fn field_ident(name: &str) -> Ident {
    syn::parse_str(name).unwrap_or_else(|_| Ident::new_raw(name, proc_macro2::Span::call_site()))
}

fn enforce_entrypoint_contract(
    front_door: &TypedSqlFrontDoor,
    checked: &CheckedStatement,
    entrypoint: MacroEntrypoint,
) -> Result<()> {
    let message = match entrypoint {
        MacroEntrypoint::Query if checked.kind != StatementKind::Query => Some(
            "query! now only accepts schema-aware SELECT statements; use command! for typed INSERT, UPDATE, or DELETE statements",
        ),
        MacroEntrypoint::Command if checked.kind == StatementKind::Query => Some(
            "command! now accepts only schema-aware INSERT, UPDATE, or DELETE statements; use query! for typed SELECT statements",
        ),
        _ => None,
    };

    match message {
        Some(message) => Err(front_door.sql.syn_error_message(message)),
        None => Ok(()),
    }
}

fn rewrite_entrypoint_error(err: Error, _entrypoint: MacroEntrypoint) -> Error {
    Error::new(err.span(), err.to_string())
}

fn verify_live_schema(
    front_door: &TypedSqlFrontDoor,
    parsed: &ParsedSql,
    checked: &CheckedSelect,
    lowered: &LoweredQuery,
) -> std::result::Result<(), crate::verify::VerificationError> {
    let binding_tables = checked
        .bindings
        .iter()
        .map(|binding| (binding.binding_name.as_str(), binding.table_name.as_str()))
        .collect::<HashMap<_, _>>();
    let referenced_tables =
        collect_referenced_tables(parsed, &binding_tables, front_door.catalog());
    let params = lowered
        .parameters
        .iter()
        .map(|parameter| declared_type_for_sql_type(parameter.sql_type))
        .collect::<Vec<_>>();
    let rows = lowered
        .columns
        .iter()
        .map(|column| declared_type_for_sql_type(column.sql_type))
        .collect::<Vec<_>>();
    verify_typed_statement_against_probe(&lowered.sql, &referenced_tables, &params, Some(&rows))
}

fn collect_referenced_tables(
    parsed: &ParsedSql,
    binding_tables: &HashMap<&str, &str>,
    catalog: &SchemaCatalog,
) -> Vec<ReferencedTable> {
    let mut columns_by_table = BTreeMap::<String, BTreeSet<String>>::new();
    for table_name in binding_tables.values() {
        columns_by_table
            .entry((**table_name).to_owned())
            .or_default();
    }
    if let Some(select) = parsed.select.as_ref() {
        collect_referenced_columns_select(select, binding_tables, &mut columns_by_table);
    }
    columns_by_table
        .into_iter()
        .map(|(table_name, column_names)| ReferencedTable {
            columns: column_names
                .into_iter()
                .map(|column_name| {
                    let column = catalog
                        .lookup_column_by_display_name(&table_name, &column_name)
                        .expect("resolved referenced column should exist in schema catalog");
                    ReferencedColumn {
                        name: column_name,
                        type_: declared_type_for_sql_type(column.sql_type()),
                        nullability: column.nullability(),
                    }
                })
                .collect(),
            name: table_name,
        })
        .collect()
}

fn collect_referenced_columns_select(
    select: &super::ParsedSelect,
    binding_tables: &HashMap<&str, &str>,
    columns_by_table: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for projection in &select.projections {
        collect_referenced_columns_expr(&projection.expr, binding_tables, columns_by_table);
    }
    if let Some(filter) = &select.filter {
        collect_referenced_columns_expr(filter, binding_tables, columns_by_table);
    }
    for join in &select.joins {
        collect_referenced_columns_expr(&join.on, binding_tables, columns_by_table);
    }
    for order_by in &select.order_by {
        collect_referenced_columns_expr(&order_by.expr, binding_tables, columns_by_table);
    }
    if let Some(limit) = &select.limit {
        collect_referenced_columns_expr(&limit.expr, binding_tables, columns_by_table);
    }
    if let Some(offset) = &select.offset {
        collect_referenced_columns_expr(&offset.expr, binding_tables, columns_by_table);
    }
}

fn collect_referenced_columns_expr(
    expr: &ParsedExpr,
    binding_tables: &HashMap<&str, &str>,
    columns_by_table: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match expr {
        ParsedExpr::Column(column) => {
            if let Some(table_name) = binding_tables.get(column.binding.value.as_str()) {
                columns_by_table
                    .entry((**table_name).to_owned())
                    .or_default()
                    .insert(column.column.value.clone());
            }
        }
        ParsedExpr::OptionalGroup(group) => {
            collect_referenced_columns_expr(&group.expr, binding_tables, columns_by_table);
        }
        ParsedExpr::Unary { expr, .. } | ParsedExpr::IsNull { expr, .. } => {
            collect_referenced_columns_expr(expr, binding_tables, columns_by_table);
        }
        ParsedExpr::Binary { left, right, .. } => {
            collect_referenced_columns_expr(left, binding_tables, columns_by_table);
            collect_referenced_columns_expr(right, binding_tables, columns_by_table);
        }
        ParsedExpr::BoolChain { terms, .. } => {
            for term in terms {
                collect_referenced_columns_expr(term, binding_tables, columns_by_table);
            }
        }
        ParsedExpr::Placeholder(_) | ParsedExpr::Literal(_) => {}
    }
}

struct TypedSqlFrontDoorInput {
    source_kind: SchemaSourceKind,
    schema: SchemaInput,
    selections: TypedSqlSelections,
    sql: PublicSqlInput,
}

struct TypedSqlFrontDoor {
    source_kind: SchemaSourceKind,
    catalog: SchemaCatalog,
    selections: TypedSqlSelections,
    sql: PublicSqlInput,
}

struct TypedQueryInput(TypedSqlFrontDoorInput);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaSourceKind {
    Inline,
    AuthoredBridge,
}

struct TypedSqlSelections {
    params: TypedSqlSelection,
    row: TypedSqlSelection,
}

impl Default for TypedSqlSelections {
    fn default() -> Self {
        Self {
            params: TypedSqlSelection::Default,
            row: TypedSqlSelection::Default,
        }
    }
}

enum TypedSqlSelection {
    Default,
    Explicit(Box<Type>),
    Infer,
}

impl Parse for TypedSqlFrontDoorInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let source_kind = if input.peek(kw::schema) {
            input.parse::<kw::schema>()?;
            SchemaSourceKind::Inline
        } else if input.peek(kw::__babar_schema) {
            input.parse::<kw::__babar_schema>()?;
            SchemaSourceKind::AuthoredBridge
        } else {
            return Err(input.error("expected `schema = { ... }` before schema-aware SQL"));
        };
        input.parse::<Token![=]>()?;
        let schema = input.parse()?;
        input.parse::<Token![,]>()?;
        let selections = parse_typed_sql_selections(input)?;
        let sql_tokens = trim_optional_trailing_comma(input.parse::<TokenStream2>()?);
        reject_extra_schema_argument(&sql_tokens, source_kind)?;
        let sql = PublicSqlInput::parse(sql_tokens)?;
        Ok(Self {
            source_kind,
            schema,
            selections,
            sql,
        })
    }
}

impl TypedSqlFrontDoorInput {
    fn into_front_door(self) -> Result<TypedSqlFrontDoor> {
        Ok(TypedSqlFrontDoor {
            source_kind: self.source_kind,
            catalog: self.schema.into_catalog()?,
            selections: self.selections,
            sql: self.sql,
        })
    }
}

impl TypedSqlFrontDoor {
    fn catalog(&self) -> &SchemaCatalog {
        &self.catalog
    }

    fn parse_with<T>(&self, parse: impl FnOnce(super::SqlSource) -> super::Result<T>) -> Result<T> {
        self.sql.parse_with(parse)
    }

    fn syn_error(&self, err: TypedSqlError) -> Error {
        self.sql.syn_error(self.decorate_error(err))
    }

    fn decorate_error(&self, mut err: TypedSqlError) -> TypedSqlError {
        if self.source_kind == SchemaSourceKind::AuthoredBridge {
            match err.kind {
                TypedSqlErrorKind::Resolve => {
                    err.message =
                        format!("authored external schema lookup failed: {}", err.message);
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
}

impl TypedQueryInput {
    fn into_front_door(self) -> Result<TypedSqlFrontDoor> {
        self.0.into_front_door()
    }
}

impl Parse for TypedQueryInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        Ok(Self(input.parse()?))
    }
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

fn parse_typed_sql_selections(input: ParseStream<'_>) -> Result<TypedSqlSelections> {
    let mut selections = TypedSqlSelections::default();

    loop {
        if input.peek(kw::params) {
            input.parse::<kw::params>()?;
            input.parse::<Token![=]>()?;
            let ty: Type = input.parse()?;
            if !matches!(selections.params, TypedSqlSelection::Default) {
                return Err(input.error("duplicate `params = Type` selection"));
            }
            selections.params = match ty {
                Type::Infer(_) => TypedSqlSelection::Infer,
                ty => TypedSqlSelection::Explicit(Box::new(ty)),
            };
        } else if input.peek(kw::row) {
            input.parse::<kw::row>()?;
            input.parse::<Token![=]>()?;
            let ty: Type = input.parse()?;
            if !matches!(selections.row, TypedSqlSelection::Default) {
                return Err(input.error("duplicate `row = Type` selection"));
            }
            selections.row = match ty {
                Type::Infer(_) => TypedSqlSelection::Infer,
                ty => TypedSqlSelection::Explicit(Box::new(ty)),
            };
        } else {
            break;
        }

        if input.is_empty() {
            return Err(input.error("expected SQL after typed SQL shape selections"));
        }
        input.parse::<Token![,]>()?;
    }

    Ok(selections)
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
            "schema-scoped wrapper already supplies the schema; inline `schema = { ... }` blocks cannot be mixed into this call"
        }
        (SchemaSourceKind::AuthoredBridge, "__babar_schema") => {
            "schema-scoped wrapper already supplies its internal schema bridge; do not pass `__babar_schema = { ... }` manually"
        }
        (SchemaSourceKind::Inline, "__babar_schema") => {
            "cannot mix inline `schema = { ... }` with the authored external schema bridge `__babar_schema = { ... }` in one schema-aware typed SQL call"
        }
        (SchemaSourceKind::Inline, "schema") => {
            "schema-aware typed SQL accepts only one `schema = { ... }` block before the SQL input"
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
                    format!("duplicate table `{qualified_name}` in schema block"),
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
                            "duplicate column `{column_name}` in schema table `{display_name}`"
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
                "unsupported schema type `{other}`; supported types are bool, bytea, varchar, text, int2, int4, int8, float4, float8, uuid, date, time, timestamp, timestamptz, json, jsonb, and numeric"
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

    use super::{compile_typed_query, MacroEntrypoint, TypedQueryInput};

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

        assert!(err
            .to_string()
            .contains("schema-scoped wrapper already supplies the schema"));
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

    #[test]
    fn public_command_keeps_non_returning_dml_command_shaped() {
        let tokens = compile_typed_query(
            parse2::<TypedQueryInput>(quote! {
                schema = {
                    table public.users {
                        id: int4,
                        name: text,
                    },
                },
                INSERT INTO users (id, name) VALUES ($id, $name)
            })
            .expect("typed command input parses"),
            MacroEntrypoint::Command,
        )
        .expect("non-returning command! should compile");

        assert!(tokens
            .to_string()
            .contains(":: babar :: query :: Command :: from_fragment"));
    }

    #[test]
    fn public_command_lowers_returning_dml_through_query_path() {
        let tokens = compile_typed_query(
            parse2::<TypedQueryInput>(quote! {
                schema = {
                    table public.users {
                        id: int4,
                        name: text,
                    },
                },
                UPDATE users SET name = $name WHERE users.id = $id RETURNING users.id, users.name
            })
            .expect("typed command input parses"),
            MacroEntrypoint::Command,
        )
        .expect("returning command! should compile");

        assert!(tokens
            .to_string()
            .contains(":: babar :: query :: Query :: from_fragment"));
    }

    #[test]
    fn public_query_rejects_write_statements() {
        let err = compile_typed_query(
            parse2::<TypedQueryInput>(quote! {
                schema = {
                    table public.users {
                        id: int4,
                        name: text,
                    },
                },
                DELETE FROM users WHERE users.id = $id
            })
            .expect("typed query input parses"),
            MacroEntrypoint::Query,
        )
        .expect_err("query! should reject write statements");

        assert!(err
            .to_string()
            .contains("query! now only accepts schema-aware SELECT statements"));
    }

    #[test]
    fn public_command_rejects_select_statements() {
        let err = compile_typed_query(
            parse2::<TypedQueryInput>(quote! {
                schema = {
                    table public.users {
                        id: int4,
                        name: text,
                    },
                },
                SELECT users.id, users.name FROM users
            })
            .expect("typed command input parses"),
            MacroEntrypoint::Command,
        )
        .expect_err("command! should reject select statements");

        assert!(err.to_string().contains(
            "command! now accepts only schema-aware INSERT, UPDATE, or DELETE statements"
        ));
    }
}
