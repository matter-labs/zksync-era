pub fn slugify(data: &str) -> String {
    data.trim().replace(" ", "-")
}
