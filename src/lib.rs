pub fn project_name() -> &'static str {
    "chess-review"
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn reports_project_name() {
        assert_eq!(project_name(), "chess-review")
    }
}
