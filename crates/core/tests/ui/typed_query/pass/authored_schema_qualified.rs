use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
        },
        table audit_logs.events {
            id: pk(int8),
            title: text,
        },
    }
}

fn main() {
    let _users: Query<(), (i32, String)> = app_schema::typed_query!(
        SELECT users.id, users.name FROM public.users
    );
    let _events: Query<(), (i64, String)> = app_schema::typed_query!(
        SELECT events.id, events.title FROM audit_logs.events
    );
}
