use super::Email;

/// Текст письма подтверждения для имени `name`.
fn verify_body(name: &str) -> String {
    let email = Email::VerifyAddress {
        to: "victim@example.com".to_owned(),
        name: name.to_owned(),
        token: "t".to_owned(),
    };
    email.render("https://zachteno.example").1
}

#[test]
fn plain_name_is_greeted() {
    assert!(
        verify_body("Иван Петров").starts_with("Здравствуйте, Иван Петров!\n"),
        "обычное имя в приветствии"
    );
}

#[test]
fn name_that_looks_like_link_is_not_put_into_letter() {
    for name in ["Служба безопасности evil.ru", "admin@evil.ru", "зайдите на evil:8080/x"]
    {
        let body = verify_body(name);
        assert!(body.starts_with("Здравствуйте!\n"), "безымянное приветствие для {name:?}");
        assert!(!body.contains("evil"), "имя не попало в письмо: {body}");
    }
}
