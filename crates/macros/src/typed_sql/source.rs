use std::collections::HashMap;

use sqlparser::{
    dialect::PostgreSqlDialect,
    tokenizer::{Token, Tokenizer},
};

use crate::typed_sql::ir::{PlaceholderId, SourceSpan};

use super::{Result, TypedSqlError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SqlSource {
    pub(crate) original_sql: String,
    pub(crate) canonical_sql: String,
    pub(crate) source_map: SourceMap,
    pub(crate) placeholders: PlaceholderTable,
    line_starts: Vec<usize>,
}

impl SqlSource {
    pub(crate) fn canonical_span_for_parser(
        &self,
        span: sqlparser::tokenizer::Span,
    ) -> Result<SourceSpan> {
        if span.start.line == 0 || span.end.line == 0 {
            return Ok(SourceSpan::new(0, 0));
        }

        let start = self.byte_offset_for_location(span.start.line, span.start.column, false)?;
        let end = self.byte_offset_for_location(span.end.line, span.end.column, true)?;
        Ok(SourceSpan::new(to_u32(start)?, to_u32(end)?))
    }

    fn byte_offset_for_location(
        &self,
        line: u64,
        column: u64,
        _end_exclusive: bool,
    ) -> Result<usize> {
        let line_index = usize::try_from(line.saturating_sub(1)).map_err(|_| {
            TypedSqlError::internal(format!("line index {line} does not fit in usize"))
        })?;
        let Some(&line_start) = self.line_starts.get(line_index) else {
            return Err(TypedSqlError::internal(format!(
                "line {line} is outside canonical SQL"
            )));
        };
        let line_end = self
            .line_starts
            .get(line_index + 1)
            .copied()
            .map(|next| next - 1)
            .unwrap_or(self.canonical_sql.len());
        let line_text = &self.canonical_sql[line_start..line_end];
        let char_index = usize::try_from(column.saturating_sub(1)).map_err(|_| {
            TypedSqlError::internal(format!("column index {column} does not fit in usize"))
        })?;
        let target = char_index;
        let offset_in_line = nth_char_boundary(line_text, target).ok_or_else(|| {
            TypedSqlError::internal(format!(
                "column {column} is outside canonical SQL line {line}"
            ))
        })?;
        Ok(line_start + offset_in_line)
    }

    pub(crate) fn anchor_parser_error_span(
        &self,
        line: u64,
        column: u64,
        expected_token: Option<&str>,
    ) -> Result<Option<SourceSpan>> {
        if line == 0 || column == 0 {
            return Ok(None);
        }

        let offset = self.byte_offset_for_location(line, column, false)?;
        let Some(canonical_span) = self.anchor_canonical_span(offset, expected_token)? else {
            return Ok(Some(
                self.source_map
                    .original_span(SourceSpan::new(to_u32(offset)?, to_u32(offset)?)),
            ));
        };
        Ok(Some(self.source_map.original_span(canonical_span)))
    }

    pub(crate) fn render_span_excerpt(&self, span: SourceSpan) -> Option<String> {
        let start = usize::try_from(span.start).ok()?;
        let end = usize::try_from(span.end).ok()?;
        if start > self.original_sql.len() || end > self.original_sql.len() || start > end {
            return None;
        }

        let line_start = self.original_sql[..start]
            .rfind('\n')
            .map_or(0, |idx| idx + 1);
        let line_end = self.original_sql[end..]
            .find('\n')
            .map_or(self.original_sql.len(), |idx| end + idx);
        let line_text = &self.original_sql[line_start..line_end];
        let line_number = self.original_sql[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let column_start = self.original_sql[line_start..start].chars().count();
        let underline_width = if start == end {
            1
        } else {
            self.original_sql[start..end].chars().count().max(1)
        };

        Some(format!(
            "   |\n{line_number:>2} | {line_text}\n   | {}{}",
            " ".repeat(column_start),
            "^".repeat(underline_width),
        ))
    }

    pub(crate) fn clause_keyword_after_from(&self) -> Result<Option<(String, SourceSpan)>> {
        let dialect = PostgreSqlDialect {};
        let tokens = Tokenizer::new(&dialect, &self.canonical_sql)
            .tokenize_with_location()
            .map_err(|error| {
                TypedSqlError::internal(format!(
                    "failed to tokenize canonical SQL while building diagnostics: {error}"
                ))
            })?;

        let mut saw_from = false;
        for token in tokens {
            if is_trivia(&token.token) {
                continue;
            }
            let rendered = token.token.to_string();
            if saw_from {
                if is_clause_keyword(&rendered) {
                    let span = self.canonical_span_for_parser(token.span)?;
                    return Ok(Some((rendered, self.source_map.original_span(span))));
                }
                saw_from = false;
            } else if rendered == "FROM" {
                saw_from = true;
            }
        }
        Ok(None)
    }

    fn anchor_canonical_span(
        &self,
        offset: usize,
        expected_token: Option<&str>,
    ) -> Result<Option<SourceSpan>> {
        let dialect = PostgreSqlDialect {};
        let tokens = Tokenizer::new(&dialect, &self.canonical_sql)
            .tokenize_with_location()
            .map_err(|error| {
                TypedSqlError::internal(format!(
                    "failed to tokenize canonical SQL while building diagnostics: {error}"
                ))
            })?;

        let mut previous = None;
        let mut previous_matching = None;
        for token in tokens {
            if is_trivia(&token.token) {
                continue;
            }
            let span = self.canonical_span_for_parser(token.span)?;
            let start = usize::try_from(span.start).map_err(|_| {
                TypedSqlError::internal(format!("span start {} does not fit in usize", span.start))
            })?;
            let end = usize::try_from(span.end).map_err(|_| {
                TypedSqlError::internal(format!("span end {} does not fit in usize", span.end))
            })?;
            if let Some(expected_token) = expected_token {
                if token.token.to_string() == expected_token {
                    if offset <= start || offset < end {
                        return Ok(Some(span));
                    }
                    previous_matching = Some(span);
                    continue;
                }
            }
            if offset <= start || offset < end {
                return Ok(Some(span));
            }
            previous = Some(span);
        }

        Ok(previous_matching.or(previous))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceholderTable {
    entries: Vec<PlaceholderEntry>,
}

impl PlaceholderTable {
    pub(crate) fn entries(&self) -> &[PlaceholderEntry] {
        &self.entries
    }

    pub(crate) fn entry_for_token(&self, token: &str) -> Option<&PlaceholderEntry> {
        let slot = token.strip_prefix('$')?.parse::<u32>().ok()?;
        self.entries.iter().find(|entry| entry.slot == slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceholderEntry {
    pub(crate) id: PlaceholderId,
    pub(crate) name: String,
    pub(crate) slot: u32,
    pub(crate) occurrences: Vec<PlaceholderOccurrence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceholderOccurrence {
    pub(crate) original_span: SourceSpan,
    pub(crate) canonical_span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceMap {
    replacements: Vec<ReplacementSpan>,
}

impl SourceMap {
    pub(crate) fn original_span(&self, canonical_span: SourceSpan) -> SourceSpan {
        SourceSpan::new(
            self.original_offset(canonical_span.start, OffsetAnchor::Start),
            self.original_offset(canonical_span.end, OffsetAnchor::End),
        )
    }

    fn original_offset(&self, canonical_offset: u32, anchor: OffsetAnchor) -> u32 {
        let mut delta = 0i64;
        for replacement in &self.replacements {
            if canonical_offset < replacement.canonical.start {
                break;
            }
            if canonical_offset <= replacement.canonical.end {
                return match anchor {
                    OffsetAnchor::Start => replacement.original.start,
                    OffsetAnchor::End => replacement.original.end,
                };
            }
            delta += i64::from(replacement.original.end - replacement.original.start)
                - i64::from(replacement.canonical.end - replacement.canonical.start);
        }
        (i64::from(canonical_offset) + delta) as u32
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReplacementSpan {
    canonical: SourceSpan,
    original: SourceSpan,
}

#[derive(Clone, Copy)]
enum OffsetAnchor {
    Start,
    End,
}

pub(crate) fn canonicalize(sql: &str) -> Result<SqlSource> {
    let mut canonical_sql = String::with_capacity(sql.len());
    let mut replacements = Vec::new();
    let mut placeholder_slots = HashMap::<String, usize>::new();
    let mut placeholders = Vec::<PlaceholderEntry>::new();

    let chars: Vec<(usize, char)> = sql.char_indices().collect();
    let mut i = 0;
    let mut state = ScanState::Normal;
    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        match state {
            ScanState::Normal => {
                if ch == '\'' {
                    canonical_sql.push(ch);
                    state = ScanState::SingleQuoted;
                    i += 1;
                    continue;
                }
                if ch == '"' {
                    canonical_sql.push(ch);
                    state = ScanState::DoubleQuoted;
                    i += 1;
                    continue;
                }
                if ch == '-' && matches!(chars.get(i + 1), Some((_, '-'))) {
                    canonical_sql.push('-');
                    canonical_sql.push('-');
                    state = ScanState::LineComment;
                    i += 2;
                    continue;
                }
                if ch == '/' && matches!(chars.get(i + 1), Some((_, '*'))) {
                    canonical_sql.push('/');
                    canonical_sql.push('*');
                    state = ScanState::BlockComment { depth: 1 };
                    i += 2;
                    continue;
                }
                if ch == '$' {
                    if let Some((_, next)) = chars.get(i + 1) {
                        if next.is_ascii_digit() {
                            let span = SourceSpan::new(to_u32(byte_idx)?, to_u32(byte_idx + 1)?);
                            return Err(TypedSqlError::unsupported_at(
                                "only named placeholders are supported in typed_sql v1",
                                span,
                            ));
                        }
                        if is_ident_start(*next) {
                            let original_start = byte_idx;
                            let mut j = i + 2;
                            while let Some((_, next_char)) = chars.get(j) {
                                if !is_ident_continue(*next_char) {
                                    break;
                                }
                                j += 1;
                            }
                            let original_end = chars.get(j).map_or(sql.len(), |(idx, _)| *idx);
                            let name: String = chars[i + 1..j].iter().map(|(_, ch)| *ch).collect();
                            let canonical_start = canonical_sql.len();
                            let entry_index =
                                *placeholder_slots.entry(name.clone()).or_insert_with(|| {
                                    let next_slot = placeholders.len() + 1;
                                    placeholders.push(PlaceholderEntry {
                                        id: PlaceholderId((next_slot - 1) as u32),
                                        name: name.clone(),
                                        slot: next_slot as u32,
                                        occurrences: Vec::new(),
                                    });
                                    next_slot - 1
                                });
                            let slot = placeholders[entry_index].slot;
                            let replacement = format!("${slot}");
                            canonical_sql.push_str(&replacement);
                            let canonical_end = canonical_sql.len();
                            let occurrence = PlaceholderOccurrence {
                                original_span: SourceSpan::new(
                                    to_u32(original_start)?,
                                    to_u32(original_end)?,
                                ),
                                canonical_span: SourceSpan::new(
                                    to_u32(canonical_start)?,
                                    to_u32(canonical_end)?,
                                ),
                            };
                            placeholders[entry_index]
                                .occurrences
                                .push(occurrence.clone());
                            replacements.push(ReplacementSpan {
                                canonical: occurrence.canonical_span,
                                original: occurrence.original_span,
                            });
                            i = j;
                            continue;
                        }
                    }
                }

                canonical_sql.push(ch);
                i += 1;
            }
            ScanState::SingleQuoted => {
                canonical_sql.push(ch);
                i += 1;
                if ch == '\'' {
                    if matches!(chars.get(i), Some((_, '\''))) {
                        canonical_sql.push('\'');
                        i += 1;
                    } else {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::DoubleQuoted => {
                canonical_sql.push(ch);
                i += 1;
                if ch == '"' {
                    if matches!(chars.get(i), Some((_, '"'))) {
                        canonical_sql.push('"');
                        i += 1;
                    } else {
                        state = ScanState::Normal;
                    }
                }
            }
            ScanState::LineComment => {
                canonical_sql.push(ch);
                i += 1;
                if ch == '\n' {
                    state = ScanState::Normal;
                }
            }
            ScanState::BlockComment { ref mut depth } => {
                canonical_sql.push(ch);
                if ch == '/' && matches!(chars.get(i + 1), Some((_, '*'))) {
                    canonical_sql.push('*');
                    *depth += 1;
                    i += 2;
                    continue;
                }
                if ch == '*' && matches!(chars.get(i + 1), Some((_, '/'))) {
                    canonical_sql.push('/');
                    *depth -= 1;
                    i += 2;
                    if *depth == 0 {
                        state = ScanState::Normal;
                    }
                    continue;
                }
                i += 1;
            }
        }
    }

    let mut line_starts = vec![0usize];
    for (idx, ch) in canonical_sql.char_indices() {
        if ch == '\n' {
            line_starts.push(idx + 1);
        }
    }

    Ok(SqlSource {
        original_sql: sql.to_owned(),
        canonical_sql,
        source_map: SourceMap { replacements },
        placeholders: PlaceholderTable {
            entries: placeholders,
        },
        line_starts,
    })
}

#[derive(Clone, Copy)]
enum ScanState {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    LineComment,
    BlockComment { depth: usize },
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn nth_char_boundary(input: &str, n: usize) -> Option<usize> {
    if n == 0 {
        return Some(0);
    }

    for (seen, (idx, _)) in input.char_indices().enumerate() {
        if seen == n {
            return Some(idx);
        }
    }

    if input.chars().count() == n {
        Some(input.len())
    } else {
        None
    }
}

fn to_u32(value: usize) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| TypedSqlError::internal(format!("value {value} does not fit in u32")))
}

fn is_trivia(token: &Token) -> bool {
    matches!(token, Token::Whitespace(_))
}

fn is_clause_keyword(token: &str) -> bool {
    matches!(
        token,
        "WHERE" | "GROUP" | "ORDER" | "LIMIT" | "OFFSET" | "JOIN"
    )
}
