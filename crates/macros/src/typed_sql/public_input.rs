use proc_macro2::{Delimiter, Group, Span, TokenStream, TokenTree};

use super::{parse_select_source, source, ParsedSql, SourceSpan, SqlSource, TypedSqlError};

#[derive(Clone, Debug)]
pub(crate) struct PublicSqlInput {
    pub(crate) source: SqlSource,
    span_map: PublicSpanMap,
}

impl PublicSqlInput {
    pub(crate) fn parse(tokens: TokenStream) -> syn::Result<Self> {
        ParsedPublicSql::parse(tokens)?.into_input()
    }

    pub(crate) fn parse_select(&self) -> syn::Result<ParsedSql> {
        self.parse_with(parse_select_source)
    }

    pub(crate) fn parse_with<T>(
        &self,
        parse: impl FnOnce(SqlSource) -> super::Result<T>,
    ) -> syn::Result<T> {
        parse(self.source.clone()).map_err(|err| self.syn_error(err))
    }

    pub(crate) fn source(&self) -> &SqlSource {
        &self.source
    }

    pub(crate) fn syn_error(&self, err: TypedSqlError) -> syn::Error {
        syn::Error::new(
            self.span_map
                .span_for(err.span)
                .unwrap_or_else(Span::call_site),
            err.render_for_user(&self.source),
        )
    }

    pub(crate) fn syn_error_message(&self, message: impl std::fmt::Display) -> syn::Error {
        let span = self
            .span_map
            .segments
            .first()
            .map(|segment| segment.token_span)
            .unwrap_or_else(Span::call_site);
        syn::Error::new(span, message.to_string())
    }
}

struct ParsedPublicSql {
    sql: String,
    span_map: PublicSpanMap,
    canonicalize_error: CanonicalizeErrorTarget,
}

impl ParsedPublicSql {
    fn parse(tokens: TokenStream) -> syn::Result<Self> {
        if let Some((sql, span)) = parse_literal_sql(&tokens)? {
            return Ok(Self {
                span_map: PublicSpanMap {
                    segments: vec![SpanSegment {
                        sql_span: SourceSpan::new(0, to_u32(sql.len())?),
                        token_span: span,
                    }],
                },
                canonicalize_error: CanonicalizeErrorTarget::SingleSpan(span),
                sql,
            });
        }

        let mut builder = SqlTextBuilder::default();
        builder.push_stream(tokens)?;
        if builder.sql.trim().is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                "typed_sql v1 expects SQL tokens",
            ));
        }

        let span_map = PublicSpanMap {
            segments: builder.segments,
        };
        Ok(Self {
            sql: builder.sql,
            canonicalize_error: CanonicalizeErrorTarget::SpanMap(span_map.clone()),
            span_map,
        })
    }

    fn into_input(self) -> syn::Result<PublicSqlInput> {
        let canonicalized = source::canonicalize_parts(&self.sql)
            .map_err(|err| self.canonicalize_error.into_syn_error(err))?;
        Ok(PublicSqlInput {
            source: SqlSource::from_canonicalized(self.sql, canonicalized),
            span_map: self.span_map,
        })
    }
}

enum CanonicalizeErrorTarget {
    SingleSpan(Span),
    SpanMap(PublicSpanMap),
}

impl CanonicalizeErrorTarget {
    fn into_syn_error(self, err: TypedSqlError) -> syn::Error {
        match self {
            Self::SingleSpan(span) => syn::Error::new(span, err),
            Self::SpanMap(span_map) => syn::Error::new(
                span_map.span_for(err.span).unwrap_or_else(Span::call_site),
                err.to_string(),
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SqlTextBuilder {
    sql: String,
    segments: Vec<SpanSegment>,
    previous_kind: Option<PieceKind>,
}

impl SqlTextBuilder {
    fn push_stream(&mut self, stream: TokenStream) -> syn::Result<()> {
        let mut tokens = stream.into_iter().peekable();
        while let Some(token) = tokens.next() {
            match token {
                TokenTree::Group(group) => {
                    let optional_suffix = if matches!(tokens.peek(), Some(TokenTree::Punct(punct)) if punct.as_char() == '?')
                    {
                        let Some(TokenTree::Punct(punct)) = tokens.next() else {
                            unreachable!("peeked optional suffix");
                        };
                        Some(punct.span())
                    } else {
                        None
                    };
                    self.push_group(group, optional_suffix)?
                }
                TokenTree::Ident(ident) => {
                    self.push_piece(&ident.to_string(), ident.span(), PieceKind::Word)?
                }
                TokenTree::Literal(literal) => self.push_literal(literal)?,
                TokenTree::Punct(punct) => self.push_punct(punct, &mut tokens)?,
            }
        }
        Ok(())
    }

    fn push_group(&mut self, group: Group, optional_suffix: Option<Span>) -> syn::Result<()> {
        match group.delimiter() {
            Delimiter::Parenthesis => {
                self.push_piece("(", group.span_open(), PieceKind::OpenDelim)?;
                self.push_stream(group.stream())?;
                self.push_piece(")", group.span_close(), PieceKind::CloseDelim)?;
                if let Some(optional_span) = optional_suffix {
                    self.push_piece("?", optional_span, PieceKind::SuffixMarker)?;
                }
                Ok(())
            }
            Delimiter::None => self.push_stream(group.stream()),
            Delimiter::Brace | Delimiter::Bracket => Err(syn::Error::new(
                group.span(),
                "typed_sql v1 token input only supports parentheses groups",
            )),
        }
    }

    fn push_literal(&mut self, literal: proc_macro2::Literal) -> syn::Result<()> {
        let token = TokenStream::from(TokenTree::Literal(literal.clone()));
        let literal_text = literal.to_string();
        let literal_span = literal.span();
        let literal = syn::parse2::<syn::Lit>(token)?;
        match literal {
            syn::Lit::Str(value) => self.push_piece(
                &render_sql_string(&value.value()),
                literal_span,
                PieceKind::Word,
            ),
            syn::Lit::Char(value) => self.push_piece(
                &render_sql_string(&value.value().to_string()),
                literal_span,
                PieceKind::Word,
            ),
            syn::Lit::Int(value) => {
                validate_numeric_literal(&literal_text, value.suffix(), literal_span)?;
                self.push_piece(&literal_text, literal_span, PieceKind::Word)
            }
            syn::Lit::Float(value) => {
                validate_numeric_literal(&literal_text, value.suffix(), literal_span)?;
                self.push_piece(&literal_text, literal_span, PieceKind::Word)
            }
            _unsupported => Err(syn::Error::new(
                literal_span,
                format!(
                    "typed_sql v1 token input does not support Rust literal `{}` here",
                    literal_text
                ),
            )),
        }
    }

    fn push_punct(
        &mut self,
        punct: proc_macro2::Punct,
        tokens: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ) -> syn::Result<()> {
        let ch = punct.as_char();
        if ch == '$' {
            let Some(next) = tokens.next() else {
                return Err(syn::Error::new(
                    punct.span(),
                    "typed_sql v1 placeholders must use a name like `$id`",
                ));
            };
            let TokenTree::Ident(ident) = next else {
                return Err(syn::Error::new(
                    span_for_tree(&next),
                    "typed_sql v1 placeholders must use a name like `$id`",
                ));
            };
            let span = punct
                .span()
                .join(ident.span())
                .unwrap_or_else(|| punct.span());
            let optional =
                matches!(tokens.peek(), Some(TokenTree::Punct(punct)) if punct.as_char() == '?');
            let span = if optional {
                let Some(TokenTree::Punct(optional_punct)) = tokens.next() else {
                    unreachable!("peeked optional suffix");
                };
                span.join(optional_punct.span()).unwrap_or(span)
            } else {
                span
            };
            return self.push_piece(
                &format!("${ident}{}", if optional { "?" } else { "" }),
                span,
                PieceKind::Word,
            );
        }

        if let Some((text, span, kind)) = try_compose_punct(ch, punct.span(), tokens) {
            return self.push_piece(&text, span, kind);
        }

        self.push_piece(&ch.to_string(), punct.span(), punct_kind(ch))
    }

    fn push_piece(&mut self, text: &str, span: Span, kind: PieceKind) -> syn::Result<()> {
        if needs_space(self.previous_kind, kind) {
            self.sql.push(' ');
        }
        let start = self.sql.len();
        self.sql.push_str(text);
        let end = self.sql.len();
        self.segments.push(SpanSegment {
            sql_span: SourceSpan::new(to_u32(start)?, to_u32(end)?),
            token_span: span,
        });
        self.previous_kind = Some(kind);
        Ok(())
    }

    fn syn_error(&self, err: TypedSqlError) -> syn::Error {
        syn::Error::new(
            self.segments
                .span_for(err.span)
                .unwrap_or_else(Span::call_site),
            err.to_string(),
        )
    }
}

#[derive(Clone, Debug)]
struct PublicSpanMap {
    segments: Vec<SpanSegment>,
}

impl PublicSpanMap {
    fn span_for(&self, span: Option<SourceSpan>) -> Option<Span> {
        self.segments.span_for(span)
    }
}

trait SpanSegments {
    fn span_for(&self, span: Option<SourceSpan>) -> Option<Span>;
}

impl SpanSegments for [SpanSegment] {
    fn span_for(&self, span: Option<SourceSpan>) -> Option<Span> {
        let span = span?;
        let start = span.start;
        let end = span.end.max(span.start.saturating_add(1));

        let first = self
            .iter()
            .position(|segment| segment.sql_span.end > start && segment.sql_span.start < end)?;
        let last = self
            .iter()
            .rposition(|segment| segment.sql_span.end > start && segment.sql_span.start < end)
            .unwrap_or(first);

        let start_span = self[first].token_span;
        let end_span = self[last].token_span;
        Some(start_span.join(end_span).unwrap_or(start_span))
    }
}

#[derive(Clone, Debug)]
struct SpanSegment {
    sql_span: SourceSpan,
    token_span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PieceKind {
    Word,
    OpenDelim,
    CloseDelim,
    SuffixMarker,
    Dot,
    Comma,
    TightOperator,
    Operator,
    Semicolon,
}

fn parse_literal_sql(tokens: &TokenStream) -> syn::Result<Option<(String, Span)>> {
    let mut iter = tokens.clone().into_iter();
    let Some(token) = iter.next() else {
        return Ok(None);
    };
    if iter.next().is_some() {
        return Ok(None);
    }

    let TokenTree::Literal(literal) = token else {
        return Ok(None);
    };
    let literal_span = literal.span();
    let literal = syn::parse2::<syn::Lit>(TokenStream::from(TokenTree::Literal(literal)))?;
    match literal {
        syn::Lit::Str(value) => Ok(Some((value.value(), literal_span))),
        _ => Ok(None),
    }
}

fn render_sql_string(value: &str) -> String {
    let mut rendered = String::with_capacity(value.len() + 2);
    rendered.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            rendered.push('\'');
        }
        rendered.push(ch);
    }
    rendered.push('\'');
    rendered
}

fn validate_numeric_literal(text: &str, suffix: &str, span: Span) -> syn::Result<()> {
    if !suffix.is_empty() {
        return Err(syn::Error::new(
            span,
            "typed_sql v1 token input only supports unsuffixed numeric literals",
        ));
    }
    if text.contains('_') {
        return Err(syn::Error::new(
            span,
            "typed_sql v1 token input does not support `_` separators in numeric literals",
        ));
    }
    Ok(())
}

fn try_compose_punct(
    current: char,
    span: Span,
    tokens: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
) -> Option<(String, Span, PieceKind)> {
    let next = tokens.peek()?;
    let TokenTree::Punct(next_punct) = next else {
        return None;
    };
    let composite = match (current, next_punct.as_char()) {
        ('>', '=') => ">=",
        ('<', '=') => "<=",
        ('!', '=') => "!=",
        ('<', '>') => "<>",
        (':', ':') => "::",
        _ => return None,
    };
    let next_span = next_punct.span();
    let span = span.join(next_span).unwrap_or(span);
    tokens.next();
    Some((composite.to_owned(), span, PieceKind::TightOperator))
}

fn punct_kind(ch: char) -> PieceKind {
    match ch {
        '.' => PieceKind::Dot,
        ',' => PieceKind::Comma,
        ';' => PieceKind::Semicolon,
        '=' | '<' | '>' | '+' | '-' | '*' | '/' => PieceKind::Operator,
        _ => PieceKind::Operator,
    }
}

fn needs_space(previous: Option<PieceKind>, next: PieceKind) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    if matches!(
        next,
        PieceKind::CloseDelim | PieceKind::Comma | PieceKind::Dot | PieceKind::SuffixMarker
    ) || matches!(previous, PieceKind::OpenDelim | PieceKind::Dot)
    {
        return false;
    }
    if matches!(next, PieceKind::TightOperator) || matches!(previous, PieceKind::TightOperator) {
        return false;
    }
    true
}

fn span_for_tree(tree: &TokenTree) -> Span {
    match tree {
        TokenTree::Group(group) => group.span(),
        TokenTree::Ident(ident) => ident.span(),
        TokenTree::Punct(punct) => punct.span(),
        TokenTree::Literal(literal) => literal.span(),
    }
}

fn to_u32(value: usize) -> syn::Result<u32> {
    u32::try_from(value).map_err(|_| syn::Error::new(Span::call_site(), "SQL input is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn accepts_token_style_sql_and_named_placeholders() {
        let input = PublicSqlInput::parse(quote! {
            SELECT users.id, users.name
            FROM users
            WHERE users.id = $id AND users.name = "alice"
            ORDER BY users.id
        })
        .expect("token SQL parses");

        assert_eq!(
            input.source.original_sql,
            "SELECT users.id, users.name FROM users WHERE users.id = $id AND users.name = 'alice' ORDER BY users.id"
        );
        assert_eq!(
            input.source.canonical_sql,
            "SELECT users.id, users.name FROM users WHERE users.id = $1 AND users.name = 'alice' ORDER BY users.id"
        );
        let parsed = input.parse_select().expect("typed SQL parses");
        assert_eq!(parsed.source.placeholders.entries().len(), 1);
    }

    #[test]
    fn accepts_single_literal_sql_without_old_macro_parser() {
        let input = PublicSqlInput::parse(quote! {
            "SELECT users.id FROM users WHERE users.id = $id"
        })
        .expect("literal SQL parses");

        assert_eq!(
            input.source.canonical_sql,
            "SELECT users.id FROM users WHERE users.id = $1"
        );
    }

    #[test]
    fn preserves_optional_suffix_syntax_in_token_sql() {
        let input = PublicSqlInput::parse(quote! {
            SELECT users.id
            FROM users
            WHERE (users.id = $id?)?
            LIMIT $limit?
        })
        .expect("token SQL parses");

        assert_eq!(
            input.source.original_sql,
            "SELECT users.id FROM users WHERE (users.id = $id?)? LIMIT $limit?"
        );
        assert_eq!(
            input.source.canonical_sql,
            "SELECT users.id FROM users WHERE (users.id = $1) LIMIT $2"
        );
    }

    #[test]
    fn rejects_non_parenthesis_groups() {
        let err = PublicSqlInput::parse(quote! {
            SELECT users.id FROM users WHERE users.id IN [1, 2, 3]
        })
        .expect_err("brackets rejected");

        assert!(err
            .to_string()
            .contains("typed_sql v1 token input only supports parentheses groups"));
    }

    #[test]
    fn rejects_suffixed_numeric_literals() {
        let err = PublicSqlInput::parse(quote! {
            SELECT users.id FROM users WHERE users.id = 1_i64
        })
        .expect_err("suffixed number rejected");

        assert!(err
            .to_string()
            .contains("only supports unsuffixed numeric literals"));
    }
}
