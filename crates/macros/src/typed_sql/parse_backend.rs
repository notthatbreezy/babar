use sqlparser::{
    ast::{Query, SetExpr, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use super::{source::SqlSource, Result, TypedSqlError};

pub(crate) fn parse_statement(source: &SqlSource) -> Result<Statement> {
    let dialect = PostgreSqlDialect {};
    let mut parser = Parser::new(&dialect)
        .try_with_sql(&source.canonical_sql)
        .map_err(|error| map_parser_error(source, &error.to_string()))?;
    let mut statements = parser
        .parse_statements()
        .map_err(|error| map_parser_error(source, &error.to_string()))?;

    if statements.len() != 1 {
        return Err(TypedSqlError::unsupported(
            "typed_sql v1 expects exactly one statement",
        ));
    }

    let statement = statements.pop().expect("statement length checked");
    match &statement {
        Statement::Query(query) => {
            if !matches!(query.body.as_ref(), SetExpr::Select(_)) {
                return Err(TypedSqlError::unsupported(
                    "typed_sql v1 does not support set operations or derived top-level queries",
                ));
            }
        }
        Statement::Insert(_) | Statement::Update(_) | Statement::Delete(_) => {}
        _ => {
            return Err(TypedSqlError::unsupported(
                "typed_sql v1 only supports SELECT, INSERT, UPDATE, and DELETE statements",
            ))
        }
    }

    Ok(statement)
}

pub(crate) fn parse_select(source: &SqlSource) -> Result<Query> {
    let statement = parse_statement(source)?;
    let Statement::Query(query) = statement else {
        return Err(TypedSqlError::unsupported(
            "typed_sql v1 only supports SELECT statements",
        ));
    };
    Ok(*query)
}

fn map_parser_error(source: &SqlSource, error: &str) -> TypedSqlError {
    let (message, location) = split_error_location(error);
    let normalized = message
        .strip_prefix("sql parser error: ")
        .unwrap_or(&message)
        .trim();
    if normalized == "Expected: end of statement, found: ." {
        if let Ok(Some((keyword, span))) = source.clause_keyword_after_from() {
            return TypedSqlError::parse_with_optional_span(
                format!("expected a table name after FROM before `{keyword}`"),
                Some(span),
            );
        }
    }
    let found_token = extract_found_token(normalized);
    let message = clean_parse_message(normalized);
    let span = location.and_then(|(line, column)| {
        source
            .anchor_parser_error_span(line, column, found_token.as_deref())
            .ok()
            .flatten()
    });
    TypedSqlError::parse_with_optional_span(message, span)
}

fn split_error_location(error: &str) -> (String, Option<(u64, u64)>) {
    let Some((message, suffix)) = error.rsplit_once(" at Line: ") else {
        return (error.trim().to_owned(), None);
    };
    let Some((line, column)) = suffix.split_once(", Column: ") else {
        return (error.trim().to_owned(), None);
    };
    let Ok(line) = line.trim().parse::<u64>() else {
        return (error.trim().to_owned(), None);
    };
    let Ok(column) = column.trim().parse::<u64>() else {
        return (error.trim().to_owned(), None);
    };
    (message.trim().to_owned(), Some((line, column)))
}

fn clean_parse_message(message: &str) -> String {
    let message = message.trim();
    let Some(details) = message.strip_prefix("Expected: ") else {
        return message.to_owned();
    };
    let Some((expected, found)) = details.split_once(", found: ") else {
        return message.to_owned();
    };

    let expected = lowercase_first(expected.trim());
    let found = match found.trim() {
        "EOF" => "end of input".to_owned(),
        other => format!("`{other}`"),
    };
    format!("expected {expected} before {found}")
}

fn extract_found_token(message: &str) -> Option<String> {
    let details = message.trim().strip_prefix("Expected: ")?;
    let (_, found) = details.split_once(", found: ")?;
    let found = found.trim();
    (found != "EOF").then(|| found.to_owned())
}

fn lowercase_first(input: &str) -> String {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_lowercase().chain(chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_sql::source::canonicalize;

    #[test]
    fn parser_errors_anchor_to_next_token_when_available() {
        let source =
            canonicalize("SELECT users.id FROM WHERE users.id = $id;").expect("source parses");
        let err = parse_select(&source).expect_err("query should fail to parse");

        assert_eq!(err.stage_name(), "parse");
        assert_eq!(err.span, Some(crate::typed_sql::SourceSpan::new(21, 26)));
        assert!(err
            .message
            .contains("expected a table name after FROM before `WHERE`"));
    }
}
