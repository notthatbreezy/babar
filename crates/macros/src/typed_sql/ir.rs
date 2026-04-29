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
pub(crate) enum ParsedExpr {
    Column(ColumnRefSyntax),
    Placeholder(PlaceholderRef),
    Literal(ParsedLiteral),
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
