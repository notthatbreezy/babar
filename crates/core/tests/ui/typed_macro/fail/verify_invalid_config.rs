fn main() {
    let _ = babar::query!(
        schema = {
            table public.verify_live_users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT verify_live_users.id, verify_live_users.name FROM public.verify_live_users WHERE verify_live_users.id = $id
    );
}
