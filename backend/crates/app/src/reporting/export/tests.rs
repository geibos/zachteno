use super::{neutralize, to_csv};

#[test]
fn formula_like_cells_are_neutralized() {
    assert_eq!(neutralize("=HYPERLINK(\"x\")"), "'=HYPERLINK(\"x\")", "формула");
    assert_eq!(neutralize("-1"), "'-1", "минус в начале");
    assert_eq!(neutralize("обычный текст"), "обычный текст", "текст без изменений");
}

#[test]
fn csv_starts_with_bom_and_uses_semicolon() {
    let bytes = to_csv(&[vec!["Иван".to_owned(), "5".to_owned()]]).unwrap();
    assert_eq!(&bytes[..3], b"\xEF\xBB\xBF", "BOM");
    assert_eq!(String::from_utf8(bytes[3..].to_vec()).unwrap(), "Иван;5\n", "разделитель ;");
}
