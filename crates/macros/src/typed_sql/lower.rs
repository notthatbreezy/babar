use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

use super::ir::{
    AstId, BinaryOp, BoolOp, ColumnRefSyntax, JoinKind, Literal, NullsOrder, ObjectNameSyntax,
    OrderDirection, OutputNameSyntax, ParsedExpr, ParsedFrom, ParsedOrderBy,
    ParsedProjection, ParsedSelect, PlaceholderId, PlaceholderRef, UnaryOp,
};
use super::resolver::{CheckedParameter, CheckedProjection, CheckedSelect, Nullability, SqlType};
use super::{ParsedSql, Result, SourceSpan, TypedSqlError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredQuery {
    pub(crate) sql: String,
    pub(crate) parameters: Vec<LoweredParameter>,
    pub(crate) columns: Vec<LoweredColumn>,
    renderer: SqlRenderer,
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

    #[cfg(test)]
    fn render_sql_for(
        &self,
        active_placeholders: impl IntoIterator<Item = PlaceholderId>,
        active_groups: impl IntoIterator<Item = AstId>,
    ) -> Result<String> {
        self.renderer.render(
            active_placeholders.into_iter().collect(),
            active_groups.into_iter().collect(),
        )
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoweredParameter {
    pub(crate) id: PlaceholderId,
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
    let renderer = SqlRenderer::new(&parsed.select);
    let sql = renderer.render(
        parameters.iter().map(|parameter| parameter.id).collect(),
        renderer.optional_group_ids().into_iter().collect(),
    )?;

    Ok(LoweredQuery {
        sql,
        parameters,
        columns,
        renderer,
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
    select: ParsedSelect,
}

impl SqlRenderer {
    fn new(select: &ParsedSelect) -> Self {
        Self {
            select: select.clone(),
        }
    }

    fn render(
        &self,
        active_placeholders: HashSet<PlaceholderId>,
        active_groups: HashSet<AstId>,
    ) -> Result<String> {
        let mut sql = String::new();
        sql.push_str("SELECT ");
        sql.push_str(
            &self
                .select
                .projections
                .iter()
                .map(render_projection)
                .collect::<Vec<_>>()
                .join(", "),
        );
        sql.push_str(" FROM ");
        sql.push_str(&render_from(&self.select.from));
        for join in &self.select.joins {
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
        if let Some(filter) = &self.select.filter {
            if let Some(filter) = render_predicate_expr(filter, &active_placeholders, &active_groups)? {
                sql.push_str(" WHERE ");
                sql.push_str(&filter);
            }
        }

        let mut order_by = Vec::new();
        for item in &self.select.order_by {
            if let Some(rendered) = render_order_by_item(item, &active_placeholders, &active_groups)?
            {
                order_by.push(rendered);
            }
        }
        if !order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&order_by.join(", "));
        }

        if let Some(limit) = &self.select.limit {
            if let Some(limit_sql) = render_value_expr(&limit.expr, &active_placeholders, &active_groups)? {
                sql.push_str(" LIMIT ");
                sql.push_str(&limit_sql);
            }
        }
        if let Some(offset) = &self.select.offset {
            if let Some(offset_sql) =
                render_value_expr(&offset.expr, &active_placeholders, &active_groups)?
            {
                sql.push_str(" OFFSET ");
                sql.push_str(&offset_sql);
            }
        }

        Ok(sql)
    }

    fn optional_group_ids(&self) -> Vec<AstId> {
        let mut ids = Vec::new();
        collect_optional_group_ids_select(&self.select, &mut ids);
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
    render_expr(expr, active_placeholders, active_groups, ExprRenderKind::Predicate)
}

fn render_value_expr(
    expr: &ParsedExpr,
    active_placeholders: &HashSet<PlaceholderId>,
    active_groups: &HashSet<AstId>,
) -> Result<Option<String>> {
    render_expr(expr, active_placeholders, active_groups, ExprRenderKind::Value)
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
            if !group_active(group.expr.as_ref(), group.id, active_placeholders, active_groups) {
                return Ok(None);
            }
            let inner = render_expr(group.expr.as_ref(), active_placeholders, active_groups, kind)?;
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
        ParsedExpr::Column(column) => Some(format!("{}.{}", column.binding.value, column.column.value)),
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
        ParsedExpr::Binary { op, left, right, .. } => {
            let left = render_expr(left, active_placeholders, active_groups, ExprRenderKind::Value)?;
            let right = render_expr(right, active_placeholders, active_groups, ExprRenderKind::Value)?;
            match (left, right) {
                (Some(left), Some(right)) => Some(format!("({left} {} {right})", render_binary_op(op))),
                _ => None,
            }
        }
        ParsedExpr::IsNull {
            negated, expr: inner, ..
        } => {
            let inner =
                render_expr(inner, active_placeholders, active_groups, ExprRenderKind::Value)?;
            inner.map(|inner| format!("({inner} IS {}NULL)", if *negated { "NOT " } else { "" }))
        }
        ParsedExpr::BoolChain { op, terms, .. } => {
            let mut rendered_terms = Vec::new();
            for term in terms {
                if let Some(rendered) =
                    render_expr(term, active_placeholders, active_groups, ExprRenderKind::Predicate)?
                {
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
        ParsedExpr::Binary { op, left, right, .. } => {
            format!(
                "({} {} {})",
                render_expr_sql(left),
                render_binary_op(op),
                render_expr_sql(right)
            )
        }
        ParsedExpr::IsNull {
            negated, expr: inner, ..
        } => format!(
            "({} IS {}NULL)",
            render_expr_sql(inner),
            if *negated { "NOT " } else { "" }
        ),
        ParsedExpr::BoolChain { op, terms, .. } => format!(
            "({})",
            terms.iter()
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
                    SchemaColumn::new("active", SqlType::Bool, Nullability::NonNull),
                    SchemaColumn::new("manager_id", SqlType::Int4, Nullability::Nullable),
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
            "SELECT u.id FROM users AS u WHERE (u.active = $2)"
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
        let lowered = parse_resolve_and_lower(
            "SELECT u.id FROM users AS u ORDER BY (u.name)? DESC",
        )
        .expect("query lowers");
        let group_id = lowered.optional_group_ids()[0];

        assert_eq!(
            lowered.render_sql_for([], []).expect("renders without ordering"),
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
        let lowered = parse_resolve_and_lower(
            "SELECT u.id FROM users AS u LIMIT $limit? OFFSET $offset?",
        )
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
            "SELECT u.id FROM users AS u OFFSET $2"
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
            lowered.render_sql_for([], []).expect("renders without predicate"),
            "SELECT u.id FROM users AS u"
        );
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
