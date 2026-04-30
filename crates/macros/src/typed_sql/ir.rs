use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct AstId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct PlaceholderId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct SourceSpan {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

impl SourceSpan {
    pub(crate) const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IdentSyntax {
    pub(crate) value: String,
    pub(crate) span: SourceSpan,
}

pub(crate) type BindingNameSyntax = IdentSyntax;
pub(crate) type ColumnNameSyntax = IdentSyntax;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObjectNameSyntax {
    pub(crate) parts: Vec<IdentSyntax>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OutputNameSyntax {
    Explicit(IdentSyntax),
    Implicit(IdentSyntax),
    Anonymous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum StatementKind {
    Query,
    Insert,
    Update,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum StatementResultKind {
    Rows,
    Command,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum RowStatementKind {
    Select,
    Insert,
    Update,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum CommandStatementKind {
    Insert,
    Update,
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedStatement {
    pub(crate) kind: StatementKind,
    pub(crate) result: StatementResultKind,
    pub(crate) body: ParsedStatementBody,
}

impl ParsedStatement {
    pub(crate) fn query(rows: ParsedRowStatement) -> Self {
        Self {
            kind: StatementKind::Query,
            result: StatementResultKind::Rows,
            body: ParsedStatementBody::Rows(rows),
        }
    }

    pub(crate) fn rows(kind: StatementKind, rows: ParsedRowStatement) -> Self {
        Self {
            kind,
            result: StatementResultKind::Rows,
            body: ParsedStatementBody::Rows(rows),
        }
    }

    pub(crate) fn command(kind: StatementKind, command: ParsedCommandStatement) -> Self {
        Self {
            kind,
            result: StatementResultKind::Command,
            body: ParsedStatementBody::Command(command),
        }
    }

    pub(crate) fn row_statement(&self) -> Option<&ParsedRowStatement> {
        match &self.body {
            ParsedStatementBody::Rows(rows) => Some(rows),
            ParsedStatementBody::Command(_) => None,
        }
    }

    pub(crate) fn command_statement(&self) -> Option<&ParsedCommandStatement> {
        match &self.body {
            ParsedStatementBody::Rows(_) => None,
            ParsedStatementBody::Command(command) => Some(command),
        }
    }

    pub(crate) fn as_select(&self) -> Option<&ParsedSelect> {
        self.row_statement().and_then(ParsedRowStatement::as_select)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParsedStatementBody {
    Rows(ParsedRowStatement),
    Command(ParsedCommandStatement),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedRowStatement {
    pub(crate) kind: RowStatementKind,
    pub(crate) shape: ParsedRowShape,
    pub(crate) body: ParsedRowStatementBody,
}

impl ParsedRowStatement {
    pub(crate) fn select(select: ParsedSelect) -> Self {
        let shape = ParsedRowShape::from_projections(&select.projections);
        Self {
            kind: RowStatementKind::Select,
            shape,
            body: ParsedRowStatementBody::Select(select),
        }
    }

    pub(crate) fn insert(insert: ParsedInsert) -> Self {
        let shape = ParsedRowShape::from_projections(&insert.returning);
        Self {
            kind: RowStatementKind::Insert,
            shape,
            body: ParsedRowStatementBody::Insert(insert),
        }
    }

    pub(crate) fn update(update: ParsedUpdate) -> Self {
        let shape = ParsedRowShape::from_projections(&update.returning);
        Self {
            kind: RowStatementKind::Update,
            shape,
            body: ParsedRowStatementBody::Update(update),
        }
    }

    pub(crate) fn delete(delete: ParsedDelete) -> Self {
        let shape = ParsedRowShape::from_projections(&delete.returning);
        Self {
            kind: RowStatementKind::Delete,
            shape,
            body: ParsedRowStatementBody::Delete(delete),
        }
    }

    pub(crate) fn as_select(&self) -> Option<&ParsedSelect> {
        match &self.body {
            ParsedRowStatementBody::Select(select) => Some(select),
            _ => None,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParsedRowStatementBody {
    Select(ParsedSelect),
    Insert(ParsedInsert),
    Update(ParsedUpdate),
    Delete(ParsedDelete),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedCommandStatement {
    pub(crate) kind: CommandStatementKind,
    pub(crate) body: ParsedCommandStatementBody,
}

impl ParsedCommandStatement {
    pub(crate) fn insert(insert: ParsedInsert) -> Self {
        Self {
            kind: CommandStatementKind::Insert,
            body: ParsedCommandStatementBody::Insert(insert),
        }
    }

    pub(crate) fn update(update: ParsedUpdate) -> Self {
        Self {
            kind: CommandStatementKind::Update,
            body: ParsedCommandStatementBody::Update(update),
        }
    }

    pub(crate) fn delete(delete: ParsedDelete) -> Self {
        Self {
            kind: CommandStatementKind::Delete,
            body: ParsedCommandStatementBody::Delete(delete),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParsedCommandStatementBody {
    Insert(ParsedInsert),
    Update(ParsedUpdate),
    Delete(ParsedDelete),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedRowShape {
    pub(crate) columns: Vec<ParsedRowColumn>,
}

impl ParsedRowShape {
    fn from_projections(projections: &[ParsedProjection]) -> Self {
        Self {
            columns: projections
                .iter()
                .map(ParsedRowColumn::from_projection)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedRowColumn {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) output_name: OutputNameSyntax,
}

impl ParsedRowColumn {
    fn from_projection(projection: &ParsedProjection) -> Self {
        Self {
            id: projection.id,
            span: projection.span,
            output_name: projection.output_name.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedSelect {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) projections: Vec<ParsedProjection>,
    pub(crate) from: ParsedFrom,
    pub(crate) joins: Vec<ParsedJoin>,
    pub(crate) filter: Option<ParsedExpr>,
    pub(crate) order_by: Vec<ParsedOrderBy>,
    pub(crate) limit: Option<ParsedLimit>,
    pub(crate) offset: Option<ParsedOffset>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedInsert {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) target: ParsedFrom,
    pub(crate) columns: Vec<ColumnNameSyntax>,
    pub(crate) values: Vec<ParsedValuesRow>,
    pub(crate) returning: Vec<ParsedProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedValuesRow {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) values: Vec<ParsedExpr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedUpdate {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) target: ParsedFrom,
    pub(crate) assignments: Vec<ParsedAssignment>,
    pub(crate) filter: ParsedExpr,
    pub(crate) returning: Vec<ParsedProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedDelete {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) target: ParsedFrom,
    pub(crate) filter: ParsedExpr,
    pub(crate) returning: Vec<ParsedProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedAssignment {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) target: ParsedAssignmentTarget,
    pub(crate) value: ParsedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedAssignmentTarget {
    pub(crate) span: SourceSpan,
    pub(crate) binding: Option<BindingNameSyntax>,
    pub(crate) column: ColumnNameSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedFrom {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) table_name: ObjectNameSyntax,
    pub(crate) binding_name: BindingNameSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedJoin {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) kind: JoinKind,
    pub(crate) right: ParsedFrom,
    pub(crate) on: ParsedExpr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedProjection {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) expr: ParsedExpr,
    pub(crate) output_name: OutputNameSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedOrderBy {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) expr: ParsedExpr,
    pub(crate) direction: OrderDirection,
    pub(crate) nulls: Option<NullsOrder>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NullsOrder {
    First,
    Last,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedLimit {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) expr: ParsedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedOffset {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) expr: ParsedExpr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedOptionalGroup {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) expr: Box<ParsedExpr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParsedExpr {
    Column(ColumnRefSyntax),
    Placeholder(PlaceholderRef),
    Literal(ParsedLiteral),
    OptionalGroup(ParsedOptionalGroup),
    Unary {
        id: AstId,
        span: SourceSpan,
        op: UnaryOp,
        expr: Box<ParsedExpr>,
    },
    Binary {
        id: AstId,
        span: SourceSpan,
        op: BinaryOp,
        left: Box<ParsedExpr>,
        right: Box<ParsedExpr>,
    },
    IsNull {
        id: AstId,
        span: SourceSpan,
        negated: bool,
        expr: Box<ParsedExpr>,
    },
    BoolChain {
        id: AstId,
        span: SourceSpan,
        op: BoolOp,
        terms: Vec<ParsedExpr>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ColumnRefSyntax {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) binding: BindingNameSyntax,
    pub(crate) column: ColumnNameSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceholderRef {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) placeholder_id: PlaceholderId,
    pub(crate) name: String,
    pub(crate) slot: u32,
    pub(crate) optional: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedLiteral {
    pub(crate) id: AstId,
    pub(crate) span: SourceSpan,
    pub(crate) value: Literal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Literal {
    Number(String),
    String(String),
    Boolean(bool),
    Null,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UnaryOp {
    Not,
    Plus,
    Minus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BinaryOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoolOp {
    And,
    Or,
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}
