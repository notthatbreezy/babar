use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

use super::ir::{
    AstId, BinaryOp, BoolOp, ColumnRefSyntax, JoinKind, Literal, NullsOrder, ObjectNameSyntax,
    OrderDirection, OutputNameSyntax, ParsedAssignment, ParsedAssignmentTarget, ParsedDelete,
    ParsedExpr, ParsedFrom, ParsedInsert, ParsedOrderBy, ParsedProjection, ParsedSelect,
    ParsedStatementBody, ParsedUpdate, ParsedValuesRow, PlaceholderId, PlaceholderRef, UnaryOp,
};
use super::resolver::{
    CheckedDelete, CheckedExpr, CheckedExprNode, CheckedInsert, CheckedParameter,
    CheckedProjection, CheckedSelect, CheckedStatement, CheckedStatementBody, CheckedUpdate,
    Nullability, SqlType,
};
use super::{ParsedSql, Result, SourceSpan, TypedSqlError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredQuery {
    pub(crate) sql: String,
    pub(crate) parameters: Vec<LoweredParameter>,
    pub(crate) columns: Vec<LoweredColumn>,
    renderer: SqlRenderer,
    toggle_group_ids: Vec<AstId>,
}

impl LoweredQuery {
    pub(crate) fn parameter_codec_tokens(&self) -> TokenStream {
        let tokens = self
            .parameters
            .iter()
            .map(LoweredParameter::template_codec_tokens)
            .chain(
                self.toggle_group_ids
                    .iter()
                    .map(|_| quote! { ::babar::__private::toggle }),
            )
            .collect::<Vec<_>>();
        quote! { (#(#tokens,)*) }
    }

    pub(crate) fn parameter_type_tokens(&self) -> TokenStream {
        let tokens = self
            .parameters
            .iter()
            .map(LoweredParameter::template_value_type_tokens)
            .chain(self.toggle_group_ids.iter().map(|_| quote! { bool }))
            .collect::<Vec<_>>();
        quote! { (#(#tokens,)*) }
    }

    pub(crate) fn row_codec_tokens(&self) -> TokenStream {
        tuple_codec_tokens(self.columns.iter().map(|column| column.codec).collect())
    }

    pub(crate) fn row_type_tokens(&self) -> TokenStream {
        let tokens = self
            .columns
            .iter()
            .map(LoweredColumn::value_type_tokens)
            .collect::<Vec<_>>();
        quote! { (#(#tokens,)*) }
    }

    pub(crate) fn is_dynamic(&self) -> bool {
        self.parameters.iter().any(|parameter| parameter.optional)
            || !self.toggle_group_ids.is_empty()
    }

    pub(crate) fn emit_query_tokens(&self) -> TokenStream {
        self.emit_query_tokens_with(self.parameter_codec_tokens(), self.row_codec_tokens())
    }

    pub(crate) fn emit_command_tokens(&self) -> TokenStream {
        self.emit_command_tokens_with(self.parameter_codec_tokens())
    }

    pub(crate) fn emit_query_tokens_with(
        &self,
        params: TokenStream,
        row: TokenStream,
    ) -> TokenStream {
        if self.is_dynamic() {
            return self.emit_dynamic_query_tokens_with(params, row);
        }

        let sql = LitStr::new(&self.sql, Span::call_site());
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

    pub(crate) fn emit_command_tokens_with(&self, params: TokenStream) -> TokenStream {
        if self.is_dynamic() {
            return self.emit_dynamic_command_tokens_with(params);
        }

        let sql = LitStr::new(&self.sql, Span::call_site());
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
            ::babar::query::Command::from_fragment(__babar_fragment)
        }}
    }

    fn emit_dynamic_query_tokens_with(&self, params: TokenStream, row: TokenStream) -> TokenStream {
        let sql = LitStr::new(&self.sql, Span::call_site());
        let n_params = self.parameters.len();
        let renderer = self.runtime_sql_renderer_tokens();

        quote! {{
            let __babar_fragment = ::babar::query::Fragment::__from_dynamic_parts(
                #sql,
                #params,
                #n_params,
                ::core::option::Option::Some(::babar::query::Origin::new(
                    file!(),
                    line!(),
                    column!(),
                )),
                #renderer,
            );
            ::babar::query::Query::from_fragment(__babar_fragment, #row)
        }}
    }

    fn emit_dynamic_command_tokens_with(&self, params: TokenStream) -> TokenStream {
        let sql = LitStr::new(&self.sql, Span::call_site());
        let n_params = self.parameters.len();
        let renderer = self.runtime_sql_renderer_tokens();

        quote! {{
            let __babar_fragment = ::babar::query::Fragment::__from_dynamic_parts(
                #sql,
                #params,
                #n_params,
                ::core::option::Option::Some(::babar::query::Origin::new(
                    file!(),
                    line!(),
                    column!(),
                )),
                #renderer,
            );
            ::babar::query::Command::from_fragment(__babar_fragment)
        }}
    }

    #[cfg(test)]
    fn render_sql_for(
        &self,
        active_placeholders: impl IntoIterator<Item = PlaceholderId>,
        active_groups: impl IntoIterator<Item = AstId>,
    ) -> Result<String> {
        let active_placeholders = active_placeholders.into_iter().collect::<HashSet<_>>();
        let sql = self.renderer.render(
            active_placeholders.clone(),
            active_groups.into_iter().collect(),
        )?;
        let active_slots = self
            .parameters
            .iter()
            .filter(|parameter| !parameter.optional || active_placeholders.contains(&parameter.id))
            .map(|parameter| parameter.position)
            .collect::<Vec<_>>();
        Ok(renumber_sql_placeholders(&sql, &active_slots))
    }

    #[cfg(test)]
    fn activate_parameter_names(&self, names: &[&str]) -> Vec<PlaceholderId> {
        self.parameters
            .iter()
            .filter(|parameter| names.iter().any(|name| *name == parameter.logical_name))
            .map(|parameter| parameter.id)
            .collect()
    }

    #[cfg(test)]
    fn optional_group_ids(&self) -> Vec<AstId> {
        self.renderer.optional_group_ids()
    }

    fn runtime_sql_renderer_tokens(&self) -> TokenStream {
        let optional_parameters = self
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| parameter.optional.then_some((index, parameter.id)))
            .collect::<Vec<_>>();
        let flag_exprs = optional_parameters
            .iter()
            .map(|(index, _)| {
                let index = syn::Index::from(*index);
                quote! { __babar_args.#index.is_some() }
            })
            .chain(self.toggle_group_ids.iter().enumerate().map(|(offset, _)| {
                let index = syn::Index::from(self.parameters.len() + offset);
                quote! { __babar_args.#index }
            }))
            .collect::<Vec<_>>();
        let arg_types = self
            .parameters
            .iter()
            .map(LoweredParameter::template_value_type_tokens)
            .chain(self.toggle_group_ids.iter().map(|_| quote! { bool }))
            .collect::<Vec<_>>();
        let shapes = runtime_sql_shapes(self, &optional_parameters);

        let arms = shapes
            .iter()
            .map(|shape| {
                let pattern = shape
                    .flags
                    .iter()
                    .map(|flag| if *flag { quote! { true } } else { quote! { false } })
                    .collect::<Vec<_>>();
                let sql = LitStr::new(&shape.sql, Span::call_site());
                let capacity = shape.active_parameter_indexes.len();
                let pushes = shape
                    .active_parameter_indexes
                    .iter()
                    .map(|index| {
                        let parameter = &self.parameters[*index];
                        let index = syn::Index::from(*index);
                        let codec = parameter.codec.tokens();
                        if parameter.optional {
                            quote! {
                                let __babar_value = __babar_args.#index.as_ref().expect("shape matched active optional input");
                                ::babar::__private::push_bound_param(
                                    &#codec,
                                    __babar_value,
                                    &mut __babar_params,
                                    &mut __babar_param_types,
                                    &mut __babar_param_formats,
                                )?;
                            }
                        } else {
                            quote! {
                                ::babar::__private::push_bound_param(
                                    &#codec,
                                    &__babar_args.#index,
                                    &mut __babar_params,
                                    &mut __babar_param_types,
                                    &mut __babar_param_formats,
                                )?;
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                quote! {
                    (#(#pattern,)*) => {
                        let mut __babar_params = ::std::vec::Vec::with_capacity(#capacity);
                        let mut __babar_param_types = ::std::vec::Vec::with_capacity(#capacity);
                        let mut __babar_param_formats = ::std::vec::Vec::with_capacity(#capacity);
                        #(#pushes)*
                        ::core::result::Result::Ok(::babar::__private::BoundStatement::new(
                            ::std::string::String::from(#sql),
                            __babar_params,
                            __babar_param_types,
                            __babar_param_formats,
                        ))
                    },
                }
            })
            .collect::<Vec<_>>();

        quote! {
            move |__babar_args: &(#(#arg_types,)*)| {
                match (#(#flag_exprs,)*) {
                    #(#arms)*
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RuntimeSqlShape {
    flags: Vec<bool>,
    sql: String,
    active_parameter_indexes: Vec<usize>,
}

fn runtime_sql_shapes(
    query: &LoweredQuery,
    optional_parameters: &[(usize, PlaceholderId)],
) -> Vec<RuntimeSqlShape> {
    let total_flags = optional_parameters.len() + query.toggle_group_ids.len();
    let mut shapes = Vec::with_capacity(1_usize << total_flags);
    for mask in 0..(1_usize << total_flags) {
        let mut flags = Vec::with_capacity(total_flags);
        let mut active_placeholders = HashSet::new();
        let mut active_groups = HashSet::new();

        for (bit_index, (_, placeholder_id)) in optional_parameters.iter().enumerate() {
            let active = (mask & (1 << bit_index)) != 0;
            flags.push(active);
            if active {
                active_placeholders.insert(*placeholder_id);
            }
        }

        for (offset, group_id) in query.toggle_group_ids.iter().enumerate() {
            let bit_index = optional_parameters.len() + offset;
            let active = (mask & (1 << bit_index)) != 0;
            flags.push(active);
            if active {
                active_groups.insert(*group_id);
            }
        }

        let sql = query
            .renderer
            .render(active_placeholders.clone(), active_groups)
            .expect("runtime SQL variants should render");
        let active_parameter_indexes = query
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| {
                (!parameter.optional || active_placeholders.contains(&parameter.id))
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let active_slots = active_parameter_indexes
            .iter()
            .map(|index| query.parameters[*index].position)
            .collect::<Vec<_>>();
        let sql = renumber_sql_placeholders(&sql, &active_slots);
        shapes.push(RuntimeSqlShape {
            flags,
            sql,
            active_parameter_indexes,
        });
    }
    shapes
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredParameter {
    pub(crate) id: PlaceholderId,
    pub(crate) logical_name: String,
    pub(crate) position: u32,
    pub(crate) sql_type: SqlType,
    pub(crate) nullability: Nullability,
    pub(crate) optional: bool,
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
    Uuid,
    Date,
    Time,
    Timestamp,
    Timestamptz,
    Json,
    Jsonb,
    Numeric,
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
    Uuid,
    Date,
    Time,
    Timestamp,
    Timestamptz,
    Json,
    Jsonb,
    Numeric,
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
            Self::Uuid => return quote! { ::babar::codec::uuid },
            Self::Date => return quote! { ::babar::codec::date },
            Self::Time => return quote! { ::babar::codec::time },
            Self::Timestamp => return quote! { ::babar::codec::timestamp },
            Self::Timestamptz => return quote! { ::babar::codec::timestamptz },
            Self::Json => return quote! { ::babar::codec::json },
            Self::Jsonb => return quote! { ::babar::codec::jsonb },
            Self::Numeric => return quote! { ::babar::codec::numeric },
            Self::Nullable(base) => base,
        };

        let inner = base.tokens();
        quote! { ::babar::codec::nullable(#inner) }
    }

    fn value_type_tokens(self) -> TokenStream {
        let base = match self {
            Self::Bool => return quote! { bool },
            Self::Bytea => return quote! { ::std::vec::Vec<u8> },
            Self::Varchar | Self::Text => return quote! { ::std::string::String },
            Self::Int2 => return quote! { i16 },
            Self::Int4 => return quote! { i32 },
            Self::Int8 => return quote! { i64 },
            Self::Float4 => return quote! { f32 },
            Self::Float8 => return quote! { f64 },
            Self::Uuid => return quote! { ::uuid::Uuid },
            Self::Date => return quote! { ::time::Date },
            Self::Time => return quote! { ::time::Time },
            Self::Timestamp => return quote! { ::time::PrimitiveDateTime },
            Self::Timestamptz => return quote! { ::time::OffsetDateTime },
            Self::Json | Self::Jsonb => return quote! { ::serde_json::Value },
            Self::Numeric => return quote! { ::rust_decimal::Decimal },
            Self::Nullable(base) => base,
        };

        let inner = base.value_type_tokens();
        quote! { ::core::option::Option<#inner> }
    }
}

impl LoweredParameter {
    fn template_codec_tokens(&self) -> TokenStream {
        let codec = self.codec.tokens();
        if self.optional {
            quote! { ::babar::codec::nullable(#codec) }
        } else {
            codec
        }
    }

    fn template_value_type_tokens(&self) -> TokenStream {
        let value = self.codec.value_type_tokens();
        if self.optional {
            quote! { ::core::option::Option<#value> }
        } else {
            value
        }
    }
}

impl LoweredColumn {
    fn value_type_tokens(&self) -> TokenStream {
        self.codec.value_type_tokens()
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
            Self::Uuid => quote! { ::babar::codec::uuid },
            Self::Date => quote! { ::babar::codec::date },
            Self::Time => quote! { ::babar::codec::time },
            Self::Timestamp => quote! { ::babar::codec::timestamp },
            Self::Timestamptz => quote! { ::babar::codec::timestamptz },
            Self::Json => quote! { ::babar::codec::json },
            Self::Jsonb => quote! { ::babar::codec::jsonb },
            Self::Numeric => quote! { ::babar::codec::numeric },
        }
    }

    fn value_type_tokens(self) -> TokenStream {
        match self {
            Self::Bool => quote! { bool },
            Self::Bytea => quote! { ::std::vec::Vec<u8> },
            Self::Varchar | Self::Text => quote! { ::std::string::String },
            Self::Int2 => quote! { i16 },
            Self::Int4 => quote! { i32 },
            Self::Int8 => quote! { i64 },
            Self::Float4 => quote! { f32 },
            Self::Float8 => quote! { f64 },
            Self::Uuid => quote! { ::uuid::Uuid },
            Self::Date => quote! { ::time::Date },
            Self::Time => quote! { ::time::Time },
            Self::Timestamp => quote! { ::time::PrimitiveDateTime },
            Self::Timestamptz => quote! { ::time::OffsetDateTime },
            Self::Json | Self::Jsonb => quote! { ::serde_json::Value },
            Self::Numeric => quote! { ::rust_decimal::Decimal },
        }
    }
}

pub(crate) fn lower_select(parsed: &ParsedSql, checked: &CheckedSelect) -> Result<LoweredQuery> {
    let select = parsed
        .select
        .as_ref()
        .expect("lower_select should only run on SELECT statements");
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
    let renderer = SqlRenderer::new(select);
    let toggle_group_ids = collect_toggle_group_ids_select(checked);
    let sql = renderer.render(
        parameters.iter().map(|parameter| parameter.id).collect(),
        renderer.optional_group_ids().into_iter().collect(),
    )?;

    Ok(LoweredQuery {
        sql,
        parameters,
        columns,
        renderer,
        toggle_group_ids,
    })
}

pub(crate) fn lower_statement(
    parsed: &ParsedSql,
    checked: &CheckedStatement,
) -> Result<LoweredQuery> {
    match &checked.body {
        CheckedStatementBody::Select(select) => lower_select(parsed, select),
        CheckedStatementBody::Insert(insert) => lower_insert(parsed, insert),
        CheckedStatementBody::Update(update) => lower_update(parsed, update),
        CheckedStatementBody::Delete(delete) => lower_delete(parsed, delete),
    }
}

fn lower_insert(parsed: &ParsedSql, checked: &CheckedInsert) -> Result<LoweredQuery> {
    let insert = match &parsed.statement.body {
        ParsedStatementBody::Rows(rows) => match &rows.body {
            super::ir::ParsedRowStatementBody::Insert(insert) => insert,
            _ => unreachable!("checked insert should align with parsed insert"),
        },
        ParsedStatementBody::Command(command) => match &command.body {
            super::ir::ParsedCommandStatementBody::Insert(insert) => insert,
            _ => unreachable!("checked insert should align with parsed insert"),
        },
    };
    lower_dml(
        parsed,
        &checked.parameters,
        &checked.projections,
        SqlRenderer::insert(insert),
        Vec::new(),
    )
}

fn lower_update(parsed: &ParsedSql, checked: &CheckedUpdate) -> Result<LoweredQuery> {
    let update = match &parsed.statement.body {
        ParsedStatementBody::Rows(rows) => match &rows.body {
            super::ir::ParsedRowStatementBody::Update(update) => update,
            _ => unreachable!("checked update should align with parsed update"),
        },
        ParsedStatementBody::Command(command) => match &command.body {
            super::ir::ParsedCommandStatementBody::Update(update) => update,
            _ => unreachable!("checked update should align with parsed update"),
        },
    };
    lower_dml(
        parsed,
        &checked.parameters,
        &checked.projections,
        SqlRenderer::update(update),
        collect_toggle_group_ids_exprs([Some(&checked.filter)]),
    )
}

fn lower_delete(parsed: &ParsedSql, checked: &CheckedDelete) -> Result<LoweredQuery> {
    let delete = match &parsed.statement.body {
        ParsedStatementBody::Rows(rows) => match &rows.body {
            super::ir::ParsedRowStatementBody::Delete(delete) => delete,
            _ => unreachable!("checked delete should align with parsed delete"),
        },
        ParsedStatementBody::Command(command) => match &command.body {
            super::ir::ParsedCommandStatementBody::Delete(delete) => delete,
            _ => unreachable!("checked delete should align with parsed delete"),
        },
    };
    lower_dml(
        parsed,
        &checked.parameters,
        &checked.projections,
        SqlRenderer::delete(delete),
        collect_toggle_group_ids_exprs([Some(&checked.filter)]),
    )
}

fn lower_dml(
    parsed: &ParsedSql,
    checked_parameters: &[CheckedParameter],
    checked_projections: &[CheckedProjection],
    renderer: SqlRenderer,
    toggle_group_ids: Vec<AstId>,
) -> Result<LoweredQuery> {
    let parameters = checked_parameters
        .iter()
        .map(|parameter| lower_parameter(parsed, parameter))
        .collect::<Result<Vec<_>>>()?;
    let columns = checked_projections
        .iter()
        .map(lower_projection)
        .collect::<Result<Vec<_>>>()?;
    let sql = renderer.render(
        parameters.iter().map(|parameter| parameter.id).collect(),
        renderer.optional_group_ids().into_iter().collect(),
    )?;
    Ok(LoweredQuery {
        sql,
        parameters,
        columns,
        renderer,
        toggle_group_ids,
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
        id: parameter.id,
        logical_name: parameter.name.clone(),
        position: parameter.slot,
        sql_type: parameter.sql_type,
        nullability: parameter.nullability,
        optional: parameter.optional,
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct SqlRenderer {
    statement: RenderedStatement,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum RenderedStatement {
    Select(ParsedSelect),
    Insert(ParsedInsert),
    Update(ParsedUpdate),
    Delete(ParsedDelete),
}

impl SqlRenderer {
    fn new(select: &ParsedSelect) -> Self {
        Self {
            statement: RenderedStatement::Select(select.clone()),
        }
    }

    fn insert(insert: &ParsedInsert) -> Self {
        Self {
            statement: RenderedStatement::Insert(insert.clone()),
        }
    }

    fn update(update: &ParsedUpdate) -> Self {
        Self {
            statement: RenderedStatement::Update(update.clone()),
        }
    }

    fn delete(delete: &ParsedDelete) -> Self {
        Self {
            statement: RenderedStatement::Delete(delete.clone()),
        }
    }

    fn render(
        &self,
        active_placeholders: HashSet<PlaceholderId>,
        active_groups: HashSet<AstId>,
    ) -> Result<String> {
        match &self.statement {
            RenderedStatement::Select(select) => {
                let mut sql = String::new();
                sql.push_str("SELECT ");
                sql.push_str(
                    &select
                        .projections
                        .iter()
                        .map(render_projection)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                sql.push_str(" FROM ");
                sql.push_str(&render_from(&select.from));
                for join in &select.joins {
                    sql.push(' ');
                    sql.push_str(join_kind_sql(join.kind));
                    sql.push(' ');
                    sql.push_str("JOIN ");
                    sql.push_str(&render_from(&join.right));
                    sql.push_str(" ON ");
                    match render_predicate_expr(&join.on, &active_placeholders, &active_groups)? {
                        Some(on) => sql.push_str(&on),
                        None => sql.push_str("TRUE"),
                    }
                }
                if let Some(filter) = &select.filter {
                    if let Some(filter) =
                        render_predicate_expr(filter, &active_placeholders, &active_groups)?
                    {
                        sql.push_str(" WHERE ");
                        sql.push_str(&filter);
                    }
                }

                let mut order_by = Vec::new();
                for item in &select.order_by {
                    if let Some(rendered) =
                        render_order_by_item(item, &active_placeholders, &active_groups)?
                    {
                        order_by.push(rendered);
                    }
                }
                if !order_by.is_empty() {
                    sql.push_str(" ORDER BY ");
                    sql.push_str(&order_by.join(", "));
                }

                if let Some(limit) = &select.limit {
                    if let Some(limit_sql) =
                        render_value_expr(&limit.expr, &active_placeholders, &active_groups)?
                    {
                        sql.push_str(" LIMIT ");
                        sql.push_str(&limit_sql);
                    }
                }
                if let Some(offset) = &select.offset {
                    if let Some(offset_sql) =
                        render_value_expr(&offset.expr, &active_placeholders, &active_groups)?
                    {
                        sql.push_str(" OFFSET ");
                        sql.push_str(&offset_sql);
                    }
                }

                Ok(sql)
            }
            RenderedStatement::Insert(insert) => {
                let mut sql = String::new();
                sql.push_str("INSERT INTO ");
                sql.push_str(&render_from(&insert.target));
                if !insert.columns.is_empty() {
                    sql.push_str(" (");
                    sql.push_str(
                        &insert
                            .columns
                            .iter()
                            .map(|column| column.value.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    sql.push(')');
                }
                sql.push_str(" VALUES ");
                sql.push_str(
                    &insert
                        .values
                        .iter()
                        .map(render_values_row)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                if !insert.returning.is_empty() {
                    sql.push_str(" RETURNING ");
                    sql.push_str(
                        &insert
                            .returning
                            .iter()
                            .map(render_projection)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
                Ok(sql)
            }
            RenderedStatement::Update(update) => {
                let mut sql = String::new();
                sql.push_str("UPDATE ");
                sql.push_str(&render_from(&update.target));
                sql.push_str(" SET ");
                sql.push_str(
                    &update
                        .assignments
                        .iter()
                        .map(render_assignment)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                if let Some(filter) =
                    render_predicate_expr(&update.filter, &active_placeholders, &active_groups)?
                {
                    sql.push_str(" WHERE ");
                    sql.push_str(&filter);
                }
                if !update.returning.is_empty() {
                    sql.push_str(" RETURNING ");
                    sql.push_str(
                        &update
                            .returning
                            .iter()
                            .map(render_projection)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
                Ok(sql)
            }
            RenderedStatement::Delete(delete) => {
                let mut sql = String::new();
                sql.push_str("DELETE FROM ");
                sql.push_str(&render_from(&delete.target));
                if let Some(filter) =
                    render_predicate_expr(&delete.filter, &active_placeholders, &active_groups)?
                {
                    sql.push_str(" WHERE ");
                    sql.push_str(&filter);
                }
                if !delete.returning.is_empty() {
                    sql.push_str(" RETURNING ");
                    sql.push_str(
                        &delete
                            .returning
                            .iter()
                            .map(render_projection)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
                Ok(sql)
            }
        }
    }

    fn optional_group_ids(&self) -> Vec<AstId> {
        let mut ids = Vec::new();
        match &self.statement {
            RenderedStatement::Select(select) => {
                collect_optional_group_ids_select(select, &mut ids)
            }
            RenderedStatement::Insert(insert) => {
                for row in &insert.values {
                    for value in &row.values {
                        collect_optional_group_ids_expr(value, &mut ids);
                    }
                }
                for projection in &insert.returning {
                    collect_optional_group_ids_expr(&projection.expr, &mut ids);
                }
            }
            RenderedStatement::Update(update) => {
                for assignment in &update.assignments {
                    collect_optional_group_ids_expr(&assignment.value, &mut ids);
                }
                collect_optional_group_ids_expr(&update.filter, &mut ids);
                for projection in &update.returning {
                    collect_optional_group_ids_expr(&projection.expr, &mut ids);
                }
            }
            RenderedStatement::Delete(delete) => {
                collect_optional_group_ids_expr(&delete.filter, &mut ids);
                for projection in &delete.returning {
                    collect_optional_group_ids_expr(&projection.expr, &mut ids);
                }
            }
        }
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }
}

fn render_projection(projection: &ParsedProjection) -> String {
    let expr = render_expr_sql(&projection.expr);
    match &projection.output_name {
        OutputNameSyntax::Explicit(alias) => format!("{expr} AS {}", alias.value),
        OutputNameSyntax::Implicit(alias) => {
            if matches!(
                &projection.expr,
                ParsedExpr::Column(ColumnRefSyntax { column, .. }) if column.value == alias.value
            ) {
                expr
            } else {
                format!("{expr} AS {}", alias.value)
            }
        }
        OutputNameSyntax::Anonymous => expr,
    }
}

fn render_from(from: &ParsedFrom) -> String {
    format!(
        "{} AS {}",
        render_object_name(&from.table_name),
        from.binding_name.value
    )
}

fn render_values_row(row: &ParsedValuesRow) -> String {
    format!(
        "({})",
        row.values
            .iter()
            .map(render_expr_sql)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn render_assignment(assignment: &ParsedAssignment) -> String {
    format!(
        "{} = {}",
        render_assignment_target(&assignment.target),
        render_expr_sql(&assignment.value)
    )
}

fn render_assignment_target(target: &ParsedAssignmentTarget) -> String {
    match &target.binding {
        Some(binding) => format!("{}.{}", binding.value, target.column.value),
        None => target.column.value.clone(),
    }
}

fn render_object_name(name: &ObjectNameSyntax) -> String {
    name.parts
        .iter()
        .map(|part| part.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn join_kind_sql(kind: JoinKind) -> &'static str {
    match kind {
        JoinKind::Inner => "INNER",
        JoinKind::Left => "LEFT",
        JoinKind::Right => "RIGHT",
        JoinKind::Full => "FULL",
    }
}

fn render_order_by_item(
    item: &ParsedOrderBy,
    active_placeholders: &HashSet<PlaceholderId>,
    active_groups: &HashSet<AstId>,
) -> Result<Option<String>> {
    let Some(expr) = render_value_expr(&item.expr, active_placeholders, active_groups)? else {
        return Ok(None);
    };
    let mut sql = expr;
    if item.direction == OrderDirection::Desc {
        sql.push_str(" DESC");
    }
    if let Some(nulls) = item.nulls {
        sql.push_str(match nulls {
            NullsOrder::First => " NULLS FIRST",
            NullsOrder::Last => " NULLS LAST",
        });
    }
    Ok(Some(sql))
}

fn render_predicate_expr(
    expr: &ParsedExpr,
    active_placeholders: &HashSet<PlaceholderId>,
    active_groups: &HashSet<AstId>,
) -> Result<Option<String>> {
    render_expr(
        expr,
        active_placeholders,
        active_groups,
        ExprRenderKind::Predicate,
    )
}

fn render_value_expr(
    expr: &ParsedExpr,
    active_placeholders: &HashSet<PlaceholderId>,
    active_groups: &HashSet<AstId>,
) -> Result<Option<String>> {
    render_expr(
        expr,
        active_placeholders,
        active_groups,
        ExprRenderKind::Value,
    )
}

#[derive(Clone, Copy)]
enum ExprRenderKind {
    Value,
    Predicate,
}

fn render_expr(
    expr: &ParsedExpr,
    active_placeholders: &HashSet<PlaceholderId>,
    active_groups: &HashSet<AstId>,
    kind: ExprRenderKind,
) -> Result<Option<String>> {
    match expr {
        ParsedExpr::Column(_)
        | ParsedExpr::Literal(_)
        | ParsedExpr::Unary { .. }
        | ParsedExpr::Binary { .. }
        | ParsedExpr::IsNull { .. }
        | ParsedExpr::BoolChain { .. } => {
            render_non_optional_expr(expr, active_placeholders, active_groups, kind)
        }
        ParsedExpr::Placeholder(placeholder) => {
            if placeholder.optional && !active_placeholders.contains(&placeholder.placeholder_id) {
                return Ok(None);
            }
            Ok(Some(render_placeholder(placeholder)))
        }
        ParsedExpr::OptionalGroup(group) => {
            if !group_active(
                group.expr.as_ref(),
                group.id,
                active_placeholders,
                active_groups,
            ) {
                return Ok(None);
            }
            let inner = render_expr(
                group.expr.as_ref(),
                active_placeholders,
                active_groups,
                kind,
            )?;
            Ok(inner.map(|inner| format!("({inner})")))
        }
    }
}

fn render_non_optional_expr(
    expr: &ParsedExpr,
    active_placeholders: &HashSet<PlaceholderId>,
    active_groups: &HashSet<AstId>,
    kind: ExprRenderKind,
) -> Result<Option<String>> {
    Ok(match expr {
        ParsedExpr::Column(column) => {
            Some(format!("{}.{}", column.binding.value, column.column.value))
        }
        ParsedExpr::Placeholder(placeholder) => Some(render_placeholder(placeholder)),
        ParsedExpr::Literal(literal) => Some(render_literal(literal.value.clone())),
        ParsedExpr::OptionalGroup(_) => unreachable!("optional groups handled by render_expr"),
        ParsedExpr::Unary { op, expr, .. } => {
            let inner = render_expr(expr, active_placeholders, active_groups, kind)?;
            inner.map(|inner| match op {
                UnaryOp::Not => format!("(NOT {inner})"),
                UnaryOp::Plus => format!("(+ {inner})"),
                UnaryOp::Minus => format!("(- {inner})"),
            })
        }
        ParsedExpr::Binary {
            op, left, right, ..
        } => {
            let left = render_expr(
                left,
                active_placeholders,
                active_groups,
                ExprRenderKind::Value,
            )?;
            let right = render_expr(
                right,
                active_placeholders,
                active_groups,
                ExprRenderKind::Value,
            )?;
            match (left, right) {
                (Some(left), Some(right)) => {
                    Some(format!("({left} {} {right})", render_binary_op(op)))
                }
                _ => None,
            }
        }
        ParsedExpr::IsNull {
            negated,
            expr: inner,
            ..
        } => {
            let inner = render_expr(
                inner,
                active_placeholders,
                active_groups,
                ExprRenderKind::Value,
            )?;
            inner.map(|inner| format!("({inner} IS {}NULL)", if *negated { "NOT " } else { "" }))
        }
        ParsedExpr::BoolChain { op, terms, .. } => {
            let mut rendered_terms = Vec::new();
            for term in terms {
                if let Some(rendered) = render_expr(
                    term,
                    active_placeholders,
                    active_groups,
                    ExprRenderKind::Predicate,
                )? {
                    rendered_terms.push(rendered);
                }
            }
            match rendered_terms.len() {
                0 => None,
                1 => rendered_terms.into_iter().next(),
                _ => Some(format!(
                    "({})",
                    rendered_terms.join(match op {
                        BoolOp::And => " AND ",
                        BoolOp::Or => " OR ",
                    })
                )),
            }
        }
    })
}

fn render_expr_sql(expr: &ParsedExpr) -> String {
    match expr {
        ParsedExpr::Column(column) => format!("{}.{}", column.binding.value, column.column.value),
        ParsedExpr::Placeholder(placeholder) => render_placeholder(placeholder),
        ParsedExpr::Literal(literal) => render_literal(literal.value.clone()),
        ParsedExpr::OptionalGroup(group) => format!("({})", render_expr_sql(group.expr.as_ref())),
        ParsedExpr::Unary { op, expr, .. } => match op {
            UnaryOp::Not => format!("(NOT {})", render_expr_sql(expr)),
            UnaryOp::Plus => format!("(+ {})", render_expr_sql(expr)),
            UnaryOp::Minus => format!("(- {})", render_expr_sql(expr)),
        },
        ParsedExpr::Binary {
            op, left, right, ..
        } => {
            format!(
                "({} {} {})",
                render_expr_sql(left),
                render_binary_op(op),
                render_expr_sql(right)
            )
        }
        ParsedExpr::IsNull {
            negated,
            expr: inner,
            ..
        } => format!(
            "({} IS {}NULL)",
            render_expr_sql(inner),
            if *negated { "NOT " } else { "" }
        ),
        ParsedExpr::BoolChain { op, terms, .. } => format!(
            "({})",
            terms
                .iter()
                .map(render_expr_sql)
                .collect::<Vec<_>>()
                .join(match op {
                    BoolOp::And => " AND ",
                    BoolOp::Or => " OR ",
                })
        ),
    }
}

fn render_placeholder(placeholder: &PlaceholderRef) -> String {
    format!("${}", placeholder.slot)
}

fn renumber_sql_placeholders(sql: &str, active_slots: &[u32]) -> String {
    if active_slots.is_empty() {
        return sql.to_owned();
    }

    let slot_map = active_slots
        .iter()
        .enumerate()
        .map(|(index, slot)| (*slot, index + 1))
        .collect::<std::collections::HashMap<_, _>>();
    let mut out = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let slot = std::str::from_utf8(&bytes[i + 1..j])
                .expect("ascii digits")
                .parse::<u32>()
                .expect("ascii digits parse");
            let mapped = slot_map.get(&slot).copied().expect("active slot present");
            out.push('$');
            out.push_str(&mapped.to_string());
            i = j;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn render_literal(literal: Literal) -> String {
    match literal {
        Literal::Number(value) => value,
        Literal::String(value) => format!("'{}'", value.replace('\'', "''")),
        Literal::Boolean(true) => "TRUE".to_owned(),
        Literal::Boolean(false) => "FALSE".to_owned(),
        Literal::Null => "NULL".to_owned(),
    }
}

fn render_binary_op(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Eq => "=",
        BinaryOp::NotEq => "<>",
        BinaryOp::Lt => "<",
        BinaryOp::LtEq => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::GtEq => ">=",
    }
}

fn group_active(
    expr: &ParsedExpr,
    group_id: AstId,
    active_placeholders: &HashSet<PlaceholderId>,
    active_groups: &HashSet<AstId>,
) -> bool {
    let required = collect_optional_placeholders(expr);
    if !required.is_empty() {
        required
            .into_iter()
            .all(|placeholder| active_placeholders.contains(&placeholder))
    } else {
        active_groups.contains(&group_id)
    }
}

fn collect_optional_placeholders(expr: &ParsedExpr) -> Vec<PlaceholderId> {
    let mut placeholders = Vec::new();
    collect_optional_placeholders_into(expr, &mut placeholders);
    placeholders.sort_by_key(|placeholder| placeholder.0);
    placeholders.dedup();
    placeholders
}

fn collect_optional_placeholders_into(expr: &ParsedExpr, placeholders: &mut Vec<PlaceholderId>) {
    match expr {
        ParsedExpr::Placeholder(placeholder) => {
            if placeholder.optional {
                placeholders.push(placeholder.placeholder_id);
            }
        }
        ParsedExpr::OptionalGroup(group) => {
            collect_optional_placeholders_into(group.expr.as_ref(), placeholders);
        }
        ParsedExpr::Unary { expr, .. } | ParsedExpr::IsNull { expr, .. } => {
            collect_optional_placeholders_into(expr, placeholders);
        }
        ParsedExpr::Binary { left, right, .. } => {
            collect_optional_placeholders_into(left, placeholders);
            collect_optional_placeholders_into(right, placeholders);
        }
        ParsedExpr::BoolChain { terms, .. } => {
            for term in terms {
                collect_optional_placeholders_into(term, placeholders);
            }
        }
        ParsedExpr::Column(_) | ParsedExpr::Literal(_) => {}
    }
}

fn collect_optional_group_ids_select(select: &ParsedSelect, ids: &mut Vec<AstId>) {
    for projection in &select.projections {
        collect_optional_group_ids_expr(&projection.expr, ids);
    }
    if let Some(filter) = &select.filter {
        collect_optional_group_ids_expr(filter, ids);
    }
    for join in &select.joins {
        collect_optional_group_ids_expr(&join.on, ids);
    }
    for order_by in &select.order_by {
        collect_optional_group_ids_expr(&order_by.expr, ids);
    }
    if let Some(limit) = &select.limit {
        collect_optional_group_ids_expr(&limit.expr, ids);
    }
    if let Some(offset) = &select.offset {
        collect_optional_group_ids_expr(&offset.expr, ids);
    }
}

fn collect_optional_group_ids_expr(expr: &ParsedExpr, ids: &mut Vec<AstId>) {
    match expr {
        ParsedExpr::OptionalGroup(group) => {
            ids.push(group.id);
            collect_optional_group_ids_expr(group.expr.as_ref(), ids);
        }
        ParsedExpr::Unary { expr, .. } | ParsedExpr::IsNull { expr, .. } => {
            collect_optional_group_ids_expr(expr, ids);
        }
        ParsedExpr::Binary { left, right, .. } => {
            collect_optional_group_ids_expr(left, ids);
            collect_optional_group_ids_expr(right, ids);
        }
        ParsedExpr::BoolChain { terms, .. } => {
            for term in terms {
                collect_optional_group_ids_expr(term, ids);
            }
        }
        ParsedExpr::Column(_) | ParsedExpr::Placeholder(_) | ParsedExpr::Literal(_) => {}
    }
}

fn collect_toggle_group_ids_select(select: &CheckedSelect) -> Vec<AstId> {
    let mut ids = Vec::new();
    if let Some(filter) = &select.filter {
        collect_toggle_group_ids_expr(filter, &mut ids);
    }
    for join in &select.joins {
        collect_toggle_group_ids_expr(&join.on, &mut ids);
    }
    for order_by in &select.order_by {
        collect_toggle_group_ids_expr(&order_by.expr, &mut ids);
    }
    if let Some(limit) = &select.limit {
        collect_toggle_group_ids_expr(limit, &mut ids);
    }
    if let Some(offset) = &select.offset {
        collect_toggle_group_ids_expr(offset, &mut ids);
    }
    ids.sort_by_key(|id| id.0);
    ids.dedup();
    ids
}

fn collect_toggle_group_ids_exprs<'a>(
    exprs: impl IntoIterator<Item = Option<&'a CheckedExpr>>,
) -> Vec<AstId> {
    let mut ids = Vec::new();
    for expr in exprs.into_iter().flatten() {
        collect_toggle_group_ids_expr(expr, &mut ids);
    }
    ids.sort_by_key(|id| id.0);
    ids.dedup();
    ids
}

fn collect_toggle_group_ids_expr(expr: &CheckedExpr, ids: &mut Vec<AstId>) {
    match &expr.node {
        CheckedExprNode::OptionalGroup {
            id,
            expr,
            required_placeholders,
        } => {
            if required_placeholders.is_empty() {
                ids.push(*id);
            }
            collect_toggle_group_ids_expr(expr, ids);
        }
        CheckedExprNode::Unary { expr, .. } | CheckedExprNode::IsNull { expr, .. } => {
            collect_toggle_group_ids_expr(expr, ids);
        }
        CheckedExprNode::Binary { left, right, .. } => {
            collect_toggle_group_ids_expr(left, ids);
            collect_toggle_group_ids_expr(right, ids);
        }
        CheckedExprNode::BoolChain { terms, .. } => {
            for term in terms {
                collect_toggle_group_ids_expr(term, ids);
            }
        }
        CheckedExprNode::Column(_)
        | CheckedExprNode::Placeholder(_)
        | CheckedExprNode::Literal(_) => {}
    }
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
        SqlType::Uuid => BaseRuntimeCodec::Uuid,
        SqlType::Date => BaseRuntimeCodec::Date,
        SqlType::Time => BaseRuntimeCodec::Time,
        SqlType::Timestamp => BaseRuntimeCodec::Timestamp,
        SqlType::Timestamptz => BaseRuntimeCodec::Timestamptz,
        SqlType::Json => BaseRuntimeCodec::Json,
        SqlType::Jsonb => BaseRuntimeCodec::Jsonb,
        SqlType::Numeric => BaseRuntimeCodec::Numeric,
        unsupported => {
            return Err(TypedSqlError::unsupported_with_optional_span(
                format!(
                    "typed_sql v1 runtime lowering does not yet support SQL type `{}`; supported lowered codecs are bool, bytea, varchar, text, int2, int4, int8, float4, float8, uuid, date, time, timestamp, timestamptz, json, jsonb, and numeric",
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
            BaseRuntimeCodec::Uuid => RuntimeCodec::Uuid,
            BaseRuntimeCodec::Date => RuntimeCodec::Date,
            BaseRuntimeCodec::Time => RuntimeCodec::Time,
            BaseRuntimeCodec::Timestamp => RuntimeCodec::Timestamp,
            BaseRuntimeCodec::Timestamptz => RuntimeCodec::Timestamptz,
            BaseRuntimeCodec::Json => RuntimeCodec::Json,
            BaseRuntimeCodec::Jsonb => RuntimeCodec::Jsonb,
            BaseRuntimeCodec::Numeric => RuntimeCodec::Numeric,
        },
        Nullability::Nullable => RuntimeCodec::Nullable(base),
    })
}

pub(crate) fn runtime_codec_tokens(
    sql_type: SqlType,
    nullability: Nullability,
    span: Option<SourceSpan>,
) -> Result<TokenStream> {
    Ok(lower_runtime_codec(sql_type, nullability, span)?.tokens())
}

pub(crate) fn runtime_value_type_tokens(
    sql_type: SqlType,
    nullability: Nullability,
    span: Option<SourceSpan>,
) -> Result<TokenStream> {
    Ok(lower_runtime_codec(sql_type, nullability, span)?.value_type_tokens())
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
                    SchemaColumn::new("active", SqlType::Bool, Nullability::NonNull),
                    SchemaColumn::new("manager_id", SqlType::Int4, Nullability::Nullable),
                    SchemaColumn::new("score", SqlType::Float8, Nullability::Nullable),
                    SchemaColumn::new("name", SqlType::Text, Nullability::NonNull),
                    SchemaColumn::new("nickname", SqlType::Other("citext"), Nullability::Nullable),
                ],
            )
            .expect("users table"),
            SchemaTable::new(
                Some("public"),
                "events",
                vec![
                    SchemaColumn::new("id", SqlType::Uuid, Nullability::NonNull),
                    SchemaColumn::new("event_date", SqlType::Date, Nullability::NonNull),
                    SchemaColumn::new("event_time", SqlType::Time, Nullability::NonNull),
                    SchemaColumn::new("created_at", SqlType::Timestamp, Nullability::NonNull),
                    SchemaColumn::new("published_at", SqlType::Timestamptz, Nullability::Nullable),
                    SchemaColumn::new("payload", SqlType::Json, Nullability::NonNull),
                    SchemaColumn::new("meta", SqlType::Jsonb, Nullability::Nullable),
                    SchemaColumn::new("amount", SqlType::Numeric, Nullability::NonNull),
                ],
            )
            .expect("events table"),
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
        let checked = resolve_select(parsed.select.as_ref().expect("select"), &fixture_catalog())?;
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
            "SELECT u.id, p.name AS pet_name, u.score FROM users AS u LEFT JOIN pets AS p ON (p.owner_id = u.id) WHERE ((u.id = $1) OR (u.id = $1)) ORDER BY p.name LIMIT $2 OFFSET 4"
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
        assert!(tokens.contains("\"SELECT u.id, p.name AS pet_name, u.score FROM users AS u LEFT JOIN pets AS p ON (p.owner_id = u.id) WHERE ((u.id = $1) OR (u.id = $1)) ORDER BY p.name LIMIT $2 OFFSET 4\""));
    }

    #[test]
    fn omits_single_optional_predicates_when_inactive() {
        let lowered = parse_resolve_and_lower(
            "SELECT u.id FROM users AS u WHERE u.id = $id? AND u.active = $active?",
        )
        .expect("query lowers");

        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&[]), [])
                .expect("renders without filters"),
            "SELECT u.id FROM users AS u"
        );
        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&["id"]), [])
                .expect("renders id filter"),
            "SELECT u.id FROM users AS u WHERE (u.id = $1)"
        );
        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&["active"]), [])
                .expect("renders active filter"),
            "SELECT u.id FROM users AS u WHERE (u.active = $1)"
        );
    }

    #[test]
    fn omits_grouped_range_predicates_as_a_unit() {
        let lowered = parse_resolve_and_lower(
            "SELECT u.id FROM users AS u WHERE (u.id >= $min? AND u.id <= $max?)?",
        )
        .expect("query lowers");

        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&["min", "max"]), [])
                .expect("renders full range"),
            "SELECT u.id FROM users AS u WHERE (((u.id >= $1) AND (u.id <= $2)))"
        );
        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&["min"]), [])
                .expect("renders omitted incomplete group"),
            "SELECT u.id FROM users AS u"
        );
    }

    #[test]
    fn omits_optional_order_by_groups() {
        let lowered =
            parse_resolve_and_lower("SELECT u.id FROM users AS u ORDER BY (u.name)? DESC")
                .expect("query lowers");
        let group_id = lowered.optional_group_ids()[0];

        assert_eq!(
            lowered
                .render_sql_for([], [])
                .expect("renders without ordering"),
            "SELECT u.id FROM users AS u"
        );
        assert_eq!(
            lowered
                .render_sql_for([], [group_id])
                .expect("renders with ordering"),
            "SELECT u.id FROM users AS u ORDER BY (u.name) DESC"
        );
    }

    #[test]
    fn omits_optional_limit_and_offset_clauses() {
        let lowered =
            parse_resolve_and_lower("SELECT u.id FROM users AS u LIMIT $limit? OFFSET $offset?")
                .expect("query lowers");

        assert_eq!(
            lowered.render_sql_for([], []).expect("renders base query"),
            "SELECT u.id FROM users AS u"
        );
        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&["limit"]), [])
                .expect("renders limit only"),
            "SELECT u.id FROM users AS u LIMIT $1"
        );
        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&["offset"]), [])
                .expect("renders offset only"),
            "SELECT u.id FROM users AS u OFFSET $1"
        );
    }

    #[test]
    fn reuses_repeated_optional_placeholders_in_emitted_sql() {
        let lowered = parse_resolve_and_lower(
            "SELECT u.id FROM users AS u WHERE u.id = $id? OR u.manager_id = $id?",
        )
        .expect("query lowers");

        assert_eq!(
            lowered
                .render_sql_for(lowered.activate_parameter_names(&["id"]), [])
                .expect("renders repeated placeholder"),
            "SELECT u.id FROM users AS u WHERE ((u.id = $1) OR (u.manager_id = $1))"
        );
        assert_eq!(
            lowered
                .render_sql_for([], [])
                .expect("renders without predicate"),
            "SELECT u.id FROM users AS u"
        );
    }

    #[test]
    fn lowers_prioritized_runtime_codecs_for_parameters_and_rows() {
        let lowered = parse_resolve_and_lower(
            "SELECT e.id, e.event_date, e.event_time, e.created_at, e.published_at, e.payload, e.meta, e.amount \
             FROM events AS e \
             WHERE e.id = $id AND e.event_date = $event_date AND e.event_time = $event_time \
               AND e.created_at = $created_at AND e.published_at = $published_at \
               AND e.payload = $payload AND e.meta = $meta AND e.amount = $amount",
        )
        .expect("extended codecs should lower");

        assert_eq!(
            normalize_tokens(lowered.parameter_codec_tokens()),
            "(:: babar :: codec :: uuid , :: babar :: codec :: date , :: babar :: codec :: time , :: babar :: codec :: timestamp , :: babar :: codec :: nullable (:: babar :: codec :: timestamptz) , :: babar :: codec :: json , :: babar :: codec :: nullable (:: babar :: codec :: jsonb) , :: babar :: codec :: numeric ,)"
        );
        assert_eq!(
            normalize_tokens(lowered.row_codec_tokens()),
            "(:: babar :: codec :: uuid , :: babar :: codec :: date , :: babar :: codec :: time , :: babar :: codec :: timestamp , :: babar :: codec :: nullable (:: babar :: codec :: timestamptz) , :: babar :: codec :: json , :: babar :: codec :: nullable (:: babar :: codec :: jsonb) , :: babar :: codec :: numeric ,)"
        );
    }

    #[test]
    fn still_rejects_types_without_a_lowered_runtime_codec() {
        let err = parse_resolve_and_lower("SELECT u.nickname FROM users AS u WHERE u.id = $id")
            .expect_err("unsupported projection type should fail lowering");

        assert_eq!(err.kind, TypedSqlErrorKind::Unsupported);
        assert!(err
            .message
            .contains("runtime lowering does not yet support SQL type `citext`"));
    }
}
