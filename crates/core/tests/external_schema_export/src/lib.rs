//! Support crate for cross-crate authored schema tests.

use babar::schema;

schema! {
    pub mod exported_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
    }
}
