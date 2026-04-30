use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
        table reporting.users {
            id: primary_key(int8),
            name: text,
            active: bool,
        },
    }
}

fn main() {
    let _public_id = app_schema::public::users::id();
    let _reporting_id = app_schema::reporting::users::id();

    let _public_users: Query<(), (String,)> = app_schema::query!(
        SELECT users.name FROM public.users WHERE users.active = true
    );
    let _reporting_users: Query<(), (String,)> = app_schema::query!(
        SELECT users.name FROM reporting.users WHERE users.active = true
    );
}
