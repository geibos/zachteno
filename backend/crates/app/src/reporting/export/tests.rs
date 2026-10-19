use std::io::Cursor;

use calamine::{Data, Reader as _, Xlsx, open_workbook_from_rs};
use rust_decimal::Decimal;
use time::macros::datetime;
use uuid::Uuid;

use super::sheets::{Datum, Sheet, submit_reason_text, test_sheets};
use super::xlsx::to_xlsx;
use super::{neutralize, to_csv};
use crate::reporting::data::Cell;
use crate::reporting::tables::{ColumnHeader, ResultsTable, TableRow};

#[test]
fn formula_like_cells_are_neutralized() {
    assert_eq!(neutralize("=HYPERLINK(\"x\")"), "'=HYPERLINK(\"x\")", "формула");
    assert_eq!(neutralize("-1"), "'-1", "минус в начале");
    assert_eq!(neutralize("обычный текст"), "обычный текст", "текст без изменений");
}

fn sheet(rows: Vec<Vec<Datum>>) -> Sheet {
    Sheet {
        name: "Лист",
        header: vec!["Имя".to_owned(), "Балл".to_owned()],
        rows,
        frozen_columns: 0,
    }
}

fn csv_body(sheet: &Sheet) -> String {
    let bytes = to_csv(sheet).unwrap();
    assert_eq!(bytes.get(..3), Some(&b"\xEF\xBB\xBF"[..]), "BOM");
    String::from_utf8(bytes.get(3..).unwrap().to_vec()).unwrap()
}

#[test]
fn csv_uses_semicolon_and_decimal_comma() {
    let body = csv_body(&sheet(vec![vec![
        Datum::Text("Иван".to_owned()),
        Datum::Number(Decimal::new(15, 1)),
    ]]));
    assert_eq!(body, "Имя;Балл\nИван;1,5\n", "разделитель ; и запятая в дроби");
}

#[test]
fn csv_keeps_negative_numbers_and_escapes_text() {
    let body = csv_body(&sheet(vec![vec![
        Datum::Text("=HYPERLINK(\"x\")".to_owned()),
        Datum::Number(Decimal::from(-96)),
    ]]));
    assert!(body.contains("'=HYPERLINK"), "текст экранирован: {body}");
    assert!(body.ends_with(";-96\n"), "число без апострофа: {body}");
}

#[test]
fn csv_writes_percent_as_number_and_moment_in_msk() {
    let body = csv_body(&sheet(vec![vec![
        Datum::Moment(datetime!(2026-10-19 10:56 UTC)),
        Datum::Percent(Decimal::new(484, 1)),
    ]]));
    assert_eq!(body.lines().nth(1), Some("19.10.2026 13:56;48,4"), "{body}");
}

#[test]
fn submit_reasons_are_russian() {
    assert_eq!(submit_reason_text("manual"), Some("вручную"), "вручную");
    assert_eq!(submit_reason_text("timeout"), Some("по времени"), "по времени");
    assert_eq!(submit_reason_text("window_closed"), Some("при закрытии теста"), "закрытие");
}

/// Ячейка вопроса, который был в попытке.
fn cell(score: i64, selected: &[&str]) -> Cell {
    Cell {
        question_id: Uuid::nil(),
        in_attempt: true,
        score: Some(Decimal::from(score)),
        selected: selected.iter().map(|&label| label.to_owned()).collect(),
        text_answer: None,
        free_text: false,
    }
}

const CHOSEN: &str = "Поменять аккумуляторы в фонаре; Застегнуть куртку";

/// Вопрос 1 с двумя выбранными вариантами, вопроса 2 в попытке не было.
fn cells() -> Vec<Cell> {
    vec![
        cell(0, &["Поменять аккумуляторы в фонаре", "Застегнуть куртку"]),
        Cell { in_attempt: false, score: None, ..cell(0, &[]) },
    ]
}

fn row() -> TableRow {
    TableRow {
        attempt_id: Uuid::nil(),
        user_id: Uuid::nil(),
        display_name: "Сергей Король".to_owned(),
        email: "s@example.com".to_owned(),
        stream_name: Some("Осень".to_owned()),
        removed: false,
        attempt_number: 1,
        started_at: datetime!(2026-10-19 10:56 UTC),
        submitted_at: datetime!(2026-10-19 11:02 UTC),
        duration_seconds: 359,
        submit_reason: Some("manual".to_owned()),
        score: Decimal::from(45),
        max_score: Decimal::from(93),
        percent: Some(Decimal::new(484, 1)),
        final_score: Some(Decimal::from(45)),
        final_percent: Some(Decimal::new(484, 1)),
        antifraud: false,
        cells: cells(),
    }
}

fn column(number: usize, body: &str) -> ColumnHeader {
    ColumnHeader {
        question_id: Uuid::nil(),
        number,
        body: body.to_owned(),
        free_text: false,
        weight: Decimal::ONE,
        max_score: Decimal::ONE,
    }
}

fn table() -> ResultsTable {
    ResultsTable {
        test_id: Uuid::nil(),
        title: "Тест".to_owned(),
        columns: vec![column(1, "Что сделать?"), column(2, "Через сколько?")],
        rows: vec![row()],
    }
}

#[test]
fn results_sheet_holds_scores_as_numbers() {
    let sheets = test_sheets(&table());
    let names: Vec<_> = sheets.iter().map(|found| found.name).collect();
    assert_eq!(names, ["Результаты", "Ответы", "Вопросы"], "листы");
    let results = sheets.first().unwrap();
    let questions = ["В1".to_owned(), "В2".to_owned()];
    assert_eq!(results.header.get(13..), Some(&questions[..]), "колонки вопросов");
    let line = results.rows.first().unwrap();
    assert_eq!(line.get(7), Some(&Datum::Text("вручную".to_owned())), "способ сдачи");
    assert_eq!(line.get(6), Some(&Datum::Number(Decimal::from(5))), "минуты");
    assert_eq!(line.get(13..), Some(&[Datum::Number(Decimal::ZERO), Datum::Empty][..]), "баллы");
}

#[test]
fn answers_and_questions_sheets_hold_text() {
    let sheets = test_sheets(&table());
    let answers = sheets.get(1).and_then(|sheet| sheet.rows.first()).unwrap();
    let expected = [Datum::Text(CHOSEN.to_owned()), Datum::Empty];
    assert_eq!(answers.get(4..), Some(&expected[..]), "ответы через «; »");
    let question = sheets.get(2).and_then(|sheet| sheet.rows.get(1)).unwrap();
    assert_eq!(question.get(1), Some(&Datum::Text("Через сколько?".to_owned())), "текст вопроса");
}

#[test]
fn xlsx_keeps_numbers_percent_and_dates_typed() {
    let bytes = to_xlsx(&test_sheets(&table())).unwrap();
    let mut book: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes)).unwrap();
    assert_eq!(book.sheet_names(), ["Результаты", "Ответы", "Вопросы"], "листы");
    let results = book.worksheet_range("Результаты").unwrap();
    assert_eq!(results.get_value((0, 0)), Some(&Data::String("Студент".to_owned())), "шапка");
    assert_eq!(results.get_value((1, 8)), Some(&Data::Float(45.0)), "балл числом");
    assert_eq!(results.get_value((1, 10)), Some(&Data::Float(0.484)), "процент долей");
    assert!(
        matches!(results.get_value((1, 4)), Some(Data::DateTime(_))),
        "дата: {:?}",
        results.get_value((1, 4))
    );
    assert_eq!(results.get_value((1, 13)), Some(&Data::Float(0.0)), "балл за вопрос");
    let answers = book.worksheet_range("Ответы").unwrap();
    assert_eq!(answers.get_value((1, 4)), Some(&Data::String(CHOSEN.to_owned())), "ответ текстом");
}

#[test]
fn xlsx_writes_formula_like_text_as_text() {
    let bytes =
        to_xlsx(&[sheet(vec![vec![Datum::Text("=HYPERLINK(\"x\")".to_owned()), Datum::Empty]])])
            .unwrap();
    let mut book: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes)).unwrap();
    let range = book.worksheet_range("Лист").unwrap();
    assert_eq!(
        range.get_value((1, 0)),
        Some(&Data::String("=HYPERLINK(\"x\")".to_owned())),
        "текст как есть"
    );
    assert!(book.worksheet_formula("Лист").unwrap().used_cells().next().is_none(), "формул нет");
}
