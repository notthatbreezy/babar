mod ir;
mod lower;
mod normalize;
mod parse_backend;
mod resolver;
mod source;

use std::borrow::Cow;

pub(crate) use ir::*;
pub(crate) use source::SqlSource;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedSql {
    pub(crate) source: SqlSource,
    pub(crate) select: ParsedSelect,
}

pub(crate) fn parse_select(sql: &str) -> Result<ParsedSql> {
    let source = source::canonicalize(sql)?;
    let query = parse_backend::parse_select(&source)?;
    let select = normalize::normalize_select(&source, &query)?;
    Ok(ParsedSql { source, select })
}

pub(crate) type Result<T, E = TypedSqlError> = std::result::Result<T, E>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypedSqlError {
    pub(crate) kind: TypedSqlErrorKind,
    pub(crate) message: String,
    pub(crate) span: Option<SourceSpan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypedSqlErrorKind {
    Parse,
    Unsupported,
    Resolve,
    Type,
    Internal,
}

impl TypedSqlError {
    fn parse(message: impl Into<String>) -> Self {
        Self {
            kind: TypedSqlErrorKind::Parse,
            message: message.into(),
            span: None,
        }
    }

    fn parse_with_optional_span(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self {
            kind: TypedSqlErrorKind::Parse,
            message: message.into(),
            span,
        }
    }

    fn unsupported(message: impl Into<String>) -> Self {
        Self {
            kind: TypedSqlErrorKind::Unsupported,
            message: message.into(),
            span: None,
        }
    }

    fn unsupported_at(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            kind: TypedSqlErrorKind::Unsupported,
            message: message.into(),
            span: Some(span),
        }
    }

    fn unsupported_with_optional_span(
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: TypedSqlErrorKind::Unsupported,
            message: message.into(),
            span,
        }
    }

    fn resolve_at(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            kind: TypedSqlErrorKind::Resolve,
            message: message.into(),
            span: Some(span),
        }
    }

    fn type_at(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            kind: TypedSqlErrorKind::Type,
            message: message.into(),
            span: Some(span),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: TypedSqlErrorKind::Internal,
            message: message.into(),
            span: None,
        }
    }

    pub(crate) const fn stage_name(&self) -> &'static str {
        match self.kind {
            TypedSqlErrorKind::Parse => "parse",
            TypedSqlErrorKind::Unsupported => "normalize",
            TypedSqlErrorKind::Resolve => "resolve",
            TypedSqlErrorKind::Type => "type",
            TypedSqlErrorKind::Internal => "internal",
        }
    }

    fn headline(&self) -> &'static str {
        match self.kind {
            TypedSqlErrorKind::Parse => "invalid SQL for typed_sql v1",
            TypedSqlErrorKind::Unsupported => {
                "this SQL construct is outside the typed_sql v1 subset"
            }
            TypedSqlErrorKind::Resolve => "could not resolve a table or column in typed_sql v1",
            TypedSqlErrorKind::Type => "typed_sql v1 could not prove this query is well-typed",
            TypedSqlErrorKind::Internal => "typed_sql v1 hit an internal error",
        }
    }

    fn help(&self) -> Option<Cow<'static, str>> {
        match self.kind {
            TypedSqlErrorKind::Parse if self.message.contains("expected an expression") => Some(
                Cow::Borrowed(
                    "add a placeholder like `$id`, a literal, or a qualified column like `users.id`",
                ),
            ),
            TypedSqlErrorKind::Unsupported if self.message.contains("wildcard projections") => Some(
                Cow::Borrowed(
                    "list each selected column explicitly, for example `SELECT users.id, users.name ...`",
                ),
            ),
            TypedSqlErrorKind::Unsupported
                if self.message.contains("fully qualified column references") =>
            {
                Some(Cow::Borrowed(
                    "qualify the column with its relation binding, for example `users.id` or `u.id`",
                ))
            }
            TypedSqlErrorKind::Unsupported if self.message.contains("plain table references") => {
                Some(Cow::Borrowed(
                    "rewrite the query to select directly from real tables with JOIN ... ON",
                ))
            }
            TypedSqlErrorKind::Type if self.message.contains("never constrained") => Some(
                Cow::Borrowed(
                    "compare the placeholder against a qualified column, or use it in LIMIT/OFFSET so its SQL type becomes known",
                ),
            ),
            TypedSqlErrorKind::Internal => Some(Cow::Borrowed(
                "this is a bug in babar; please file an issue with the query that triggered it",
            )),
            _ => None,
        }
    }

    pub(crate) fn render_for_user(&self, source: &SqlSource) -> String {
        let mut rendered = format!("{}: {}", self.headline(), self.message);
        if let Some(span) = self.span {
            if let Some(excerpt) = source.render_span_excerpt(span) {
                rendered.push('\n');
                rendered.push_str(&excerpt);
            }
        }
        if let Some(help) = self.help() {
            rendered.push_str("\nhelp: ");
            rendered.push_str(&help);
        }
        rendered
    }
}

impl std::fmt::Display for TypedSqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.headline(), self.message)?;
        if let Some(help) = self.help() {
            write!(f, "\nhelp: {help}")?;
        }
        if let Some(span) = self.span {
            write!(f, "\nspan: {span}")?;
        }
        Ok(())
    }
}

impl std::error::Error for TypedSqlError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    const CORPUS_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/typed_query/corpus");

    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
    enum CorpusClass {
        ParseOkSupported,
        ParseOkUnsupported,
        SyntaxError,
    }

    impl CorpusClass {
        fn parse(value: &str) -> Self {
            match value {
                "parse-ok-supported" => Self::ParseOkSupported,
                "parse-ok-unsupported" => Self::ParseOkUnsupported,
                "syntax-error" => Self::SyntaxError,
                other => panic!("unsupported corpus class `{other}`"),
            }
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct ExpectedError {
        kind: TypedSqlErrorKind,
        span: Option<SourceSpan>,
        message_contains: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Fixture {
        provenance: String,
        class: CorpusClass,
        name: String,
        sql: String,
        statement_kinds: Vec<String>,
        expected_ir: Option<String>,
        expected_error: Option<ExpectedError>,
    }

    #[test]
    fn canonicalizes_named_placeholders_and_tracks_occurrences() {
        let parsed = parse_select(
            "SELECT users.id FROM users WHERE users.id = $user_id OR users.manager_id = $user_id",
        )
        .expect("query parses");

        assert_eq!(
            parsed.source.canonical_sql,
            "SELECT users.id FROM users WHERE users.id = $1 OR users.manager_id = $1"
        );
        assert_eq!(parsed.source.placeholders.entries().len(), 1);
        let placeholder = &parsed.source.placeholders.entries()[0];
        assert_eq!(placeholder.name, "user_id");
        assert_eq!(placeholder.slot, 1);
        assert_eq!(placeholder.occurrences.len(), 2);
        assert_eq!(
            placeholder.occurrences[0].original_span,
            SourceSpan::new(44, 52)
        );
        assert_eq!(
            placeholder.occurrences[0].canonical_span,
            SourceSpan::new(44, 46)
        );
    }

    #[test]
    fn normalizes_supported_select_subset_into_ir() {
        let parsed = parse_select(
            "SELECT u.id, p.name AS pet_name FROM users AS u INNER JOIN pets AS p ON p.owner_id = u.id WHERE u.id = $user_id AND p.deleted_at IS NULL ORDER BY p.name DESC NULLS LAST LIMIT 10 OFFSET $skip",
        )
        .expect("query parses");

        assert_eq!(parsed.select.projections.len(), 2);
        assert_eq!(parsed.select.from.binding_name.value, "u");
        assert_eq!(parsed.select.joins.len(), 1);
        assert_eq!(parsed.select.joins[0].kind, JoinKind::Inner);
        assert_eq!(parsed.select.order_by.len(), 1);
        assert_eq!(parsed.select.order_by[0].direction, OrderDirection::Desc);
        assert_eq!(parsed.select.order_by[0].nulls, Some(NullsOrder::Last));
        assert!(parsed.select.limit.is_some());
        assert!(parsed.select.offset.is_some());

        let ParsedExpr::BoolChain { op, terms, .. } = parsed.select.filter.expect("where clause")
        else {
            panic!("expected bool chain")
        };
        assert_eq!(op, BoolOp::And);
        assert_eq!(terms.len(), 2);
        assert!(matches!(
            terms[0],
            ParsedExpr::Binary {
                op: BinaryOp::Eq,
                ..
            }
        ));
        assert!(matches!(
            terms[1],
            ParsedExpr::IsNull { negated: false, .. }
        ));
    }

    #[test]
    fn rejects_unsupported_projection_and_bare_columns_during_normalization() {
        let wildcard_error = parse_select("SELECT * FROM users").expect_err("wildcard rejected");
        assert_eq!(wildcard_error.kind, TypedSqlErrorKind::Unsupported);
        assert!(wildcard_error.message.contains("wildcard projections"));

        let bare_column_error =
            parse_select("SELECT id FROM users").expect_err("bare column rejected");
        assert_eq!(bare_column_error.kind, TypedSqlErrorKind::Unsupported);
        assert!(bare_column_error
            .message
            .contains("fully qualified column references"));
    }

    #[test]
    fn rejects_ctes_and_subqueries_outside_v1_subset() {
        let with_error = parse_select("WITH active_users AS (SELECT users.id FROM users) SELECT active_users.id FROM active_users")
            .expect_err("cte rejected");
        assert_eq!(with_error.kind, TypedSqlErrorKind::Unsupported);

        let subquery_error = parse_select(
            "SELECT users.id FROM (SELECT users.id FROM users) AS users WHERE users.id = $id",
        )
        .expect_err("subquery rejected");
        assert_eq!(subquery_error.kind, TypedSqlErrorKind::Unsupported);
        assert!(subquery_error.message.contains("plain table references"));
    }

    #[test]
    fn render_for_user_includes_excerpt_and_help() {
        let err = parse_select("SELECT id FROM users").expect_err("bare column rejected");
        let rendered = err.render_for_user(&source::canonicalize("SELECT id FROM users").unwrap());

        assert!(rendered.contains("outside the typed_sql v1 subset"));
        assert!(rendered.contains("SELECT id FROM users"));
        assert!(rendered.contains("^^"));
        assert!(rendered.contains("help: qualify the column"));
    }

    #[test]
    fn conformance_corpus_matches_expected_ir_and_errors() {
        let fixtures = load_fixtures();
        for fixture in &fixtures {
            match fixture.class {
                CorpusClass::ParseOkSupported => {
                    let parsed = parse_select(&fixture.sql).unwrap_or_else(|err| {
                        panic!("fixture `{}` should parse: {err}", fixture.name)
                    });
                    assert_eq!(fixture.statement_kinds, vec!["select"]);
                    assert_eq!(
                        fixture
                            .expected_ir
                            .as_deref()
                            .expect("expected IR fixture")
                            .trim_end(),
                        render_parsed_sql(&parsed).trim_end(),
                        "fixture `{}` from `{}` should match expected normalized IR",
                        fixture.name,
                        fixture.provenance,
                    );
                }
                CorpusClass::ParseOkUnsupported | CorpusClass::SyntaxError => {
                    let err = parse_select(&fixture.sql).expect_err("negative fixture should fail");
                    let expected = fixture
                        .expected_error
                        .as_ref()
                        .expect("negative fixture should define expected error");
                    assert_eq!(
                        err.kind, expected.kind,
                        "fixture `{}` from `{}` kind mismatch",
                        fixture.name, fixture.provenance,
                    );
                    assert_eq!(
                        err.span, expected.span,
                        "fixture `{}` from `{}` span mismatch",
                        fixture.name, fixture.provenance,
                    );
                    assert!(
                        err.message.contains(&expected.message_contains),
                        "fixture `{}` from `{}` expected `{}` in `{}`",
                        fixture.name,
                        fixture.provenance,
                        expected.message_contains,
                        err.message,
                    );
                }
            }
        }
    }

    #[test]
    fn differential_harness_smoke_runs_on_supported_corpus() {
        let fixtures = load_fixtures()
            .into_iter()
            .filter(|fixture| fixture.class == CorpusClass::ParseOkSupported)
            .collect::<Vec<_>>();
        for fixture in fixtures {
            let primary = parse_select(&fixture.sql).expect("primary parse");
            let oracle = parse_select(&fixture.sql).expect("oracle parse");
            assert_eq!(
                render_parsed_sql(&primary),
                render_parsed_sql(&oracle),
                "fixture `{}` from `{}` should produce identical IR across harness backends",
                fixture.name,
                fixture.provenance,
            );
        }
    }

    #[test]
    fn corpus_layout_covers_planned_provenance_and_classes() {
        let fixtures = load_fixtures();
        let mut summary = BTreeMap::<(&str, CorpusClass), usize>::new();
        for fixture in &fixtures {
            *summary
                .entry((fixture.provenance.as_str(), fixture.class))
                .or_insert(0) += 1;
        }
        assert!(summary.contains_key(&("babar-real", CorpusClass::ParseOkSupported)));
        assert!(summary.contains_key(&("pg-query", CorpusClass::ParseOkSupported)));
        assert!(summary.contains_key(&("postgres-regress", CorpusClass::ParseOkUnsupported)));
        assert!(summary.contains_key(&("pg-parse", CorpusClass::SyntaxError)));
    }

    fn load_fixtures() -> Vec<Fixture> {
        let mut fixtures = Vec::new();
        for provenance_dir in read_dir_sorted(Path::new(CORPUS_ROOT)) {
            let provenance = provenance_dir
                .file_name()
                .expect("provenance dir name")
                .to_string_lossy()
                .into_owned();
            for class_dir in read_dir_sorted(&provenance_dir) {
                let class = CorpusClass::parse(
                    &class_dir
                        .file_name()
                        .expect("class dir name")
                        .to_string_lossy(),
                );
                for sql_path in read_dir_sorted(&class_dir) {
                    if sql_path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
                        continue;
                    }
                    fixtures.push(load_fixture(&provenance, class, &sql_path));
                }
            }
        }
        fixtures
    }

    fn load_fixture(provenance: &str, class: CorpusClass, sql_path: &Path) -> Fixture {
        let stem = sql_path
            .file_stem()
            .expect("fixture stem")
            .to_string_lossy()
            .into_owned();
        let sql = read_to_string(sql_path);
        let meta = parse_meta(&read_to_string(&sql_path.with_extension("meta")));
        let statement_kinds = meta
            .get("statement_kinds")
            .expect("fixture should declare statement_kinds")
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let (expected_ir, expected_error) = match class {
            CorpusClass::ParseOkSupported => (
                Some(read_to_string(&sql_path.with_extension("expected"))),
                None,
            ),
            CorpusClass::ParseOkUnsupported | CorpusClass::SyntaxError => (
                None,
                Some(parse_expected_error(&read_to_string(
                    &sql_path.with_extension("error"),
                ))),
            ),
        };
        Fixture {
            provenance: provenance.to_owned(),
            class,
            name: stem,
            sql,
            statement_kinds,
            expected_ir,
            expected_error,
        }
    }

    fn parse_expected_error(source: &str) -> ExpectedError {
        let meta = parse_meta(source);
        ExpectedError {
            kind: match meta
                .get("stage")
                .expect("error file should declare stage")
                .as_str()
            {
                "parse" => TypedSqlErrorKind::Parse,
                "normalize" => TypedSqlErrorKind::Unsupported,
                "resolve" => TypedSqlErrorKind::Resolve,
                "type" => TypedSqlErrorKind::Type,
                other => panic!("unsupported stage `{other}`"),
            },
            span: meta.get("span").map(|value| parse_span(value)),
            message_contains: meta
                .get("message_contains")
                .expect("error file should declare message_contains")
                .to_owned(),
        }
    }

    fn parse_meta(source: &str) -> BTreeMap<String, String> {
        let mut meta = BTreeMap::new();
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .unwrap_or_else(|| panic!("metadata line must contain `=`: {line}"));
            meta.insert(key.trim().to_owned(), value.trim().to_owned());
        }
        meta
    }

    fn parse_span(value: &str) -> SourceSpan {
        let (start, end) = value
            .split_once("..")
            .unwrap_or_else(|| panic!("invalid span `{value}`"));
        SourceSpan::new(
            start.parse().expect("valid span start"),
            end.parse().expect("valid span end"),
        )
    }

    fn read_dir_sorted(path: &Path) -> Vec<PathBuf> {
        let mut entries = fs::read_dir(path)
            .unwrap_or_else(|err| panic!("failed to read {path:?}: {err}"))
            .map(|entry| entry.expect("dir entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    fn read_to_string(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|err| panic!("failed to read {path:?}: {err}"))
    }

    fn render_parsed_sql(parsed: &ParsedSql) -> String {
        let mut out = String::new();
        writeln!(&mut out, "select {}", parsed.select.span).expect("write header");
        writeln!(&mut out, "placeholders:").expect("write placeholders header");
        for entry in parsed.source.placeholders.entries() {
            writeln!(
                &mut out,
                "  - ${} @ {}",
                entry.name, entry.occurrences[0].original_span,
            )
            .expect("write placeholder");
        }
        writeln!(&mut out, "projections:").expect("write projections header");
        for projection in &parsed.select.projections {
            writeln!(
                &mut out,
                "  - {} -> {}",
                render_expr(&projection.expr, &parsed.source),
                render_output_name(&projection.output_name),
            )
            .expect("write projection");
        }
        writeln!(
            &mut out,
            "from: {} as {}",
            render_object_name(&parsed.select.from.table_name),
            parsed.select.from.binding_name.value,
        )
        .expect("write from");
        writeln!(&mut out, "joins:").expect("write joins header");
        for join in &parsed.select.joins {
            writeln!(
                &mut out,
                "  - {} {} as {} on {}",
                match join.kind {
                    JoinKind::Inner => "inner",
                    JoinKind::Left => "left",
                    JoinKind::Right => "right",
                    JoinKind::Full => "full",
                },
                render_object_name(&join.right.table_name),
                join.right.binding_name.value,
                render_expr(&join.on, &parsed.source),
            )
            .expect("write join");
        }
        writeln!(
            &mut out,
            "where: {}",
            parsed.select.filter.as_ref().map_or_else(
                || "<none>".to_owned(),
                |expr| render_expr(expr, &parsed.source)
            ),
        )
        .expect("write where");
        writeln!(&mut out, "order_by:").expect("write order header");
        for item in &parsed.select.order_by {
            let mut rendered = format!(
                "  - {} {}",
                render_expr(&item.expr, &parsed.source),
                match item.direction {
                    OrderDirection::Asc => "asc",
                    OrderDirection::Desc => "desc",
                },
            );
            if let Some(nulls) = item.nulls {
                rendered.push_str(" nulls ");
                rendered.push_str(match nulls {
                    NullsOrder::First => "first",
                    NullsOrder::Last => "last",
                });
            }
            writeln!(&mut out, "{rendered}").expect("write order item");
        }
        writeln!(
            &mut out,
            "limit: {}",
            parsed.select.limit.as_ref().map_or_else(
                || "<none>".to_owned(),
                |limit| render_expr(&limit.expr, &parsed.source)
            ),
        )
        .expect("write limit");
        writeln!(
            &mut out,
            "offset: {}",
            parsed.select.offset.as_ref().map_or_else(
                || "<none>".to_owned(),
                |offset| render_expr(&offset.expr, &parsed.source)
            ),
        )
        .expect("write offset");
        out.trim_end().to_owned()
    }

    fn render_object_name(name: &ObjectNameSyntax) -> String {
        name.parts
            .iter()
            .map(|part| part.value.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }

    fn render_output_name(name: &OutputNameSyntax) -> &str {
        match name {
            OutputNameSyntax::Explicit(ident) | OutputNameSyntax::Implicit(ident) => &ident.value,
            OutputNameSyntax::Anonymous => "<anonymous>",
        }
    }

    fn render_expr(expr: &ParsedExpr, source: &SqlSource) -> String {
        match expr {
            ParsedExpr::Column(column) => {
                format!("{}.{}", column.binding.value, column.column.value)
            }
            ParsedExpr::Placeholder(placeholder) => format!("${}", placeholder.name),
            ParsedExpr::Literal(literal) => match &literal.value {
                Literal::Number(value) => value.clone(),
                Literal::String(value) => format!("'{value}'"),
                Literal::Boolean(value) => value.to_string(),
                Literal::Null => "NULL".to_owned(),
            },
            ParsedExpr::Unary { op, expr, .. } => format!(
                "({} {})",
                match op {
                    UnaryOp::Not => "NOT",
                    UnaryOp::Plus => "+",
                    UnaryOp::Minus => "-",
                },
                render_expr(expr, source),
            ),
            ParsedExpr::Binary {
                op, left, right, ..
            } => format!(
                "({} {} {})",
                render_expr(left, source),
                match op {
                    BinaryOp::Eq => "=",
                    BinaryOp::NotEq => "<>",
                    BinaryOp::Lt => "<",
                    BinaryOp::LtEq => "<=",
                    BinaryOp::Gt => ">",
                    BinaryOp::GtEq => ">=",
                },
                render_expr(right, source),
            ),
            ParsedExpr::IsNull { negated, expr, .. } => format!(
                "({} IS {}NULL)",
                render_expr(expr, source),
                if *negated { "NOT " } else { "" },
            ),
            ParsedExpr::BoolChain { op, terms, .. } => format!(
                "{}({})",
                match op {
                    BoolOp::And => "AND",
                    BoolOp::Or => "OR",
                },
                terms
                    .iter()
                    .map(|term| render_expr(term, source))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        }
    }
}
