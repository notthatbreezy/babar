//! Internal tracing helpers.

use tracing::{field, info_span, Span};

use crate::config::Config;

pub(crate) fn connect_span(config: &Config) -> Span {
    info_span!(
        "db.connect",
        db.system = field::display("postgresql"),
        db.user = field::display(config.user_str()),
        db.name = field::display(config.database_str()),
        net.peer.name = field::display(config.host_str()),
        net.peer.port = i64::from(config.port),
    )
}

pub(crate) fn prepare_span(sql: &str) -> Span {
    info_span!(
        "db.prepare",
        db.system = field::display("postgresql"),
        db.statement = field::display(sql),
        db.operation = field::display(sql_operation(sql)),
    )
}

pub(crate) fn execute_span(sql: &str) -> Span {
    info_span!(
        "db.execute",
        db.system = field::display("postgresql"),
        db.statement = field::display(sql),
        db.operation = field::display(sql_operation(sql)),
    )
}

pub(crate) fn transaction_span(label: &str) -> Span {
    info_span!(
        "db.transaction",
        db.system = field::display("postgresql"),
        db.operation = field::display(label),
    )
}

fn sql_operation(sql: &str) -> &str {
    sql.split_whitespace().next().unwrap_or("<unknown>")
}
