use babar::query::Query;

fn main() {
    let _query: Query<(i64,), (String,)> = babar::query!(
        schema = {
            table public.verify_live_users {
                id: int8,
                name: text,
                active: bool,
            },
        },
        SELECT verify_live_users.name
        FROM public.verify_live_users
        WHERE verify_live_users.id = $id AND verify_live_users.active = true
    );
}
