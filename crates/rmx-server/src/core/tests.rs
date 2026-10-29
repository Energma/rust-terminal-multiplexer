use super::helpers::unescape;

#[test]
fn unescapes_send_keys() {
    assert_eq!(unescape(r"ls\r\tx\\y\e"), "ls\r\tx\\y\x1b");
}
