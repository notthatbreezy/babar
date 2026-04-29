use babar::query::Query;

babar::schema! {
    mod app_schema {
        table service.widgets {
            id: primary_key(int4),
            name: text,
        },
    }
}

fn main() {
    let _query: Query<(), (i32,)> = app_schema::typed_query!(
        SELECT users.id FROM service.users
    );
}
