use super::labels::ellipsize_start;

#[test]
fn ellipsize_keeps_the_end() {
    assert_eq!(ellipsize_start("~/code/api", 20), "~/code/api");
    assert_eq!(ellipsize_start("/very/long/path/to/api", 8), "…/to/api");
    assert_eq!(ellipsize_start("abc", 0), "");
}
