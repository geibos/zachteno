use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbImage};

use super::{Rejected, process};

fn jpeg_with_exif() -> Vec<u8> {
    let mut plain = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::new(16, 8)).write_to(&mut plain, ImageFormat::Jpeg).unwrap();
    let bytes = plain.into_inner();
    let exif = b"\xFF\xE1\x00\x16Exif\x00\x00GPS-SECRET-LOCATION";
    let mut with_exif = bytes[..2].to_vec();
    with_exif.extend_from_slice(exif);
    with_exif.extend_from_slice(&bytes[2..]);
    with_exif
}

#[test]
fn exif_is_stripped_from_jpeg() {
    let upload = jpeg_with_exif();
    assert!(upload.windows(10).any(|window| window == b"GPS-SECRET"), "исходник с EXIF");
    let processed = process(&upload).unwrap();
    assert!(
        !processed.bytes.windows(10).any(|window| window == b"GPS-SECRET"),
        "метаданные удалены"
    );
    assert_eq!(
        (processed.width, processed.height, processed.content_type),
        (16, 8, "image/jpeg"),
        "размер"
    );
}

#[test]
fn oversized_and_unsupported_files_are_rejected() {
    assert_eq!(process(&vec![0; 10 * 1024 * 1024 + 1]), Err(Rejected::TooLarge), "больше 10 МБ");
    assert_eq!(process(b"%PDF-1.7 not an image"), Err(Rejected::UnsupportedFormat), "не картинка");
}

#[test]
fn png_is_reencoded_as_png() {
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::new(4, 4)).write_to(&mut png, ImageFormat::Png).unwrap();
    assert_eq!(process(&png.into_inner()).unwrap().content_type, "image/png", "PNG остаётся PNG");
}
