//! Листы выгрузки: одна модель для CSV и XLSX, чтобы колонки не разъезжались.

use std::iter::once;

use rust_decimal::Decimal;
use time::OffsetDateTime;

use crate::reporting::data::Cell;
use crate::reporting::tables::{
    ColumnHeader, CourseSummaryTable, ResultsTable, SummaryCell, TableRow,
};

/// Значение ячейки выгрузки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Datum {
    /// Текст: имя, ответ, подпись.
    Text(String),
    /// Число: балл, номер попытки, минуты.
    Number(Decimal),
    /// Процент в процентах (48.4 — это 48,4 %).
    Percent(Decimal),
    /// Момент времени, выводится по Москве.
    Moment(OffsetDateTime),
    /// Пустая ячейка.
    Empty,
}

/// Лист выгрузки.
#[derive(Debug)]
pub(crate) struct Sheet {
    /// Имя листа в книге.
    pub name: &'static str,
    pub header: Vec<String>,
    pub rows: Vec<Vec<Datum>>,
    /// Сколько первых колонок закрепить вместе с шапкой.
    pub frozen_columns: u16,
}

/// Колонки строки попытки перед колонками вопросов на листе «Результаты».
const RESULTS_HEADER: [&str; 13] = [
    "Студент",
    "Email",
    "Поток",
    "Попытка",
    "Начало (МСК)",
    "Сдача (МСК)",
    "Длительность, мин",
    "Как сдана",
    "Балл",
    "Максимум",
    "Процент",
    "Итог",
    "Итог, %",
];

/// Колонки, по которым узнаётся попытка на листе «Ответы».
const ANSWERS_HEADER: [&str; 4] = ["Студент", "Email", "Поток", "Попытка"];

/// Разделитель выбранных вариантов: запятая встречается внутри самих вариантов.
const OPTION_SEPARATOR: &str = "; ";

/// Листы выгрузки теста: «Результаты», «Ответы», «Вопросы».
pub(crate) fn test_sheets(table: &ResultsTable) -> Vec<Sheet> {
    vec![results_sheet(table), answers_sheet(table), questions_sheet(&table.columns)]
}

fn header_with_questions(fixed: &[&str], columns: &[ColumnHeader]) -> Vec<String> {
    fixed
        .iter()
        .map(|&title| title.to_owned())
        .chain(columns.iter().map(|column| format!("В{}", column.number)))
        .collect()
}

fn results_sheet(table: &ResultsTable) -> Sheet {
    Sheet {
        name: "Результаты",
        header: header_with_questions(&RESULTS_HEADER, &table.columns),
        rows: table.rows.iter().map(results_row).collect(),
        frozen_columns: 1,
    }
}

fn results_row(row: &TableRow) -> Vec<Datum> {
    let mut line = vec![
        Datum::Text(row.display_name.clone()),
        Datum::Text(row.email.clone()),
        text_or_empty(row.stream_name.as_deref()),
        Datum::Number(Decimal::from(row.attempt_number)),
        Datum::Moment(row.started_at),
        Datum::Moment(row.submitted_at),
        Datum::Number(Decimal::from(row.duration_seconds.checked_div(60).unwrap_or_default())),
        text_or_empty(row.submit_reason.as_deref().and_then(submit_reason_text)),
        Datum::Number(row.score),
        Datum::Number(row.max_score),
        percent_or_empty(row.percent),
        row.final_score.map_or(Datum::Empty, Datum::Number),
        percent_or_empty(row.final_percent),
    ];
    line.extend(row.cells.iter().map(|cell| cell.score.map_or(Datum::Empty, Datum::Number)));
    line
}

fn answers_sheet(table: &ResultsTable) -> Sheet {
    let rows = table.rows.iter().map(|row| {
        let mut line = vec![
            Datum::Text(row.display_name.clone()),
            Datum::Text(row.email.clone()),
            text_or_empty(row.stream_name.as_deref()),
            Datum::Number(Decimal::from(row.attempt_number)),
        ];
        line.extend(row.cells.iter().map(answer));
        line
    });
    Sheet {
        name: "Ответы",
        header: header_with_questions(&ANSWERS_HEADER, &table.columns),
        rows: rows.collect(),
        frozen_columns: 1,
    }
}

/// Ответ текстом: свободный ответ или выбранные варианты; пусто, если вопроса не было или ответа нет.
fn answer(cell: &Cell) -> Datum {
    if !cell.in_attempt {
        return Datum::Empty;
    }
    let text = cell.text_answer.clone().unwrap_or_else(|| cell.selected.join(OPTION_SEPARATOR));
    text_or_empty(Some(&text))
}

fn questions_sheet(columns: &[ColumnHeader]) -> Sheet {
    let rows = columns.iter().map(|column| {
        vec![
            Datum::Number(Decimal::from(column.number)),
            Datum::Text(column.body.clone()),
            Datum::Number(column.weight),
            Datum::Number(column.max_score),
        ]
    });
    Sheet {
        name: "Вопросы",
        header: ["№", "Вопрос", "Вес", "Максимум"].map(str::to_owned).to_vec(),
        rows: rows.collect(),
        frozen_columns: 0,
    }
}

/// Сводная курса: по две колонки на тест — итоговый балл и итоговый процент.
pub(crate) fn summary_sheet(summary: &CourseSummaryTable) -> Sheet {
    let header = ["Студент".to_owned(), "Email".to_owned()]
        .into_iter()
        .chain(
            summary
                .tests
                .iter()
                .flat_map(|test| [format!("{}, балл", test.title), format!("{}, %", test.title)]),
        )
        .collect();
    let rows = summary.rows.iter().map(|row| {
        once(Datum::Text(row.display_name.clone()))
            .chain(once(Datum::Text(row.email.clone())))
            .chain(row.cells.iter().flat_map(summary_cell))
            .collect()
    });
    Sheet { name: "Сводная", header, rows: rows.collect(), frozen_columns: 1 }
}

fn summary_cell(cell: &SummaryCell) -> [Datum; 2] {
    match (cell.final_score, cell.max_score) {
        (Some(score), Some(_)) => [Datum::Number(score), percent_or_empty(cell.final_percent)],
        (None, Some(_)) => [Datum::Text("скрыт".to_owned()), Datum::Empty],
        (_, None) => [Datum::Text("нет попыток".to_owned()), Datum::Empty],
    }
}

/// Способ сдачи по-русски.
pub(crate) fn submit_reason_text(reason: &str) -> Option<&'static str> {
    match reason {
        "manual" => Some("вручную"),
        "timeout" => Some("по времени"),
        "window_closed" => Some("при закрытии теста"),
        _ => None,
    }
}

fn text_or_empty(text: Option<&str>) -> Datum {
    text.filter(|found| !found.is_empty())
        .map_or(Datum::Empty, |found| Datum::Text(found.to_owned()))
}

fn percent_or_empty(percent: Option<Decimal>) -> Datum {
    percent.map_or(Datum::Empty, Datum::Percent)
}
