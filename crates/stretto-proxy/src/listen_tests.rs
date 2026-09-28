use super::*;

#[test]
fn a_url_names_loopback_for_an_unspecified_address() {
    let at = |a: &str| url(a.parse().unwrap());
    assert_eq!(at("0.0.0.0:8931"), "http://127.0.0.1:8931/mcp");
    assert_eq!(at("[::]:8931"), "http://[::1]:8931/mcp");
    assert_eq!(at("192.168.1.5:80"), "http://192.168.1.5:80/mcp");
}
