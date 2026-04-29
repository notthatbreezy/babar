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
    let _query: Query<(), (String,)> = app_schema::typed_query!(
        SELECT widgets.handle FROM service.widgets
    );
}
