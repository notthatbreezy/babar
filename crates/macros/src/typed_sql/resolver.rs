use std::collections::{hash_map::Entry, HashMap};

use crate::typed_sql::ir::{
    BinaryOp, BoolOp, JoinKind, Literal, OutputNameSyntax, ParsedExpr, ParsedOrderBy,
    ParsedProjection, ParsedSelect, PlaceholderId, PlaceholderRef, SourceSpan, UnaryOp,
};

use super::{Result, TypedSqlError};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct TableId(pub(crate) usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ColumnId(pub(crate) usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct BindingId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum SqlType {
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
    Other(&'static str),
}

impl SqlType {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Bytea => "bytea",
            Self::Varchar => "varchar",
            Self::Text => "text",
            Self::Int2 => "int2",
            Self::Int4 => "int4",
            Self::Int8 => "int8",
            Self::Float4 => "float4",
            Self::Float8 => "float8",
            Self::Uuid => "uuid",
            Self::Date => "date",
            Self::Time => "time",
            Self::Timestamp => "timestamp",
            Self::Timestamptz => "timestamptz",
            Self::Json => "json",
            Self::Jsonb => "jsonb",
            Self::Numeric => "numeric",
            Self::Other(name) => name,
        }
    }

    const fn is_text_like(self) -> bool {
        matches!(self, Self::Text | Self::Varchar)
    }

    const fn is_integer_like(self) -> bool {
        matches!(self, Self::Int2 | Self::Int4 | Self::Int8)
    }

    const fn is_numeric_like(self) -> bool {
        matches!(
            self,
            Self::Int2 | Self::Int4 | Self::Int8 | Self::Float4 | Self::Float8 | Self::Numeric
        )
    }

    const fn supports_unary_sign(self) -> bool {
        self.is_numeric_like()
    }

    fn comparable_to(self, other: Self) -> bool {
        self == other
            || (self.is_text_like() && other.is_text_like())
            || (self.is_integer_like() && other.is_integer_like())
    }

    const fn limit_compatible(self) -> bool {
        self.is_integer_like()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum Nullability {
    NonNull,
    Nullable,
}

impl Nullability {
    const fn widen(self, other: Self) -> Self {
        if matches!(self, Self::Nullable) || matches!(other, Self::Nullable) {
            Self::Nullable
        } else {
            Self::NonNull
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemaCatalog {
    tables: Vec<SchemaTable>,
    tables_by_qualified_name: HashMap<Vec<String>, TableId>,
    tables_by_unqualified_name: HashMap<String, Vec<TableId>>,
}

impl SchemaCatalog {
    pub(crate) fn new(tables: Vec<SchemaTable>) -> Result<Self> {
        let mut tables_by_qualified_name = HashMap::new();
        let mut tables_by_unqualified_name = HashMap::<String, Vec<TableId>>::new();

        for (index, table) in tables.iter().enumerate() {
            let table_id = TableId(index);
            let qualified_name = table.qualified_name_parts();
            if tables_by_qualified_name
                .insert(qualified_name.clone(), table_id)
                .is_some()
            {
                return Err(TypedSqlError::internal(format!(
                    "duplicate schema table `{}` in catalog",
                    qualified_name.join("."),
                )));
            }
            tables_by_unqualified_name
                .entry(table.name.clone())
                .or_default()
                .push(table_id);
        }

        Ok(Self {
            tables,
            tables_by_qualified_name,
            tables_by_unqualified_name,
        })
    }

    fn resolve_table(&self, parts: &[String], span: SourceSpan) -> Result<TableId> {
        if let Some(&table_id) = self.tables_by_qualified_name.get(parts) {
            return Ok(table_id);
        }

        if parts.len() == 1 {
            let name = &parts[0];
            let Some(matches) = self.tables_by_unqualified_name.get(name) else {
                return Err(TypedSqlError::resolve_at(
                    format!("unknown table `{name}`"),
                    span,
                ));
            };
            if matches.len() == 1 {
                return Ok(matches[0]);
            }
            return Err(TypedSqlError::resolve_at(
                format!("table `{name}` is ambiguous; qualify it with a schema name"),
                span,
            ));
        }

        Err(TypedSqlError::resolve_at(
            format!("unknown table `{}`", parts.join(".")),
            span,
        ))
    }

    fn table(&self, table_id: TableId) -> &SchemaTable {
        &self.tables[table_id.0]
    }

    fn column(&self, table_id: TableId, column_id: ColumnId) -> &SchemaColumn {
        &self.tables[table_id.0].columns[column_id.0]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemaTable {
    schema: Option<String>,
    name: String,
    columns: Vec<SchemaColumn>,
    columns_by_name: HashMap<String, ColumnId>,
}

impl SchemaTable {
    pub(crate) fn new(
        schema: Option<&str>,
        name: &str,
        columns: Vec<SchemaColumn>,
    ) -> Result<Self> {
        let mut columns_by_name = HashMap::new();
        for (index, column) in columns.iter().enumerate() {
            match columns_by_name.entry(column.name.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(ColumnId(index));
                }
                Entry::Occupied(_) => {
                    return Err(TypedSqlError::internal(format!(
                        "duplicate column `{}` on schema table `{name}`",
                        column.name,
                    )));
                }
            }
        }

        Ok(Self {
            schema: schema.map(ToOwned::to_owned),
            name: name.to_owned(),
            columns,
            columns_by_name,
        })
    }

    fn qualified_name_parts(&self) -> Vec<String> {
        match &self.schema {
            Some(schema) => vec![schema.clone(), self.name.clone()],
            None => vec![self.name.clone()],
        }
    }

    fn display_name(&self) -> String {
        self.qualified_name_parts().join(".")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemaColumn {
    name: String,
    sql_type: SqlType,
    nullability: Nullability,
}

impl SchemaColumn {
    pub(crate) fn new(name: &str, sql_type: SqlType, nullability: Nullability) -> Self {
        Self {
            name: name.to_owned(),
            sql_type,
            nullability,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedSelect {
    pub(crate) bindings: Vec<CheckedBinding>,
    pub(crate) projections: Vec<CheckedProjection>,
    pub(crate) filter: Option<CheckedExpr>,
    pub(crate) joins: Vec<CheckedJoin>,
    pub(crate) order_by: Vec<CheckedOrderBy>,
    pub(crate) limit: Option<CheckedExpr>,
    pub(crate) offset: Option<CheckedExpr>,
    pub(crate) parameters: Vec<CheckedParameter>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedBinding {
    pub(crate) id: BindingId,
    pub(crate) binding_name: String,
    pub(crate) table_name: String,
    pub(crate) nullability: Nullability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedProjection {
    pub(crate) ordinal: u32,
    pub(crate) output_name: String,
    pub(crate) sql_type: SqlType,
    pub(crate) nullability: Nullability,
    pub(crate) expr: CheckedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedJoin {
    pub(crate) kind: JoinKind,
    pub(crate) right_binding: BindingId,
    pub(crate) on: CheckedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedOrderBy {
    pub(crate) expr: CheckedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedParameter {
    pub(crate) id: PlaceholderId,
    pub(crate) name: String,
    pub(crate) slot: u32,
    pub(crate) sql_type: SqlType,
    pub(crate) nullability: Nullability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExprKind {
    Value,
    Predicate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedExpr {
    pub(crate) span: SourceSpan,
    pub(crate) kind: ExprKind,
    pub(crate) sql_type: SqlType,
    pub(crate) nullability: Nullability,
    pub(crate) node: CheckedExprNode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedExprNode {
    Column(ResolvedColumnRef),
    Placeholder(ResolvedPlaceholderRef),
    Literal(Literal),
    Unary {
        op: UnaryOp,
        expr: Box<CheckedExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<CheckedExpr>,
        right: Box<CheckedExpr>,
    },
    IsNull {
        negated: bool,
        expr: Box<CheckedExpr>,
    },
    BoolChain {
        op: BoolOp,
        terms: Vec<CheckedExpr>,
    },
}

pub(crate) fn resolve_select(
    select: &ParsedSelect,
    catalog: &SchemaCatalog,
) -> Result<CheckedSelect> {
    let mut next_binding_id = 0u32;
    let mut bindings = Vec::<ResolvedBinding>::new();
    let mut scope = Scope::default();

    let from = resolve_from(&select.from, catalog, &mut next_binding_id, &mut scope)?;
    bindings.push(from.clone());

    let mut resolved_joins = Vec::with_capacity(select.joins.len());
    for join in &select.joins {
        let right = resolve_from(&join.right, catalog, &mut next_binding_id, &mut scope)?;
        bindings.push(right.clone());
        let on_scope = scope.clone();
        let on = resolve_expr(&join.on, &on_scope, &bindings, catalog)?;
        resolved_joins.push(ResolvedJoin {
            kind: join.kind,
            right_binding: right.id,
            on,
        });
    }

    let projections = select
        .projections
        .iter()
        .map(|projection| resolve_projection(projection, &scope, &bindings, catalog))
        .collect::<Result<Vec<_>>>()?;
    let filter = select
        .filter
        .as_ref()
        .map(|expr| resolve_expr(expr, &scope, &bindings, catalog))
        .transpose()?;
    let order_by = select
        .order_by
        .iter()
        .map(|item| resolve_order_by(item, &scope, &bindings, catalog))
        .collect::<Result<Vec<_>>>()?;
    let limit = select
        .limit
        .as_ref()
        .map(|limit| resolve_clause_expr(&limit.expr, &scope, &bindings, catalog))
        .transpose()?;
    let offset = select
        .offset
        .as_ref()
        .map(|offset| resolve_clause_expr(&offset.expr, &scope, &bindings, catalog))
        .transpose()?;

    let mut inference = InferenceContext::new();
    let mut env = RowEnv::new(from.id);
    let mut checked_join_plans = Vec::with_capacity(resolved_joins.len());

    for join in &resolved_joins {
        let on_env = env.with_binding(join.right_binding, Nullability::NonNull);
        let on = analyze_expr(&join.on, &on_env, catalog, &mut inference)?;
        require_predicate(&on, join.on.span())?;
        env = apply_join_nullability(join.kind, env, join.right_binding);
        checked_join_plans.push(CheckedJoinPlan { on_env });
    }

    if let Some(filter_expr) = &filter {
        let checked = analyze_expr(filter_expr, &env, catalog, &mut inference)?;
        require_predicate(&checked, filter_expr.span())?;
    }

    for projection in &projections {
        if !matches!(projection.output_name, ProjectionOutputName::Explicit(_))
            && !matches!(projection.expr.node, ResolvedExprNode::Column(_))
        {
            return Err(TypedSqlError::type_at(
                "typed_sql v1 requires computed projection expressions to use `AS alias`",
                projection.expr.span(),
            ));
        }
        let _ = analyze_expr(&projection.expr, &env, catalog, &mut inference)?;
    }

    for item in &order_by {
        let _ = analyze_expr(&item.expr, &env, catalog, &mut inference)?;
    }

    if let Some(limit_expr) = &limit {
        let checked = analyze_expr(limit_expr, &env, catalog, &mut inference)?;
        require_limit_like(&checked, limit_expr.span(), &mut inference)?;
    }

    if let Some(offset_expr) = &offset {
        let checked = analyze_expr(offset_expr, &env, catalog, &mut inference)?;
        require_limit_like(&checked, offset_expr.span(), &mut inference)?;
    }

    inference.solve()?;

    let checked_joins = resolved_joins
        .iter()
        .zip(checked_join_plans.iter())
        .map(|(join, plan)| {
            Ok(CheckedJoin {
                kind: join.kind,
                right_binding: join.right_binding,
                on: finalize_expr(&join.on, &plan.on_env, catalog, &inference)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let checked_filter = filter
        .as_ref()
        .map(|expr| finalize_expr(expr, &env, catalog, &inference))
        .transpose()?;

    let checked_projections = projections
        .iter()
        .enumerate()
        .map(|(ordinal, projection)| {
            let expr = finalize_expr(&projection.expr, &env, catalog, &inference)?;
            Ok(CheckedProjection {
                ordinal: ordinal as u32,
                output_name: projection.output_name.render().to_owned(),
                sql_type: expr.sql_type,
                nullability: expr.nullability,
                expr,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let checked_order_by = order_by
        .iter()
        .map(|item| {
            Ok(CheckedOrderBy {
                expr: finalize_expr(&item.expr, &env, catalog, &inference)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let checked_limit = limit
        .as_ref()
        .map(|expr| finalize_expr(expr, &env, catalog, &inference))
        .transpose()?;
    let checked_offset = offset
        .as_ref()
        .map(|expr| finalize_expr(expr, &env, catalog, &inference))
        .transpose()?;

    let checked_bindings = bindings
        .iter()
        .map(|binding| CheckedBinding {
            id: binding.id,
            binding_name: binding.binding_name.clone(),
            table_name: catalog.table(binding.table).display_name(),
            nullability: env.binding_nullability(binding.id),
        })
        .collect::<Vec<_>>();

    Ok(CheckedSelect {
        bindings: checked_bindings,
        projections: checked_projections,
        filter: checked_filter,
        joins: checked_joins,
        order_by: checked_order_by,
        limit: checked_limit,
        offset: checked_offset,
        parameters: inference.parameters(),
    })
}

#[derive(Clone, Debug, Default)]
struct Scope {
    bindings_by_name: HashMap<String, BindingId>,
}

impl Scope {
    fn insert(
        &mut self,
        binding_name: &str,
        binding_id: BindingId,
        span: SourceSpan,
    ) -> Result<()> {
        match self.bindings_by_name.entry(binding_name.to_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(binding_id);
                Ok(())
            }
            Entry::Occupied(_) => Err(TypedSqlError::resolve_at(
                format!(
                    "binding `{binding_name}` is already defined in this SELECT scope; choose a distinct alias"
                ),
                span,
            )),
        }
    }

    fn lookup(&self, binding_name: &str, span: SourceSpan) -> Result<BindingId> {
        self.bindings_by_name
            .get(binding_name)
            .copied()
            .ok_or_else(|| {
                let mut visible = self.bindings_by_name.keys().cloned().collect::<Vec<_>>();
                visible.sort();
                let suffix = if visible.is_empty() {
                    "; no relation bindings are visible here".to_owned()
                } else {
                    format!("; visible bindings: {}", visible.join(", "))
                };
                TypedSqlError::resolve_at(
                    format!("unknown table/alias binding `{binding_name}`{suffix}"),
                    span,
                )
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedBinding {
    id: BindingId,
    binding_name: String,
    table: TableId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedProjection {
    expr: ResolvedExpr,
    output_name: ProjectionOutputName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedOrderBy {
    expr: ResolvedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedJoin {
    kind: JoinKind,
    right_binding: BindingId,
    on: ResolvedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProjectionOutputName {
    Explicit(String),
    Implicit(String),
    Anonymous,
}

impl ProjectionOutputName {
    fn render(&self) -> &str {
        match self {
            Self::Explicit(name) | Self::Implicit(name) => name,
            Self::Anonymous => "<anonymous>",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedExpr {
    span: SourceSpan,
    node: ResolvedExprNode,
}

impl ResolvedExpr {
    const fn span(&self) -> SourceSpan {
        self.span
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ResolvedExprNode {
    Column(ResolvedColumnRef),
    Placeholder(ResolvedPlaceholderRef),
    Literal(Literal),
    Unary {
        op: UnaryOp,
        expr: Box<ResolvedExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<ResolvedExpr>,
        right: Box<ResolvedExpr>,
    },
    IsNull {
        negated: bool,
        expr: Box<ResolvedExpr>,
    },
    BoolChain {
        op: BoolOp,
        terms: Vec<ResolvedExpr>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedColumnRef {
    pub(crate) binding: BindingId,
    pub(crate) table: TableId,
    pub(crate) column: ColumnId,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedPlaceholderRef {
    pub(crate) id: PlaceholderId,
    pub(crate) name: String,
    pub(crate) slot: u32,
    pub(crate) span: SourceSpan,
}

fn resolve_from(
    from: &crate::typed_sql::ir::ParsedFrom,
    catalog: &SchemaCatalog,
    next_binding_id: &mut u32,
    scope: &mut Scope,
) -> Result<ResolvedBinding> {
    let table_name = from
        .table_name
        .parts
        .iter()
        .map(|part| part.value.clone())
        .collect::<Vec<_>>();
    let table = catalog.resolve_table(&table_name, from.table_name.span)?;
    let binding_name = from.binding_name.value.clone();
    let binding = ResolvedBinding {
        id: BindingId(*next_binding_id),
        binding_name: binding_name.clone(),
        table,
    };
    *next_binding_id += 1;
    scope.insert(&binding_name, binding.id, from.binding_name.span)?;
    Ok(binding)
}

fn resolve_projection(
    projection: &ParsedProjection,
    scope: &Scope,
    bindings: &[ResolvedBinding],
    catalog: &SchemaCatalog,
) -> Result<ResolvedProjection> {
    Ok(ResolvedProjection {
        expr: resolve_expr(&projection.expr, scope, bindings, catalog)?,
        output_name: match &projection.output_name {
            OutputNameSyntax::Explicit(ident) => {
                ProjectionOutputName::Explicit(ident.value.clone())
            }
            OutputNameSyntax::Implicit(ident) => {
                ProjectionOutputName::Implicit(ident.value.clone())
            }
            OutputNameSyntax::Anonymous => ProjectionOutputName::Anonymous,
        },
    })
}

fn resolve_order_by(
    order_by: &ParsedOrderBy,
    scope: &Scope,
    bindings: &[ResolvedBinding],
    catalog: &SchemaCatalog,
) -> Result<ResolvedOrderBy> {
    Ok(ResolvedOrderBy {
        expr: resolve_expr(&order_by.expr, scope, bindings, catalog)?,
    })
}

fn resolve_clause_expr(
    expr: &ParsedExpr,
    scope: &Scope,
    bindings: &[ResolvedBinding],
    catalog: &SchemaCatalog,
) -> Result<ResolvedExpr> {
    resolve_expr(expr, scope, bindings, catalog)
}

fn resolve_expr(
    expr: &ParsedExpr,
    scope: &Scope,
    bindings: &[ResolvedBinding],
    catalog: &SchemaCatalog,
) -> Result<ResolvedExpr> {
    let span = expr_span(expr);
    let node = match expr {
        ParsedExpr::Column(column) => {
            let binding = scope.lookup(&column.binding.value, column.binding.span)?;
            let resolved_binding = &bindings[binding.0 as usize];
            let table = catalog.table(resolved_binding.table);
            let Some(&column_id) = table.columns_by_name.get(&column.column.value) else {
                let available = table
                    .columns
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(TypedSqlError::resolve_at(
                    format!(
                        "unknown column `{}.{}` on bound table `{}`; available columns: {}",
                        column.binding.value,
                        column.column.value,
                        table.display_name(),
                        available,
                    ),
                    column.column.span,
                ));
            };
            ResolvedExprNode::Column(ResolvedColumnRef {
                binding,
                table: resolved_binding.table,
                column: column_id,
                span,
            })
        }
        ParsedExpr::Placeholder(placeholder) => {
            ResolvedExprNode::Placeholder(ResolvedPlaceholderRef::from_placeholder(placeholder))
        }
        ParsedExpr::Literal(literal) => ResolvedExprNode::Literal(literal.value.clone()),
        ParsedExpr::Unary { op, expr, .. } => ResolvedExprNode::Unary {
            op: *op,
            expr: Box::new(resolve_expr(expr, scope, bindings, catalog)?),
        },
        ParsedExpr::Binary {
            op, left, right, ..
        } => ResolvedExprNode::Binary {
            op: op.clone(),
            left: Box::new(resolve_expr(left, scope, bindings, catalog)?),
            right: Box::new(resolve_expr(right, scope, bindings, catalog)?),
        },
        ParsedExpr::IsNull { negated, expr, .. } => ResolvedExprNode::IsNull {
            negated: *negated,
            expr: Box::new(resolve_expr(expr, scope, bindings, catalog)?),
        },
        ParsedExpr::BoolChain { op, terms, .. } => ResolvedExprNode::BoolChain {
            op: *op,
            terms: terms
                .iter()
                .map(|term| resolve_expr(term, scope, bindings, catalog))
                .collect::<Result<Vec<_>>>()?,
        },
    };
    Ok(ResolvedExpr { span, node })
}

impl ResolvedPlaceholderRef {
    fn from_placeholder(placeholder: &PlaceholderRef) -> Self {
        Self {
            id: placeholder.placeholder_id,
            name: placeholder.name.clone(),
            slot: placeholder.slot,
            span: placeholder.span,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RowEnv {
    binding_nullability: HashMap<BindingId, Nullability>,
}

impl RowEnv {
    fn new(base_binding: BindingId) -> Self {
        Self {
            binding_nullability: HashMap::from([(base_binding, Nullability::NonNull)]),
        }
    }

    fn with_binding(&self, binding: BindingId, nullability: Nullability) -> Self {
        let mut next = self.clone();
        next.binding_nullability.insert(binding, nullability);
        next
    }

    fn binding_nullability(&self, binding: BindingId) -> Nullability {
        self.binding_nullability
            .get(&binding)
            .copied()
            .unwrap_or(Nullability::NonNull)
    }

    fn mark_all_nullable(mut self) -> Self {
        for nullability in self.binding_nullability.values_mut() {
            *nullability = Nullability::Nullable;
        }
        self
    }
}

fn apply_join_nullability(kind: JoinKind, env: RowEnv, right_binding: BindingId) -> RowEnv {
    match kind {
        JoinKind::Inner => env.with_binding(right_binding, Nullability::NonNull),
        JoinKind::Left => env.with_binding(right_binding, Nullability::Nullable),
        JoinKind::Right => env
            .mark_all_nullable()
            .with_binding(right_binding, Nullability::NonNull),
        JoinKind::Full => env
            .mark_all_nullable()
            .with_binding(right_binding, Nullability::Nullable),
    }
}

#[derive(Clone, Debug)]
struct CheckedJoinPlan {
    on_env: RowEnv,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ValueType {
    Concrete(SqlType),
    Placeholder(PlaceholderId),
    IntegerLiteral,
    NumericLiteral,
    StringLiteral,
    Null,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExprMeta {
    kind: ExprKind,
    value_type: ValueType,
    nullability: Nullability,
}

fn analyze_expr(
    expr: &ResolvedExpr,
    env: &RowEnv,
    catalog: &SchemaCatalog,
    inference: &mut InferenceContext,
) -> Result<ExprMeta> {
    match &expr.node {
        ResolvedExprNode::Column(column) => {
            let binding_nullability = env.binding_nullability(column.binding);
            let schema_column = catalog.column(column.table, column.column);
            Ok(ExprMeta {
                kind: ExprKind::Value,
                value_type: ValueType::Concrete(schema_column.sql_type),
                nullability: schema_column.nullability.widen(binding_nullability),
            })
        }
        ResolvedExprNode::Placeholder(placeholder) => {
            inference.observe_placeholder(placeholder);
            Ok(ExprMeta {
                kind: ExprKind::Value,
                value_type: ValueType::Placeholder(placeholder.id),
                nullability: inference.placeholder_nullability(placeholder.id),
            })
        }
        ResolvedExprNode::Literal(literal) => Ok(ExprMeta {
            kind: ExprKind::Value,
            value_type: literal_value_type(literal),
            nullability: if matches!(literal, Literal::Null) {
                Nullability::Nullable
            } else {
                Nullability::NonNull
            },
        }),
        ResolvedExprNode::Unary { op, expr: inner } => {
            let inner = analyze_expr(inner, env, catalog, inference)?;
            match op {
                UnaryOp::Not => {
                    if inner.kind != ExprKind::Predicate {
                        return Err(TypedSqlError::type_at(
                            format!(
                                "typed_sql v1 requires `NOT` to apply to a predicate expression, but this operand is {}",
                                describe_expr_meta(&inner)
                            ),
                            expr.span(),
                        ));
                    }
                    Ok(ExprMeta {
                        kind: ExprKind::Predicate,
                        value_type: ValueType::Concrete(SqlType::Bool),
                        nullability: inner.nullability,
                    })
                }
                UnaryOp::Plus | UnaryOp::Minus => {
                    ensure_signed_numeric_operand(&inner, expr.span())?;
                    Ok(ExprMeta {
                        kind: ExprKind::Value,
                        value_type: inner.value_type,
                        nullability: inner.nullability,
                    })
                }
            }
        }
        ResolvedExprNode::Binary { left, right, .. } => {
            let left_span = left.span();
            let right_span = right.span();
            let left = analyze_expr(left, env, catalog, inference)?;
            let right = analyze_expr(right, env, catalog, inference)?;
            constrain_comparison(&left, left_span, &right, right_span, expr.span(), inference)?;
            Ok(ExprMeta {
                kind: ExprKind::Predicate,
                value_type: ValueType::Concrete(SqlType::Bool),
                nullability: left.nullability.widen(right.nullability),
            })
        }
        ResolvedExprNode::IsNull { expr: inner, .. } => {
            let _ = analyze_expr(inner, env, catalog, inference)?;
            Ok(ExprMeta {
                kind: ExprKind::Predicate,
                value_type: ValueType::Concrete(SqlType::Bool),
                nullability: Nullability::NonNull,
            })
        }
        ResolvedExprNode::BoolChain { terms, .. } => {
            let mut nullability = Nullability::NonNull;
            for term in terms {
                let term = analyze_expr(term, env, catalog, inference)?;
                if term.kind != ExprKind::Predicate {
                    return Err(TypedSqlError::type_at(
                        format!(
                            "typed_sql v1 requires boolean chains to contain predicate expressions, but one term is {}",
                            describe_expr_meta(&term)
                        ),
                        expr.span(),
                    ));
                }
                nullability = nullability.widen(term.nullability);
            }
            Ok(ExprMeta {
                kind: ExprKind::Predicate,
                value_type: ValueType::Concrete(SqlType::Bool),
                nullability,
            })
        }
    }
}

fn ensure_signed_numeric_operand(meta: &ExprMeta, span: SourceSpan) -> Result<()> {
    match meta.value_type {
        ValueType::Concrete(sql_type) if sql_type.supports_unary_sign() => Ok(()),
        ValueType::Placeholder(_) | ValueType::IntegerLiteral | ValueType::NumericLiteral => Ok(()),
        _ => Err(TypedSqlError::type_at(
            format!(
                "typed_sql v1 only supports unary `+` and `-` on numeric expressions, but this operand is {}",
                describe_expr_meta(meta)
            ),
            span,
        )),
    }
}

fn constrain_comparison(
    left: &ExprMeta,
    left_span: SourceSpan,
    right: &ExprMeta,
    right_span: SourceSpan,
    span: SourceSpan,
    inference: &mut InferenceContext,
) -> Result<()> {
    match (&left.value_type, &right.value_type) {
        (ValueType::Null, _) | (_, ValueType::Null) => Err(TypedSqlError::type_at(
            "typed_sql v1 requires `IS NULL` / `IS NOT NULL` instead of direct `NULL` comparisons",
            span,
        )),
        (ValueType::Concrete(left), ValueType::Concrete(right)) => {
            if left.comparable_to(*right) {
                Ok(())
            } else {
                Err(TypedSqlError::type_at(
                    format!(
                        "typed_sql v1 cannot compare `{}` to `{}`",
                        left.name(),
                        right.name(),
                    ),
                    span,
                ))
            }
        }
        (ValueType::Concrete(expected), ValueType::Placeholder(placeholder)) => {
            inference.expect_placeholder(*placeholder, *expected, right.nullability, right_span)
        }
        (ValueType::Placeholder(placeholder), ValueType::Concrete(expected)) => {
            inference.expect_placeholder(*placeholder, *expected, left.nullability, left_span)
        }
        (ValueType::Placeholder(left_placeholder), ValueType::Placeholder(right_placeholder)) => {
            inference.relate_placeholders(*left_placeholder, *right_placeholder, span);
            Ok(())
        }
        (ValueType::Concrete(concrete), literal) | (literal, ValueType::Concrete(concrete)) => {
            if literal_compatible(*concrete, literal) {
                Ok(())
            } else {
                Err(TypedSqlError::type_at(
                    format!(
                        "typed_sql v1 cannot compare `{}` to `{}`",
                        concrete.name(),
                        describe_value_type(literal),
                    ),
                    span,
                ))
            }
        }
        (
            ValueType::Placeholder(_),
            ValueType::IntegerLiteral | ValueType::NumericLiteral | ValueType::StringLiteral,
        )
        | (
            ValueType::IntegerLiteral | ValueType::NumericLiteral | ValueType::StringLiteral,
            ValueType::Placeholder(_),
        ) => Ok(()),
        (left, right) => {
            if literals_comparable(left, right) {
                Ok(())
            } else {
                Err(TypedSqlError::type_at(
                    format!(
                        "typed_sql v1 cannot compare `{}` to `{}`",
                        describe_value_type(left),
                        describe_value_type(right),
                    ),
                    span,
                ))
            }
        }
    }
}

fn require_predicate(expr: &ExprMeta, span: SourceSpan) -> Result<()> {
    if expr.kind == ExprKind::Predicate {
        Ok(())
    } else {
        Err(TypedSqlError::type_at(
            format!(
                "typed_sql v1 requires predicate expressions in `WHERE` and `JOIN ... ON` clauses, but this expression is {}",
                describe_expr_meta(expr)
            ),
            span,
        ))
    }
}

fn require_limit_like(
    expr: &ExprMeta,
    span: SourceSpan,
    inference: &mut InferenceContext,
) -> Result<()> {
    match expr.value_type {
        ValueType::Concrete(sql_type) if sql_type.limit_compatible() => Ok(()),
        ValueType::Placeholder(placeholder) => {
            inference.expect_placeholder(placeholder, SqlType::Int8, Nullability::NonNull, span)
        }
        ValueType::IntegerLiteral => Ok(()),
        _ => Err(TypedSqlError::type_at(
            format!(
                "typed_sql v1 requires LIMIT/OFFSET expressions to be int8-compatible, but this expression is {}",
                describe_expr_meta(expr)
            ),
            span,
        )),
    }
}

fn finalize_expr(
    expr: &ResolvedExpr,
    env: &RowEnv,
    catalog: &SchemaCatalog,
    inference: &InferenceContext,
) -> Result<CheckedExpr> {
    match &expr.node {
        ResolvedExprNode::Column(column) => {
            let binding_nullability = env.binding_nullability(column.binding);
            let schema_column = catalog.column(column.table, column.column);
            Ok(CheckedExpr {
                span: expr.span(),
                kind: ExprKind::Value,
                sql_type: schema_column.sql_type,
                nullability: schema_column.nullability.widen(binding_nullability),
                node: CheckedExprNode::Column(column.clone()),
            })
        }
        ResolvedExprNode::Placeholder(placeholder) => Ok(CheckedExpr {
            span: expr.span(),
            kind: ExprKind::Value,
            sql_type: inference
                .placeholder_sql_type(placeholder.id)
                .ok_or_else(|| {
                    TypedSqlError::internal(format!(
                        "placeholder `${}` should be solved before finalization",
                        placeholder.name,
                    ))
                })?,
            nullability: inference.placeholder_nullability(placeholder.id),
            node: CheckedExprNode::Placeholder(placeholder.clone()),
        }),
        ResolvedExprNode::Literal(literal) => {
            let Some(sql_type) = finalize_literal_type(literal) else {
                return Err(TypedSqlError::type_at(
                    "typed_sql v1 does not infer a concrete type for bare `NULL` literals",
                    expr.span(),
                ));
            };
            Ok(CheckedExpr {
                span: expr.span(),
                kind: ExprKind::Value,
                sql_type,
                nullability: if matches!(literal, Literal::Null) {
                    Nullability::Nullable
                } else {
                    Nullability::NonNull
                },
                node: CheckedExprNode::Literal(literal.clone()),
            })
        }
        ResolvedExprNode::Unary { op, expr: inner } => {
            let inner = finalize_expr(inner, env, catalog, inference)?;
            match op {
                UnaryOp::Not => Ok(CheckedExpr {
                    span: expr.span(),
                    kind: ExprKind::Predicate,
                    sql_type: SqlType::Bool,
                    nullability: inner.nullability,
                    node: CheckedExprNode::Unary {
                        op: *op,
                        expr: Box::new(inner),
                    },
                }),
                UnaryOp::Plus | UnaryOp::Minus => {
                    if !inner.sql_type.supports_unary_sign() {
                        return Err(TypedSqlError::type_at(
                            format!(
                                "typed_sql v1 only supports unary `+` and `-` on numeric expressions, but this operand has SQL type `{}`",
                                inner.sql_type.name()
                            ),
                            expr.span(),
                        ));
                    }
                    Ok(CheckedExpr {
                        span: expr.span(),
                        kind: ExprKind::Value,
                        sql_type: inner.sql_type,
                        nullability: inner.nullability,
                        node: CheckedExprNode::Unary {
                            op: *op,
                            expr: Box::new(inner),
                        },
                    })
                }
            }
        }
        ResolvedExprNode::Binary { op, left, right } => {
            let left = finalize_expr(left, env, catalog, inference)?;
            let right = finalize_expr(right, env, catalog, inference)?;
            Ok(CheckedExpr {
                span: expr.span(),
                kind: ExprKind::Predicate,
                sql_type: SqlType::Bool,
                nullability: left.nullability.widen(right.nullability),
                node: CheckedExprNode::Binary {
                    op: op.clone(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
            })
        }
        ResolvedExprNode::IsNull {
            negated,
            expr: inner,
        } => Ok(CheckedExpr {
            span: expr.span(),
            kind: ExprKind::Predicate,
            sql_type: SqlType::Bool,
            nullability: Nullability::NonNull,
            node: CheckedExprNode::IsNull {
                negated: *negated,
                expr: Box::new(finalize_expr(inner, env, catalog, inference)?),
            },
        }),
        ResolvedExprNode::BoolChain { op, terms } => {
            let terms = terms
                .iter()
                .map(|term| finalize_expr(term, env, catalog, inference))
                .collect::<Result<Vec<_>>>()?;
            let nullability = terms.iter().fold(Nullability::NonNull, |acc, term| {
                acc.widen(term.nullability)
            });
            Ok(CheckedExpr {
                span: expr.span(),
                kind: ExprKind::Predicate,
                sql_type: SqlType::Bool,
                nullability,
                node: CheckedExprNode::BoolChain { op: *op, terms },
            })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InferenceContext {
    placeholders: HashMap<PlaceholderId, PlaceholderState>,
    pending: Vec<PendingConstraint>,
}

impl InferenceContext {
    fn new() -> Self {
        Self {
            placeholders: HashMap::new(),
            pending: Vec::new(),
        }
    }

    fn expect_placeholder(
        &mut self,
        placeholder: PlaceholderId,
        expected: SqlType,
        nullability: Nullability,
        span: SourceSpan,
    ) -> Result<()> {
        let state = self.placeholder_state_mut(placeholder, span);
        state.nullability = state.nullability.widen(nullability);
        match state.inferred_type {
            Some(actual) if actual != expected => Err(TypedSqlError::type_at(
                format!(
                    "placeholder `${}` was inferred as `{}` but is also required to be `{}`",
                    state.name.as_deref().unwrap_or("?"),
                    actual.name(),
                    expected.name(),
                ),
                span,
            )),
            Some(_) => Ok(()),
            None => {
                state.inferred_type = Some(expected);
                Ok(())
            }
        }
    }

    fn observe_placeholder(&mut self, placeholder: &ResolvedPlaceholderRef) {
        let state = self.placeholder_state_mut(placeholder.id, placeholder.span);
        state.name = Some(placeholder.name.clone());
        state.slot = Some(placeholder.slot);
    }

    fn relate_placeholders(&mut self, left: PlaceholderId, right: PlaceholderId, span: SourceSpan) {
        if left != right {
            self.pending.push(PendingConstraint { left, right, span });
        }
        let _ = self.placeholder_state_mut(left, span);
        let _ = self.placeholder_state_mut(right, span);
    }

    fn placeholder_nullability(&self, placeholder: PlaceholderId) -> Nullability {
        self.placeholders
            .get(&placeholder)
            .map(|state| state.nullability)
            .unwrap_or(Nullability::NonNull)
    }

    fn placeholder_sql_type(&self, placeholder: PlaceholderId) -> Option<SqlType> {
        self.placeholders
            .get(&placeholder)
            .and_then(|state| state.inferred_type)
    }

    fn placeholder_state_mut(
        &mut self,
        placeholder: PlaceholderId,
        span: SourceSpan,
    ) -> &mut PlaceholderState {
        self.placeholders
            .entry(placeholder)
            .or_insert_with(|| PlaceholderState {
                id: placeholder,
                name: None,
                slot: None,
                inferred_type: None,
                nullability: Nullability::NonNull,
                first_span: span,
            })
    }

    fn solve(&mut self) -> Result<()> {
        loop {
            let mut changed = false;
            for constraint in self.pending.clone() {
                let left = self
                    .placeholders
                    .get(&constraint.left)
                    .and_then(|state| state.inferred_type);
                let right = self
                    .placeholders
                    .get(&constraint.right)
                    .and_then(|state| state.inferred_type);
                match (left, right) {
                    (Some(left), Some(right)) if left != right => {
                        return Err(TypedSqlError::type_at(
                            format!(
                                "placeholders in the same comparison must solve to one SQL type, but `{}` and `{}` differ",
                                left.name(),
                                right.name(),
                            ),
                            constraint.span,
                        ));
                    }
                    (Some(left), None) => {
                        self.expect_placeholder(
                            constraint.right,
                            left,
                            self.placeholder_nullability(constraint.left),
                            constraint.span,
                        )?;
                        changed = true;
                    }
                    (None, Some(right)) => {
                        self.expect_placeholder(
                            constraint.left,
                            right,
                            self.placeholder_nullability(constraint.right),
                            constraint.span,
                        )?;
                        changed = true;
                    }
                    _ => {}
                }
            }
            if !changed {
                break;
            }
        }

        let mut unresolved = self
            .placeholders
            .values()
            .filter(|state| state.inferred_type.is_none())
            .collect::<Vec<_>>();
        unresolved.sort_by_key(|state| state.id.0);
        if let Some(state) = unresolved.first() {
            return Err(TypedSqlError::type_at(
                format!(
                    "placeholder `${}` is never constrained to a concrete SQL type in typed_sql v1",
                    state.name.as_deref().unwrap_or("?"),
                ),
                state.first_span,
            ));
        }
        Ok(())
    }

    fn parameters(&self) -> Vec<CheckedParameter> {
        let mut parameters = self
            .placeholders
            .values()
            .filter_map(|state| {
                state.inferred_type.map(|sql_type| CheckedParameter {
                    id: state.id,
                    name: state
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("p{}", state.id.0 + 1)),
                    slot: state.slot.unwrap_or(state.id.0 + 1),
                    sql_type,
                    nullability: state.nullability,
                })
            })
            .collect::<Vec<_>>();
        parameters.sort_by_key(|parameter| parameter.slot);
        parameters
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlaceholderState {
    id: PlaceholderId,
    name: Option<String>,
    slot: Option<u32>,
    inferred_type: Option<SqlType>,
    nullability: Nullability,
    first_span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingConstraint {
    left: PlaceholderId,
    right: PlaceholderId,
    span: SourceSpan,
}

fn literal_value_type(literal: &Literal) -> ValueType {
    match literal {
        Literal::Number(number) => {
            if is_integer_literal(number) {
                ValueType::IntegerLiteral
            } else {
                ValueType::NumericLiteral
            }
        }
        Literal::String(_) => ValueType::StringLiteral,
        Literal::Boolean(_) => ValueType::Concrete(SqlType::Bool),
        Literal::Null => ValueType::Null,
    }
}

fn finalize_literal_type(literal: &Literal) -> Option<SqlType> {
    match literal_value_type(literal) {
        ValueType::Concrete(sql_type) => Some(sql_type),
        ValueType::IntegerLiteral => Some(SqlType::Int8),
        ValueType::NumericLiteral => Some(SqlType::Numeric),
        ValueType::StringLiteral => Some(SqlType::Text),
        ValueType::Null => None,
        ValueType::Placeholder(_) => None,
    }
}

fn literal_compatible(concrete: SqlType, literal: &ValueType) -> bool {
    match literal {
        ValueType::IntegerLiteral => concrete.is_numeric_like(),
        ValueType::NumericLiteral => matches!(
            concrete,
            SqlType::Float4 | SqlType::Float8 | SqlType::Numeric
        ),
        ValueType::StringLiteral => concrete.is_text_like(),
        _ => false,
    }
}

fn literals_comparable(left: &ValueType, right: &ValueType) -> bool {
    matches!(
        (left, right),
        (ValueType::IntegerLiteral, ValueType::IntegerLiteral)
            | (ValueType::NumericLiteral, ValueType::NumericLiteral)
            | (ValueType::IntegerLiteral, ValueType::NumericLiteral)
            | (ValueType::NumericLiteral, ValueType::IntegerLiteral)
            | (ValueType::StringLiteral, ValueType::StringLiteral)
    )
}

fn describe_value_type(value_type: &ValueType) -> &'static str {
    match value_type {
        ValueType::Concrete(sql_type) => sql_type.name(),
        ValueType::Placeholder(_) => "placeholder",
        ValueType::IntegerLiteral => "integer literal",
        ValueType::NumericLiteral => "numeric literal",
        ValueType::StringLiteral => "string literal",
        ValueType::Null => "NULL",
    }
}

fn is_integer_literal(number: &str) -> bool {
    number
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'_' || byte == b'-' || byte == b'+')
}

fn expr_span(expr: &ParsedExpr) -> SourceSpan {
    match expr {
        ParsedExpr::Column(column) => column.span,
        ParsedExpr::Placeholder(placeholder) => placeholder.span,
        ParsedExpr::Literal(literal) => literal.span,
        ParsedExpr::Unary { span, .. }
        | ParsedExpr::Binary { span, .. }
        | ParsedExpr::IsNull { span, .. }
        | ParsedExpr::BoolChain { span, .. } => *span,
    }
}

fn describe_expr_meta(expr: &ExprMeta) -> String {
    match expr.kind {
        ExprKind::Predicate => "a predicate expression".to_owned(),
        ExprKind::Value => format!(
            "a value expression of SQL type `{}`",
            describe_value_type(&expr.value_type)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_sql::parse_select;
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

    fn parse_and_resolve(sql: &str) -> Result<CheckedSelect> {
        let parsed = parse_select(sql)?;
        resolve_select(&parsed.select, &fixture_catalog())
    }

    #[test]
    fn resolves_bindings_columns_and_parameter_types() {
        let checked = parse_and_resolve(
            "SELECT u.id, p.name AS pet_name FROM users AS u INNER JOIN pets AS p ON p.owner_id = u.id WHERE u.id = $id ORDER BY p.name LIMIT $limit",
        )
        .expect("query resolves");

        assert_eq!(checked.bindings.len(), 2);
        assert_eq!(checked.bindings[0].binding_name, "u");
        assert_eq!(checked.bindings[0].nullability, Nullability::NonNull);
        assert_eq!(checked.bindings[1].binding_name, "p");
        assert_eq!(checked.bindings[1].nullability, Nullability::NonNull);

        assert_eq!(checked.projections[0].output_name, "id");
        assert_eq!(checked.projections[0].sql_type, SqlType::Int4);
        assert_eq!(checked.projections[0].nullability, Nullability::NonNull);
        assert_eq!(checked.projections[1].output_name, "pet_name");
        assert_eq!(checked.projections[1].sql_type, SqlType::Text);

        assert_eq!(checked.parameters.len(), 2);
        assert_eq!(checked.parameters[0].slot, 1);
        assert_eq!(checked.parameters[0].sql_type, SqlType::Int4);
        assert_eq!(checked.parameters[1].slot, 2);
        assert_eq!(checked.parameters[1].sql_type, SqlType::Int8);

        assert_eq!(checked.joins[0].on.kind, ExprKind::Predicate);
        assert_eq!(
            checked.filter.as_ref().expect("where").kind,
            ExprKind::Predicate
        );
    }

    #[test]
    fn outer_join_widens_downstream_nullability() {
        let checked = parse_and_resolve(
            "SELECT u.id, p.name AS pet_name FROM users AS u LEFT JOIN pets AS p ON p.owner_id = u.id",
        )
        .expect("query resolves");

        assert_eq!(checked.bindings[0].nullability, Nullability::NonNull);
        assert_eq!(checked.bindings[1].nullability, Nullability::Nullable);
        assert_eq!(checked.projections[0].nullability, Nullability::NonNull);
        assert_eq!(checked.projections[1].nullability, Nullability::Nullable);
    }

    #[test]
    fn right_join_widens_left_binding_nullability() {
        let checked = parse_and_resolve(
            "SELECT u.id, p.name AS pet_name FROM users AS u RIGHT JOIN pets AS p ON p.owner_id = u.id",
        )
        .expect("query resolves");

        assert_eq!(checked.bindings[0].nullability, Nullability::Nullable);
        assert_eq!(checked.bindings[1].nullability, Nullability::NonNull);
        assert_eq!(checked.projections[0].nullability, Nullability::Nullable);
        assert_eq!(checked.projections[1].nullability, Nullability::NonNull);
    }

    #[test]
    fn full_join_widens_both_sides_nullability() {
        let checked = parse_and_resolve(
            "SELECT u.id, p.name AS pet_name FROM users AS u FULL JOIN pets AS p ON p.owner_id = u.id",
        )
        .expect("query resolves");

        assert_eq!(checked.bindings[0].nullability, Nullability::Nullable);
        assert_eq!(checked.bindings[1].nullability, Nullability::Nullable);
        assert_eq!(checked.projections[0].nullability, Nullability::Nullable);
        assert_eq!(checked.projections[1].nullability, Nullability::Nullable);
    }

    #[test]
    fn placeholder_constraints_propagate_across_comparisons() {
        let checked =
            parse_and_resolve("SELECT u.id FROM users AS u WHERE u.id = $id AND $id = $other")
                .expect("query resolves");

        assert_eq!(checked.parameters.len(), 2);
        assert_eq!(checked.parameters[0].sql_type, SqlType::Int4);
        assert_eq!(checked.parameters[1].sql_type, SqlType::Int4);
    }

    #[test]
    fn alias_scope_rejects_original_table_name_lookup() {
        let err = parse_and_resolve("SELECT users.id FROM users AS u")
            .expect_err("alias should hide original table name");

        assert_eq!(err.kind, TypedSqlErrorKind::Resolve);
        assert!(err.message.contains("unknown table/alias binding `users`"));
        assert!(err.message.contains("visible bindings: u"));
        assert_eq!(err.span, Some(SourceSpan::new(7, 12)));
    }

    #[test]
    fn where_requires_predicate_expression() {
        let err = parse_and_resolve("SELECT u.id FROM users AS u WHERE u.active")
            .expect_err("scalar bool column should be rejected in WHERE");

        assert_eq!(err.kind, TypedSqlErrorKind::Type);
        assert!(err.message.contains("predicate expressions"));
        assert!(err.message.contains("SQL type `bool`"));
        assert_eq!(err.span, Some(SourceSpan::new(34, 42)));
    }

    #[test]
    fn computed_projection_requires_alias() {
        let err = parse_and_resolve("SELECT u.id = $id FROM users AS u")
            .expect_err("computed projection should require alias");

        assert_eq!(err.kind, TypedSqlErrorKind::Type);
        assert!(err.message.contains("computed projection expressions"));
    }

    #[test]
    fn unconstrained_placeholder_is_rejected() {
        let err = parse_and_resolve("SELECT $id AS id FROM users AS u")
            .expect_err("placeholder should need a concrete type");

        assert_eq!(err.kind, TypedSqlErrorKind::Type);
        assert!(err.message.contains("never constrained"));
        assert_eq!(err.span, Some(SourceSpan::new(7, 10)));
    }

    #[test]
    fn unknown_column_reports_available_columns() {
        let err = parse_and_resolve("SELECT u.nickname FROM users AS u")
            .expect_err("unknown column should be rejected");

        assert_eq!(err.kind, TypedSqlErrorKind::Resolve);
        assert!(err
            .message
            .contains("available columns: id, active, manager_id, name"));
        assert_eq!(err.span, Some(SourceSpan::new(9, 17)));
    }

    #[test]
    fn placeholder_type_conflicts_point_at_placeholder_occurrence() {
        let err =
            parse_and_resolve("SELECT u.id FROM users AS u WHERE u.id = $id AND u.name = $id")
                .expect_err("placeholder should not solve to two concrete types");

        assert_eq!(err.kind, TypedSqlErrorKind::Type);
        assert!(err
            .message
            .contains("placeholder `$id` was inferred as `int4`"));
        assert_eq!(err.span, Some(SourceSpan::new(58, 61)));
    }
}
