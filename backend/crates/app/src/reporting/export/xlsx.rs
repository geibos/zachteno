//! Запись листов выгрузки в книгу Excel.

use anyhow::Context as _;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive as _;
use rust_xlsxwriter::{ColNum, ExcelDateTime, Format, RowNum, Workbook, Worksheet};
use time::OffsetDateTime;

use super::msk_local;
use super::sheets::{Datum, Sheet};

/// Верхняя граница ширины колонки при подборе, в пикселях: длинные тексты не растягивают лист.
const MAX_COLUMN_WIDTH_PX: u32 = 320;

/// Ширина колонки с датой и временем `ДД.ММ.ГГГГ ЧЧ:ММ`, в символах.
const MOMENT_WIDTH_CHARS: u16 = 17;

struct Formats {
    header: Format,
    percent: Format,
    moment: Format,
}

impl Formats {
    fn new() -> Self {
        Self {
            header: Format::new().set_bold(),
            percent: Format::new().set_num_format("0.0%"),
            moment: Format::new().set_num_format("dd.mm.yyyy hh:mm"),
        }
    }
}

/// Книга XLSX из листов в заданном порядке.
pub(crate) fn to_xlsx(sheets: &[Sheet]) -> anyhow::Result<Vec<u8>> {
    let mut book = Workbook::new();
    let formats = Formats::new();
    for sheet in sheets {
        write_sheet(book.add_worksheet(), sheet, &formats)?;
    }
    Ok(book.save_to_buffer()?)
}

fn write_sheet(worksheet: &mut Worksheet, sheet: &Sheet, formats: &Formats) -> anyhow::Result<()> {
    let _named = worksheet.set_name(sheet.name)?;
    for (column, title) in (0..).zip(&sheet.header) {
        let _written = worksheet.write_string_with_format(0, column, title, &formats.header)?;
    }
    for (row, line) in (1..).zip(&sheet.rows) {
        for (column, datum) in (0..).zip(line) {
            write_datum(worksheet, (row, column), datum, formats)?;
        }
    }
    arrange(worksheet, sheet)
}

/// Автофильтр, закреплённая шапка и ширина колонок.
fn arrange(worksheet: &mut Worksheet, sheet: &Sheet) -> anyhow::Result<()> {
    let last_row = RowNum::try_from(sheet.rows.len()).context("слишком много строк")?;
    let last_column =
        ColNum::try_from(sheet.header.len().saturating_sub(1)).context("слишком много колонок")?;
    let _filtered = worksheet.autofilter(0, 0, last_row, last_column)?;
    let _frozen = worksheet.set_freeze_panes(1, sheet.frozen_columns)?;
    let _fitted = worksheet.set_autofit_max_width(MAX_COLUMN_WIDTH_PX).autofit();
    // Подбор не знает ширины формата даты, и Excel показал бы «#####».
    let moments = sheet.rows.first().into_iter().flat_map(|line| (0..).zip(line));
    for (column, _) in moments.filter(|(_, datum)| matches!(datum, Datum::Moment(_))) {
        let _widened = worksheet.set_column_width(column, MOMENT_WIDTH_CHARS)?;
    }
    Ok(())
}

fn write_datum(
    worksheet: &mut Worksheet,
    (row, column): (RowNum, ColNum),
    datum: &Datum,
    formats: &Formats,
) -> anyhow::Result<()> {
    let _written = match datum {
        Datum::Text(text) => worksheet.write_string(row, column, text)?,
        Datum::Number(number) => worksheet.write_number(row, column, excel_number(*number)?)?,
        Datum::Percent(percent) => {
            let share = percent.checked_div(Decimal::ONE_HUNDRED).context("процент")?;
            worksheet.write_number_with_format(
                row,
                column,
                excel_number(share)?,
                &formats.percent,
            )?
        }
        Datum::Moment(moment) => {
            let date = excel_moment(*moment)?;
            worksheet.write_datetime_with_format(row, column, date, &formats.moment)?
        }
        Datum::Empty => return Ok(()),
    };
    Ok(())
}

fn excel_number(number: Decimal) -> anyhow::Result<f64> {
    number.to_f64().with_context(|| format!("{number} не переводится в число Excel"))
}

/// Момент по Москве как дата Excel: в Excel нет часовых поясов.
fn excel_moment(moment: OffsetDateTime) -> anyhow::Result<ExcelDateTime> {
    let local = msk_local(moment);
    let year = u16::try_from(local.year()).context("год вне диапазона Excel")?;
    Ok(ExcelDateTime::from_ymd(year, u8::from(local.month()), local.day())?.and_hms(
        u16::from(local.hour()),
        local.minute(),
        local.second(),
    )?)
}
