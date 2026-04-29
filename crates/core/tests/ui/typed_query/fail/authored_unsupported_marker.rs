babar::schema! {
    mod app_schema {
        table public.users {
            id: indexed(int4),
        },
    }
}

fn main() {}
