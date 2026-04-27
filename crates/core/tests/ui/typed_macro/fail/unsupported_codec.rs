fn main() {
    let _ = babar::query!(
        "SELECT id FROM users WHERE id = $1",
        params = (uuid,),
        row = (int4,),
    );
}
