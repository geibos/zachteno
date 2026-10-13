use super::content_security_policy;

#[test]
fn inline_script_hash_is_added_to_csp() {
    let page = r#"<html><script src="/a.js"></script><script>alert(1)</script></html>"#;
    let csp = content_security_policy(page);
    assert!(csp.contains("script-src 'self' 'sha256-"), "хеш inline-скрипта: {csp}");
    assert_eq!(csp.matches("'sha256-").count(), 1, "скрипт с src не хешируется");
}
