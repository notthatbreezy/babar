//! Cross-crate coverage for exported schema modules.

use babar::query::{Command, Query};
use babar_external_schema_export::exported_schema;

#[test]
fn exported_schema_exposes_query_across_crate_boundaries() {
    let schema_scoped: Query<(i32,), (String,)> = exported_schema::query!(
        SELECT users.name FROM users WHERE users.id = $id AND users.active = true
    );
    let inline: Query<(i32,), (String,)> = babar::query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT users.name FROM users WHERE users.id = $id AND users.active = true
    );

    assert_eq!(schema_scoped.sql(), inline.sql());
    assert_eq!(schema_scoped.param_oids(), inline.param_oids());
    assert_eq!(schema_scoped.output_oids(), inline.output_oids());
}

#[test]
fn exported_schema_exposes_command_across_crate_boundaries() {
    let schema_scoped: Command<(i32, String, bool)> = exported_schema::command!(
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );
    let inline: Command<(i32, String, bool)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );

    assert_eq!(schema_scoped.sql(), inline.sql());
    assert_eq!(schema_scoped.param_oids(), inline.param_oids());
}
