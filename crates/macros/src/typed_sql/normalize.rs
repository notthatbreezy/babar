use std::collections::HashSet;

use sqlparser::{
    ast::Spanned,
    ast::{
        Assignment, AssignmentTarget, BinaryOperator, Delete, Expr, FromTable, Ident, Insert, Join,
        JoinConstraint, JoinOperator, LimitClause, ObjectName, ObjectNamePart, OrderBy,
        OrderByExpr, OrderByKind, Query, Select, SelectItem, SetExpr, Statement, TableAlias,
        TableFactor, TableObject, TableWithJoins, UnaryOperator, Update, Value, ValueWithSpan,
        Values,
    },
};

use crate::typed_sql::ir::{
    AstId, BinaryOp, BindingNameSyntax, BoolOp, ColumnRefSyntax, IdentSyntax, JoinKind, Literal,
    NullsOrder, ObjectNameSyntax, OrderDirection, OutputNameSyntax, ParsedAssignment,
    ParsedAssignmentTarget, ParsedCommandStatement, ParsedDelete, ParsedExpr, ParsedFrom,
    ParsedInsert, ParsedJoin, ParsedLimit, ParsedLiteral, ParsedOffset, ParsedOptionalGroup,
    ParsedOrderBy, ParsedProjection, ParsedRowStatement, ParsedSelect, ParsedStatement,
    ParsedUpdate, ParsedValuesRow, PlaceholderId, PlaceholderRef, SourceSpan, StatementKind,
    UnaryOp,
};

use super::{
    source::{PlaceholderEntry, SqlSource},
    Result, TypedSqlError,
};

pub(crate) fn normalize_select(source: &SqlSource, query: &Query) -> Result<ParsedSelect> {
    let mut normalizer = Normalizer::new(source);
    normalizer.normalize_query(query)
}

pub(crate) fn normalize_statement(
    source: &SqlSource,
    statement: &Statement,
) -> Result<ParsedStatement> {
    let mut normalizer = Normalizer::new(source);
    normalizer.normalize_statement(statement)
}

struct Normalizer<'a> {
    source: &'a SqlSource,
    consumed_optional_groups: HashSet<SourceSpan>,
    next_id: u32,
}

impl<'a> Normalizer<'a> {
    fn new(source: &'a SqlSource) -> Self {
        Self {
            source,
            consumed_optional_groups: HashSet::new(),
            next_id: 0,
        }
    }

    fn normalize_query(&mut self, query: &Query) -> Result<ParsedSelect> {
        if query.with.is_some() {
            return Err(self.unsupported_node(query, "typed_sql v1 does not support WITH clauses"));
        }
        if query.fetch.is_some() {
            return Err(self.unsupported_node(query, "typed_sql v1 does not support FETCH clauses"));
        }
        if !query.locks.is_empty() {
            return Err(
                self.unsupported_node(query, "typed_sql v1 does not support locking clauses")
            );
        }
        if query.for_clause.is_some() {
            return Err(self.unsupported_node(query, "typed_sql v1 does not support FOR clauses"));
        }
        if query.settings.is_some()
            || query.format_clause.is_some()
            || !query.pipe_operators.is_empty()
        {
            return Err(
                self.unsupported_node(query, "typed_sql v1 only supports standard SELECT queries")
            );
        }

        let SetExpr::Select(select) = query.body.as_ref() else {
            return Err(self.unsupported_node(query, "typed_sql v1 only supports SELECT queries"));
        };
        let select = select.as_ref();
        self.reject_select_features(select)?;

        let from_item = select
            .from
            .first()
            .ok_or_else(|| self.unsupported_node(select, "typed_sql v1 requires a FROM clause"))?;
        if select.from.len() != 1 {
            return Err(self.unsupported_node(
                select,
                "typed_sql v1 supports exactly one FROM relation with optional JOINs",
            ));
        }

        let from = self.normalize_from_root(from_item)?;
        let joins = from_item
            .joins
            .iter()
            .map(|join| self.normalize_join(join))
            .collect::<Result<Vec<_>>>()?;
        let projections = select
            .projection
            .iter()
            .map(|item| self.normalize_projection(item))
            .collect::<Result<Vec<_>>>()?;
        let filter = select
            .selection
            .as_ref()
            .map(|expr| self.normalize_expr(expr, ExprContext::Predicate))
            .transpose()?;
        let order_by = self.normalize_order_by(query.order_by.as_ref())?;
        let (limit, offset) = self.normalize_limit_clause(query.limit_clause.as_ref())?;
        self.reject_unconsumed_optional_groups()?;

        Ok(ParsedSelect {
            id: self.alloc_id(),
            span: self.source_span(query)?,
            projections,
            from,
            joins,
            filter,
            order_by,
            limit,
            offset,
        })
    }

    fn normalize_statement(&mut self, statement: &Statement) -> Result<ParsedStatement> {
        match statement {
            Statement::Query(query) => Ok(ParsedStatement::query(ParsedRowStatement::select(
                self.normalize_query(query)?,
            ))),
            Statement::Insert(insert) => self.normalize_insert_statement(insert),
            Statement::Update(update) => self.normalize_update_statement(update),
            Statement::Delete(delete) => self.normalize_delete_statement(delete),
            _ => Err(TypedSqlError::unsupported(
                "typed_sql v1 only supports SELECT, INSERT, UPDATE, and DELETE statements",
            )),
        }
    }

    fn normalize_insert_statement(&mut self, insert: &Insert) -> Result<ParsedStatement> {
        self.reject_insert_features(insert)?;

        let target = self.normalize_insert_target(insert)?;
        let columns = insert
            .columns
            .iter()
            .map(|ident| self.normalize_ident(ident))
            .collect::<Result<Vec<_>>>()?;
        let values = self.normalize_insert_values(insert)?;
        let returning = self.normalize_returning(&insert.returning)?;
        self.reject_unconsumed_optional_groups()?;

        let insert = ParsedInsert {
            id: self.alloc_id(),
            span: self.source_span(insert)?,
            target,
            columns,
            values,
            returning,
        };
        Ok(if insert.returning.is_empty() {
            ParsedStatement::command(
                StatementKind::Insert,
                ParsedCommandStatement::insert(insert),
            )
        } else {
            ParsedStatement::rows(StatementKind::Insert, ParsedRowStatement::insert(insert))
        })
    }

    fn reject_insert_features(&self, insert: &Insert) -> Result<()> {
        if insert.optimizer_hint.is_some()
            || insert.or.is_some()
            || insert.ignore
            || insert.overwrite
            || !insert.assignments.is_empty()
            || insert.partitioned.is_some()
            || !insert.after_columns.is_empty()
            || insert.has_table_keyword
            || insert.replace_into
            || insert.priority.is_some()
            || insert.insert_alias.is_some()
            || insert.settings.is_some()
            || insert.format_clause.is_some()
        {
            return Err(self.unsupported_node(
                insert,
                "typed_sql v1 only supports INSERT ... VALUES ... with optional explicit RETURNING columns",
            ));
        }

        if insert.on.is_some() {
            return Err(self.unsupported_node(
                insert,
                "typed_sql v1 does not support ON CONFLICT or other INSERT conflict clauses",
            ));
        }

        Ok(())
    }

    fn normalize_insert_target(&mut self, insert: &Insert) -> Result<ParsedFrom> {
        let TableObject::TableName(name) = &insert.table else {
            return Err(self.unsupported_node(
                insert,
                "typed_sql v1 only supports plain table names as INSERT targets",
            ));
        };
        let table_name = self.normalize_object_name(name)?;
        let binding_name = match &insert.table_alias {
            Some(alias) => self.normalize_ident(alias)?,
            None => self.normalize_ident(object_name_last_ident(name)?)?,
        };
        Ok(ParsedFrom {
            id: self.alloc_id(),
            span: self.source_span(insert)?,
            table_name,
            binding_name,
        })
    }

    fn normalize_insert_values(&mut self, insert: &Insert) -> Result<Vec<ParsedValuesRow>> {
        let Some(source) = &insert.source else {
            return Err(self.unsupported_node(
                insert,
                "typed_sql v1 only supports INSERT ... VALUES ... statements",
            ));
        };
        self.reject_values_query_features(source)?;
        let SetExpr::Values(values) = source.body.as_ref() else {
            return Err(self.unsupported_node(
                source.as_ref(),
                "typed_sql v1 does not support INSERT ... SELECT",
            ));
        };
        self.normalize_values(values)
    }

    fn reject_values_query_features(&self, query: &Query) -> Result<()> {
        if query.with.is_some() {
            return Err(self.unsupported_node(query, "typed_sql v1 does not support WITH clauses"));
        }
        if query.fetch.is_some() {
            return Err(self.unsupported_node(query, "typed_sql v1 does not support FETCH clauses"));
        }
        if !query.locks.is_empty() {
            return Err(
                self.unsupported_node(query, "typed_sql v1 does not support locking clauses")
            );
        }
        if query.for_clause.is_some() {
            return Err(self.unsupported_node(query, "typed_sql v1 does not support FOR clauses"));
        }
        if query.settings.is_some()
            || query.format_clause.is_some()
            || !query.pipe_operators.is_empty()
            || query.order_by.is_some()
            || query.limit_clause.is_some()
        {
            return Err(self.unsupported_node(
                query,
                "typed_sql v1 only supports plain INSERT ... VALUES rows",
            ));
        }
        Ok(())
    }

    fn normalize_values(&mut self, values: &Values) -> Result<Vec<ParsedValuesRow>> {
        values
            .rows
            .iter()
            .map(|row| {
                let row_values = row
                    .iter()
                    .map(|expr| self.normalize_expr(expr, ExprContext::Projection))
                    .collect::<Result<Vec<_>>>()?;
                let span = match (row.first(), row.last()) {
                    (Some(first), Some(last)) => {
                        let first = self.source_span(first)?;
                        let last = self.source_span(last)?;
                        SourceSpan::new(first.start, last.end)
                    }
                    _ => self.source_span(values)?,
                };
                Ok(ParsedValuesRow {
                    id: self.alloc_id(),
                    span,
                    values: row_values,
                })
            })
            .collect()
    }

    fn normalize_update_statement(&mut self, update: &Update) -> Result<ParsedStatement> {
        if update.optimizer_hint.is_some() || update.or.is_some() || update.limit.is_some() {
            return Err(self.unsupported_node(
                update,
                "typed_sql v1 only supports UPDATE ... SET ... WHERE ... with optional explicit RETURNING columns",
            ));
        }
        if update.from.is_some() {
            return Err(
                self.unsupported_node(update, "typed_sql v1 does not support UPDATE ... FROM")
            );
        }
        if !update.table.joins.is_empty() {
            return Err(self.unsupported_node(
                update,
                "typed_sql v1 does not support joined UPDATE targets",
            ));
        }

        let target = self.normalize_table_factor(
            &update.table.relation,
            self.source_span(&update.table.relation)?,
        )?;
        let assignments = update
            .assignments
            .iter()
            .map(|assignment| self.normalize_assignment(assignment))
            .collect::<Result<Vec<_>>>()?;
        let Some(selection) = update.selection.as_ref() else {
            return Err(self.unsupported_node(
                update,
                "typed_sql v1 requires UPDATE statements to include a WHERE predicate",
            ));
        };
        let filter = self.normalize_expr(selection, ExprContext::Predicate)?;
        let returning = self.normalize_returning(&update.returning)?;
        self.reject_unconsumed_optional_groups()?;

        let update = ParsedUpdate {
            id: self.alloc_id(),
            span: self.source_span(update)?,
            target,
            assignments,
            filter,
            returning,
        };
        Ok(if update.returning.is_empty() {
            ParsedStatement::command(
                StatementKind::Update,
                ParsedCommandStatement::update(update),
            )
        } else {
            ParsedStatement::rows(StatementKind::Update, ParsedRowStatement::update(update))
        })
    }

    fn normalize_delete_statement(&mut self, delete: &Delete) -> Result<ParsedStatement> {
        if delete.optimizer_hint.is_some()
            || !delete.tables.is_empty()
            || !delete.order_by.is_empty()
            || delete.limit.is_some()
        {
            return Err(self.unsupported_node(
                delete,
                "typed_sql v1 only supports DELETE ... WHERE ... with optional explicit RETURNING columns",
            ));
        }
        if delete.using.is_some() {
            return Err(
                self.unsupported_node(delete, "typed_sql v1 does not support DELETE ... USING")
            );
        }

        let target = self.normalize_delete_target(delete)?;
        let Some(selection) = delete.selection.as_ref() else {
            return Err(self.unsupported_node(
                delete,
                "typed_sql v1 requires DELETE statements to include a WHERE predicate",
            ));
        };
        let filter = self.normalize_expr(selection, ExprContext::Predicate)?;
        let returning = self.normalize_returning(&delete.returning)?;
        self.reject_unconsumed_optional_groups()?;

        let delete = ParsedDelete {
            id: self.alloc_id(),
            span: self.source_span(delete)?,
            target,
            filter,
            returning,
        };
        Ok(if delete.returning.is_empty() {
            ParsedStatement::command(
                StatementKind::Delete,
                ParsedCommandStatement::delete(delete),
            )
        } else {
            ParsedStatement::rows(StatementKind::Delete, ParsedRowStatement::delete(delete))
        })
    }

    fn normalize_delete_target(&mut self, delete: &Delete) -> Result<ParsedFrom> {
        let tables = match &delete.from {
            FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => tables,
        };
        if tables.len() != 1 {
            return Err(self.unsupported_node(
                delete,
                "typed_sql v1 supports DELETE from exactly one target table",
            ));
        }
        let table = &tables[0];
        if !table.joins.is_empty() {
            return Err(self.unsupported_node(
                delete,
                "typed_sql v1 does not support joined DELETE targets",
            ));
        }
        self.normalize_table_factor(&table.relation, self.source_span(&table.relation)?)
    }

    fn normalize_assignment(&mut self, assignment: &Assignment) -> Result<ParsedAssignment> {
        let target = match &assignment.target {
            AssignmentTarget::ColumnName(name) => self.normalize_assignment_target(name)?,
            AssignmentTarget::Tuple(_) => {
                return Err(self.unsupported_node(
                    assignment,
                    "typed_sql v1 does not support tuple assignments in UPDATE",
                ))
            }
        };
        Ok(ParsedAssignment {
            id: self.alloc_id(),
            span: self.source_span(assignment)?,
            target,
            value: self.normalize_expr(&assignment.value, ExprContext::Projection)?,
        })
    }

    fn normalize_assignment_target(&self, name: &ObjectName) -> Result<ParsedAssignmentTarget> {
        let span = self.source_span(name)?;
        let parts = name
            .0
            .iter()
            .map(|part| match part {
                ObjectNamePart::Identifier(ident) => self.normalize_ident(ident),
                ObjectNamePart::Function(_) => Err(self.unsupported_node(
                    name,
                    "typed_sql v1 does not support function-based assignment targets",
                )),
            })
            .collect::<Result<Vec<_>>>()?;
        match parts.as_slice() {
            [column] => Ok(ParsedAssignmentTarget {
                span,
                binding: None,
                column: column.clone(),
            }),
            [binding, column] => Ok(ParsedAssignmentTarget {
                span,
                binding: Some(binding.clone()),
                column: column.clone(),
            }),
            _ => Err(TypedSqlError::unsupported_at(
                "typed_sql v1 only supports single-column UPDATE assignments",
                span,
            )),
        }
    }

    fn normalize_returning(
        &mut self,
        returning: &Option<Vec<SelectItem>>,
    ) -> Result<Vec<ParsedProjection>> {
        returning
            .as_ref()
            .map(|items| {
                items
                    .iter()
                    .map(|item| self.normalize_returning_projection(item))
                    .collect()
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn normalize_returning_projection(&mut self, item: &SelectItem) -> Result<ParsedProjection> {
        match item {
            SelectItem::UnnamedExpr(expr) => {
                let output_name = self.infer_output_name(expr)?;
                Ok(ParsedProjection {
                    id: self.alloc_id(),
                    span: self.source_span(item)?,
                    expr: self.normalize_expr(expr, ExprContext::Projection)?,
                    output_name,
                })
            }
            SelectItem::ExprWithAlias { expr, alias } => Ok(ParsedProjection {
                id: self.alloc_id(),
                span: self.source_span(item)?,
                expr: self.normalize_expr(expr, ExprContext::Projection)?,
                output_name: OutputNameSyntax::Explicit(self.normalize_ident(alias)?),
            }),
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => Err(self
                .unsupported_node(
                    item,
                    "typed_sql v1 does not support wildcard RETURNING projections",
                )),
        }
    }

    fn reject_select_features(&self, select: &Select) -> Result<()> {
        if select.distinct.is_some() {
            return Err(self.unsupported_node(select, "typed_sql v1 does not support DISTINCT"));
        }
        if select.optimizer_hint.is_some()
            || select.select_modifiers.is_some()
            || select.top.is_some()
            || select.exclude.is_some()
            || select.into.is_some()
            || !select.lateral_views.is_empty()
            || select.prewhere.is_some()
            || !select.connect_by.is_empty()
            || !matches!(&select.group_by, sqlparser::ast::GroupByExpr::Expressions(exprs, modifiers) if exprs.is_empty() && modifiers.is_empty())
            || !select.cluster_by.is_empty()
            || !select.distribute_by.is_empty()
            || !select.sort_by.is_empty()
            || select.having.is_some()
            || !select.named_window.is_empty()
            || select.qualify.is_some()
            || select.value_table_mode.is_some()
            || !matches!(select.flavor, sqlparser::ast::SelectFlavor::Standard)
        {
            return Err(self.unsupported_node(
                select,
                "typed_sql v1 only supports SELECT/FROM/JOIN/WHERE/ORDER BY/LIMIT/OFFSET",
            ));
        }
        Ok(())
    }

    fn normalize_from_root(&mut self, table: &TableWithJoins) -> Result<ParsedFrom> {
        self.normalize_table_factor(&table.relation, self.source_span(&table.relation)?)
    }

    fn normalize_table_factor(
        &mut self,
        table: &TableFactor,
        span: SourceSpan,
    ) -> Result<ParsedFrom> {
        match table {
            TableFactor::Table {
                name,
                alias,
                args,
                with_hints,
                version,
                with_ordinality,
                partitions,
                json_path,
                sample,
                index_hints,
            } => {
                if args.is_some()
                    || !with_hints.is_empty()
                    || version.is_some()
                    || *with_ordinality
                    || !partitions.is_empty()
                    || json_path.is_some()
                    || sample.is_some()
                    || !index_hints.is_empty()
                {
                    return Err(TypedSqlError::unsupported_at(
                        "typed_sql v1 only supports plain table references in FROM/JOIN",
                        span,
                    ));
                }

                let table_name = self.normalize_object_name(name)?;
                let binding_name = self.binding_name(name, alias.as_ref())?;
                Ok(ParsedFrom {
                    id: self.alloc_id(),
                    span,
                    table_name,
                    binding_name,
                })
            }
            _ => Err(TypedSqlError::unsupported_at(
                "typed_sql v1 only supports plain table references in FROM/JOIN",
                span,
            )),
        }
    }

    fn binding_name(
        &self,
        table_name: &ObjectName,
        alias: Option<&TableAlias>,
    ) -> Result<BindingNameSyntax> {
        let ident = if let Some(alias) = alias {
            if !alias.columns.is_empty() {
                return Err(self
                    .unsupported_node(alias, "typed_sql v1 does not support column alias lists"));
            }
            alias.name.clone()
        } else {
            object_name_last_ident(table_name)?.clone()
        };
        self.normalize_ident(&ident)
    }

    fn normalize_join(&mut self, join: &Join) -> Result<ParsedJoin> {
        if join.global {
            return Err(self.unsupported_node(join, "typed_sql v1 does not support GLOBAL joins"));
        }

        let (kind, on) = match &join.join_operator {
            JoinOperator::Join(JoinConstraint::On(expr))
            | JoinOperator::Inner(JoinConstraint::On(expr)) => (JoinKind::Inner, expr),
            JoinOperator::Left(JoinConstraint::On(expr))
            | JoinOperator::LeftOuter(JoinConstraint::On(expr)) => (JoinKind::Left, expr),
            JoinOperator::Right(JoinConstraint::On(expr))
            | JoinOperator::RightOuter(JoinConstraint::On(expr)) => (JoinKind::Right, expr),
            JoinOperator::FullOuter(JoinConstraint::On(expr)) => (JoinKind::Full, expr),
            JoinOperator::Join(_)
            | JoinOperator::Inner(_)
            | JoinOperator::Left(_)
            | JoinOperator::LeftOuter(_)
            | JoinOperator::Right(_)
            | JoinOperator::RightOuter(_)
            | JoinOperator::FullOuter(_) => {
                return Err(
                    self.unsupported_node(join, "typed_sql v1 requires JOIN ... ON predicates")
                )
            }
            _ => {
                return Err(self.unsupported_node(
                    join,
                    "typed_sql v1 only supports INNER/LEFT/RIGHT/FULL JOIN ... ON",
                ))
            }
        };

        Ok(ParsedJoin {
            id: self.alloc_id(),
            span: self.source_span(join)?,
            kind,
            right: self
                .normalize_table_factor(&join.relation, self.source_span(&join.relation)?)?,
            on: self.normalize_expr(on, ExprContext::Predicate)?,
        })
    }

    fn normalize_projection(&mut self, item: &SelectItem) -> Result<ParsedProjection> {
        match item {
            SelectItem::UnnamedExpr(expr) => {
                let output_name = self.infer_output_name(expr)?;
                Ok(ParsedProjection {
                    id: self.alloc_id(),
                    span: self.source_span(item)?,
                    expr: self.normalize_expr(expr, ExprContext::Projection)?,
                    output_name,
                })
            }
            SelectItem::ExprWithAlias { expr, alias } => Ok(ParsedProjection {
                id: self.alloc_id(),
                span: self.source_span(item)?,
                expr: self.normalize_expr(expr, ExprContext::Projection)?,
                output_name: OutputNameSyntax::Explicit(self.normalize_ident(alias)?),
            }),
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
                Err(self
                    .unsupported_node(item, "typed_sql v1 does not support wildcard projections"))
            }
        }
    }

    fn infer_output_name(&self, expr: &Expr) -> Result<OutputNameSyntax> {
        match expr {
            Expr::CompoundIdentifier(parts) if parts.len() == 2 => Ok(OutputNameSyntax::Implicit(
                self.normalize_ident(parts.last().expect("parts len checked"))?,
            )),
            _ => Ok(OutputNameSyntax::Anonymous),
        }
    }

    fn normalize_order_by(&mut self, order_by: Option<&OrderBy>) -> Result<Vec<ParsedOrderBy>> {
        let Some(order_by) = order_by else {
            return Ok(Vec::new());
        };
        let OrderByKind::Expressions(items) = &order_by.kind else {
            return Err(
                self.unsupported_node(order_by, "typed_sql v1 does not support ORDER BY ALL")
            );
        };
        items
            .iter()
            .map(|item| self.normalize_order_by_expr(item))
            .collect()
    }

    fn normalize_order_by_expr(&mut self, item: &OrderByExpr) -> Result<ParsedOrderBy> {
        if item.with_fill.is_some() {
            return Err(
                self.unsupported_node(item, "typed_sql v1 does not support ORDER BY WITH FILL")
            );
        }
        Ok(ParsedOrderBy {
            id: self.alloc_id(),
            span: self.source_span(item)?,
            expr: self.normalize_expr(&item.expr, ExprContext::OrderBy)?,
            direction: if item.options.asc == Some(false) {
                OrderDirection::Desc
            } else {
                OrderDirection::Asc
            },
            nulls: item.options.nulls_first.map(|nulls_first| {
                if nulls_first {
                    NullsOrder::First
                } else {
                    NullsOrder::Last
                }
            }),
        })
    }

    fn normalize_limit_clause(
        &mut self,
        limit_clause: Option<&LimitClause>,
    ) -> Result<(Option<ParsedLimit>, Option<ParsedOffset>)> {
        let Some(limit_clause) = limit_clause else {
            return Ok((None, None));
        };

        match limit_clause {
            LimitClause::LimitOffset {
                limit,
                offset,
                limit_by,
            } => {
                if !limit_by.is_empty() {
                    return Err(self
                        .unsupported_node(limit_clause, "typed_sql v1 does not support LIMIT BY"));
                }

                let parsed_limit = limit
                    .as_ref()
                    .map(|expr| {
                        Ok(ParsedLimit {
                            id: self.alloc_id(),
                            span: self.source_span(expr)?,
                            expr: self.normalize_expr(expr, ExprContext::Limit)?,
                        })
                    })
                    .transpose()?;
                let parsed_offset = offset
                    .as_ref()
                    .map(|offset| {
                        Ok(ParsedOffset {
                            id: self.alloc_id(),
                            span: self.source_span(offset)?,
                            expr: self.normalize_expr(&offset.value, ExprContext::Offset)?,
                        })
                    })
                    .transpose()?;
                Ok((parsed_limit, parsed_offset))
            }
            LimitClause::OffsetCommaLimit { .. } => Err(self.unsupported_node(
                limit_clause,
                "typed_sql v1 only supports LIMIT ... OFFSET ... syntax",
            )),
        }
    }

    fn normalize_expr(&mut self, expr: &Expr, context: ExprContext) -> Result<ParsedExpr> {
        match expr {
            Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
                let binding = self.normalize_ident(&parts[0])?;
                let column = self.normalize_ident(&parts[1])?;
                Ok(ParsedExpr::Column(ColumnRefSyntax {
                    id: self.alloc_id(),
                    span: self.source_span(expr)?,
                    binding,
                    column,
                }))
            }
            Expr::Identifier(_) => Err(self.unsupported_node(
                expr,
                "typed_sql v1 requires fully qualified column references",
            )),
            Expr::Value(value) => self.normalize_value(value, context),
            Expr::UnaryOp { op, expr: inner } => {
                let op = match op {
                    UnaryOperator::Not => UnaryOp::Not,
                    UnaryOperator::Plus => UnaryOp::Plus,
                    UnaryOperator::Minus => UnaryOp::Minus,
                    _ => {
                        return Err(self.unsupported_node(
                            expr,
                            "typed_sql v1 only supports NOT, +, and - unary operators",
                        ))
                    }
                };
                Ok(ParsedExpr::Unary {
                    id: self.alloc_id(),
                    span: self.source_span(expr)?,
                    op,
                    expr: Box::new(self.normalize_expr(inner, context.nested_value_context())?),
                })
            }
            Expr::BinaryOp { left, op, right } => {
                self.normalize_binary_expr(expr, left, op, right, context)
            }
            Expr::IsNull(inner) => Ok(ParsedExpr::IsNull {
                id: self.alloc_id(),
                span: self.source_span(expr)?,
                negated: false,
                expr: Box::new(self.normalize_expr(inner, context.nested_value_context())?),
            }),
            Expr::IsNotNull(inner) => Ok(ParsedExpr::IsNull {
                id: self.alloc_id(),
                span: self.source_span(expr)?,
                negated: true,
                expr: Box::new(self.normalize_expr(inner, context.nested_value_context())?),
            }),
            Expr::Nested(inner) => self.normalize_nested_expr(expr, inner, context),
            _ => {
                Err(self
                    .unsupported_node(expr, "typed_sql v1 does not support this expression form"))
            }
        }
    }

    fn normalize_binary_expr(
        &mut self,
        expr: &Expr,
        left: &Expr,
        op: &BinaryOperator,
        right: &Expr,
        context: ExprContext,
    ) -> Result<ParsedExpr> {
        match op {
            BinaryOperator::And => {
                let mut terms = Vec::new();
                self.collect_bool_terms(left, BoolOp::And, &mut terms)?;
                self.collect_bool_terms(right, BoolOp::And, &mut terms)?;
                Ok(ParsedExpr::BoolChain {
                    id: self.alloc_id(),
                    span: self.source_span(expr)?,
                    op: BoolOp::And,
                    terms,
                })
            }
            BinaryOperator::Or => {
                let mut terms = Vec::new();
                self.collect_bool_terms(left, BoolOp::Or, &mut terms)?;
                self.collect_bool_terms(right, BoolOp::Or, &mut terms)?;
                Ok(ParsedExpr::BoolChain {
                    id: self.alloc_id(),
                    span: self.source_span(expr)?,
                    op: BoolOp::Or,
                    terms,
                })
            }
            _ => Ok(ParsedExpr::Binary {
                id: self.alloc_id(),
                span: self.source_span(expr)?,
                op: map_binary_op(op).ok_or_else(|| {
                    self.unsupported_node(
                        expr,
                        "typed_sql v1 only supports simple comparison operators",
                    )
                })?,
                left: Box::new(self.normalize_expr(left, context.comparison_operand_context())?),
                right: Box::new(self.normalize_expr(right, context.comparison_operand_context())?),
            }),
        }
    }

    fn collect_bool_terms(
        &mut self,
        expr: &Expr,
        op: BoolOp,
        terms: &mut Vec<ParsedExpr>,
    ) -> Result<()> {
        if let Expr::BinaryOp {
            left,
            op: inner_op,
            right,
        } = expr
        {
            let expected = match op {
                BoolOp::And => BinaryOperator::And,
                BoolOp::Or => BinaryOperator::Or,
            };
            if *inner_op == expected {
                self.collect_bool_terms(left, op, terms)?;
                self.collect_bool_terms(right, op, terms)?;
                return Ok(());
            }
        }
        terms.push(self.normalize_expr(expr, ExprContext::Predicate)?);
        Ok(())
    }

    fn normalize_value(
        &mut self,
        value: &ValueWithSpan,
        context: ExprContext,
    ) -> Result<ParsedExpr> {
        match &value.value {
            Value::Placeholder(token) => {
                let Some(entry) = self.source.placeholders.entry_for_token(token) else {
                    return Err(TypedSqlError::internal(format!(
                        "parser produced unknown placeholder token {token}"
                    )));
                };
                Ok(ParsedExpr::Placeholder(
                    self.normalize_placeholder(value, entry, context)?,
                ))
            }
            Value::Number(number, _) => Ok(ParsedExpr::Literal(ParsedLiteral {
                id: self.alloc_id(),
                span: self.source_span(value)?,
                value: Literal::Number(number.to_string()),
            })),
            Value::SingleQuotedString(text)
            | Value::DoubleQuotedString(text)
            | Value::TripleSingleQuotedString(text)
            | Value::TripleDoubleQuotedString(text)
            | Value::EscapedStringLiteral(text)
            | Value::NationalStringLiteral(text)
            | Value::UnicodeStringLiteral(text)
            | Value::SingleQuotedRawStringLiteral(text)
            | Value::DoubleQuotedRawStringLiteral(text)
            | Value::TripleSingleQuotedRawStringLiteral(text)
            | Value::TripleDoubleQuotedRawStringLiteral(text) => {
                Ok(ParsedExpr::Literal(ParsedLiteral {
                    id: self.alloc_id(),
                    span: self.source_span(value)?,
                    value: Literal::String(text.clone()),
                }))
            }
            Value::Boolean(value_bool) => Ok(ParsedExpr::Literal(ParsedLiteral {
                id: self.alloc_id(),
                span: self.source_span(value)?,
                value: Literal::Boolean(*value_bool),
            })),
            Value::Null => Ok(ParsedExpr::Literal(ParsedLiteral {
                id: self.alloc_id(),
                span: self.source_span(value)?,
                value: Literal::Null,
            })),
            _ => {
                Err(self.unsupported_node(value, "typed_sql v1 does not support this literal form"))
            }
        }
    }

    fn normalize_placeholder(
        &mut self,
        value: &ValueWithSpan,
        entry: &PlaceholderEntry,
        context: ExprContext,
    ) -> Result<PlaceholderRef> {
        let canonical_span = self.source.canonical_span_for_parser(value.span)?;
        let occurrence = entry
            .occurrences
            .iter()
            .find(|occurrence| occurrence.canonical_span == canonical_span)
            .ok_or_else(|| {
                TypedSqlError::internal(format!(
                    "parser span {canonical_span} did not match placeholder occurrence for `${}`",
                    entry.name
                ))
            })?;
        if occurrence.optional && !context.allows_optional_placeholder() {
            return Err(TypedSqlError::unsupported_at(
                match context {
                    ExprContext::NestedComparisonOperand => {
                        "typed_sql v1 requires `$value?` to directly own a WHERE/JOIN comparison predicate or the full LIMIT/OFFSET expression; wrap larger predicate fragments in `(...)?` when the ownership boundary would otherwise be ambiguous"
                    }
                    _ => {
                        "typed_sql v1 only supports `$value?` when it owns a WHERE/JOIN comparison predicate or the full LIMIT/OFFSET expression"
                    }
                },
                occurrence.original_span,
            ));
        }
        Ok(PlaceholderRef {
            id: self.alloc_id(),
            span: occurrence.original_span,
            placeholder_id: PlaceholderId(entry.id.0),
            name: entry.name.clone(),
            slot: entry.slot,
            optional: occurrence.optional,
        })
    }

    fn normalize_nested_expr(
        &mut self,
        expr: &Expr,
        inner: &Expr,
        context: ExprContext,
    ) -> Result<ParsedExpr> {
        let canonical_span = self.source.canonical_span_for_parser(expr.span())?;
        let inner_expr = self.normalize_expr(inner, context)?;
        let Some(group) = self
            .source
            .optional_groups
            .entry_for_canonical_span(canonical_span)
        else {
            return Ok(inner_expr);
        };
        self.consumed_optional_groups.insert(group.original_span);
        if !context.allows_optional_group() {
            return Err(TypedSqlError::unsupported_at(
                "typed_sql v1 requires `(...)?` to own an entire parenthesized WHERE/JOIN predicate or a single ORDER BY expression; LIMIT/OFFSET and other clause bodies must use direct `$value?` placeholders instead",
                group.original_span,
            ));
        }
        Ok(ParsedExpr::OptionalGroup(ParsedOptionalGroup {
            id: self.alloc_id(),
            span: group.original_span,
            expr: Box::new(inner_expr),
        }))
    }

    fn normalize_object_name(&self, name: &ObjectName) -> Result<ObjectNameSyntax> {
        let parts = name
            .0
            .iter()
            .map(|part| match part {
                ObjectNamePart::Identifier(ident) => self.normalize_ident(ident),
                ObjectNamePart::Function(_) => Err(self.unsupported_node(
                    name,
                    "typed_sql v1 does not support function-based relation names",
                )),
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(ObjectNameSyntax {
            span: self.source_span(name)?,
            parts,
        })
    }

    fn normalize_ident(&self, ident: &Ident) -> Result<IdentSyntax> {
        Ok(IdentSyntax {
            value: ident.value.clone(),
            span: self.source_span_from_ident(ident)?,
        })
    }

    fn source_span<T: Spanned>(&self, node: &T) -> Result<SourceSpan> {
        let canonical = self.source.canonical_span_for_parser(node.span())?;
        Ok(self.source.source_map.original_span(canonical))
    }

    fn source_span_from_ident(&self, ident: &Ident) -> Result<SourceSpan> {
        let canonical = self.source.canonical_span_for_parser(ident.span)?;
        Ok(self.source.source_map.original_span(canonical))
    }

    fn alloc_id(&mut self) -> AstId {
        let next = self.next_id;
        self.next_id += 1;
        AstId(next)
    }

    fn unsupported_node<T: Spanned>(&self, node: &T, message: impl Into<String>) -> TypedSqlError {
        let span = self.source_span(node).ok();
        TypedSqlError::unsupported_with_optional_span(message.into(), span)
    }

    fn reject_unconsumed_optional_groups(&self) -> Result<()> {
        let Some(group) = self
            .source
            .optional_groups
            .entries()
            .iter()
            .find(|group| !self.consumed_optional_groups.contains(&group.original_span))
        else {
            return Ok(());
        };
        Err(TypedSqlError::unsupported_at(
            "typed_sql v1 requires `(...)?` to own an entire parenthesized WHERE/JOIN predicate or a single ORDER BY expression; LIMIT/OFFSET and other clause bodies must use direct `$value?` placeholders instead",
            group.original_span,
        ))
    }
}

#[derive(Clone, Copy)]
enum ExprContext {
    Projection,
    Predicate,
    ComparisonOperand,
    NestedComparisonOperand,
    OrderBy,
    Limit,
    Offset,
}

impl ExprContext {
    fn allows_optional_placeholder(self) -> bool {
        matches!(self, Self::ComparisonOperand | Self::Limit | Self::Offset)
    }

    fn allows_optional_group(self) -> bool {
        matches!(self, Self::Predicate | Self::OrderBy)
    }

    fn comparison_operand_context(self) -> Self {
        match self {
            Self::Predicate | Self::ComparisonOperand | Self::NestedComparisonOperand => {
                Self::ComparisonOperand
            }
            other => other,
        }
    }

    fn nested_value_context(self) -> Self {
        match self {
            Self::ComparisonOperand | Self::NestedComparisonOperand => {
                Self::NestedComparisonOperand
            }
            other => other,
        }
    }
}

fn object_name_last_ident(name: &ObjectName) -> Result<&Ident> {
    let Some(part) = name.0.last() else {
        return Err(TypedSqlError::unsupported(
            "typed_sql v1 requires a named relation",
        ));
    };
    match part {
        ObjectNamePart::Identifier(ident) => Ok(ident),
        ObjectNamePart::Function(_) => Err(TypedSqlError::unsupported(
            "typed_sql v1 does not support function-based relation names",
        )),
    }
}

fn map_binary_op(op: &BinaryOperator) -> Option<BinaryOp> {
    match op {
        BinaryOperator::Eq => Some(BinaryOp::Eq),
        BinaryOperator::NotEq => Some(BinaryOp::NotEq),
        BinaryOperator::Lt => Some(BinaryOp::Lt),
        BinaryOperator::LtEq => Some(BinaryOp::LtEq),
        BinaryOperator::Gt => Some(BinaryOp::Gt),
        BinaryOperator::GtEq => Some(BinaryOp::GtEq),
        _ => None,
    }
}
