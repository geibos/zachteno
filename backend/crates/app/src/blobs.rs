//! Хранилище картинок (Garage по S3 API, в тестах — память) и их обработка (design D10).

use std::io::Cursor;
use std::sync::Arc;

use anyhow::Context as _;
use bytes::Bytes;
use image::codecs::gif::{GifDecoder, GifEncoder};
use image::{AnimationDecoder as _, ImageDecoder as _, ImageFormat, ImageReader, Limits};
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as StorePath;
use object_store::{GetOptions, ObjectStore, PutOptions, PutPayload};

use crate::config::StorageConfig;

#[cfg(test)]
mod tests;

/// Хранилище объектов.
pub(crate) type Blobs = Arc<dyn ObjectStore>;

/// Наибольший размер загружаемого файла.
pub(crate) const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
/// Наибольшая сторона картинки в пикселях (защита от «бомб»).
const MAX_SIDE: u32 = 8000;
/// Наибольшая сторона кадра анимации.
const MAX_GIF_SIDE: u32 = 2000;
/// Не больше стольких кадров анимации.
const MAX_FRAMES: usize = 300;
/// Суммарный бюджет пикселей на все кадры анимации.
const MAX_GIF_PIXELS: u64 = 100_000_000;

/// S3-клиент для Garage: path-style, http разрешён для внутренней сети.
pub(crate) fn connect(config: &StorageConfig) -> anyhow::Result<Blobs> {
    let store = AmazonS3Builder::new()
        .with_endpoint(&config.endpoint)
        .with_region(&config.region)
        .with_bucket_name(&config.bucket)
        .with_access_key_id(&config.access_key_id)
        .with_secret_access_key(config.secret_access_key.expose())
        .with_allow_http(true)
        .with_virtual_hosted_style_request(false)
        .build()
        .context("настройка S3")?;
    Ok(Arc::new(store))
}

/// Кладёт объект.
///
/// cancel-safe: yes — объект либо записан целиком, либо нет; ключи случайные.
pub(crate) async fn put(blobs: &Blobs, key: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
    let _written = blobs.put_opts(&StorePath::from(key), PutPayload::from(bytes), PutOptions::default()).await?;
    Ok(())
}

/// Читает объект целиком.
///
/// cancel-safe: yes — только чтение.
pub(crate) async fn get(blobs: &Blobs, key: &str) -> anyhow::Result<Bytes> {
    Ok(blobs.get_opts(&StorePath::from(key), GetOptions::default()).await?.bytes().await?)
}

/// Картинка после обработки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Processed {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    pub width: u32,
    pub height: u32,
}

/// Почему файл не принят.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Rejected {
    #[error("файл больше 10 МБ")]
    TooLarge,
    #[error("поддерживаются PNG, JPEG, WebP и GIF")]
    UnsupportedFormat,
    #[error("картинка повреждена или слишком большая по размеру в пикселях")]
    Undecodable,
}

/// Проверяет формат декодированием и перекодирует картинку, удаляя метаданные (EXIF и т. п.).
pub(crate) fn process(upload: &[u8]) -> Result<Processed, Rejected> {
    if upload.len() > MAX_UPLOAD_BYTES {
        return Err(Rejected::TooLarge);
    }
    let format = image::guess_format(upload).map_err(|_| Rejected::UnsupportedFormat)?;
    match format {
        ImageFormat::Gif => reencode_gif(upload),
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP => reencode_still(upload, format),
        _ => Err(Rejected::UnsupportedFormat),
    }
}

fn limits(side: u32) -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(side);
    limits.max_image_height = Some(side);
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits
}

fn reencode_still(upload: &[u8], format: ImageFormat) -> Result<Processed, Rejected> {
    let mut reader = ImageReader::with_format(Cursor::new(upload), format);
    reader.limits(limits(MAX_SIDE));
    let decoded = reader.decode().map_err(|_| Rejected::Undecodable)?;
    let (output, content_type) = match format {
        ImageFormat::Jpeg => (ImageFormat::Jpeg, "image/jpeg"),
        ImageFormat::WebP => (ImageFormat::WebP, "image/webp"),
        _ => (ImageFormat::Png, "image/png"),
    };
    let mut encoded = Cursor::new(Vec::new());
    let normalized = if output == ImageFormat::Jpeg { image::DynamicImage::ImageRgb8(decoded.to_rgb8()) } else { decoded };
    normalized.write_to(&mut encoded, output).map_err(|_| Rejected::Undecodable)?;
    Ok(Processed { content_type, width: normalized.width(), height: normalized.height(), bytes: encoded.into_inner() })
}

fn reencode_gif(upload: &[u8]) -> Result<Processed, Rejected> {
    let mut decoder = GifDecoder::new(Cursor::new(upload)).map_err(|_| Rejected::Undecodable)?;
    decoder.set_limits(limits(MAX_GIF_SIDE)).map_err(|_| Rejected::Undecodable)?;
    let (width, height) = decoder.dimensions();
    let frame_pixels = u64::from(width).saturating_mul(u64::from(height)).max(1);
    let frame_budget = usize::try_from(MAX_GIF_PIXELS.checked_div(frame_pixels).unwrap_or(0)).unwrap_or(0).min(MAX_FRAMES);
    let mut frames = Vec::new();
    for frame in decoder.into_frames() {
        if frames.len() >= frame_budget {
            return Err(Rejected::Undecodable);
        }
        frames.push(frame.map_err(|_| Rejected::Undecodable)?);
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut encoded);
        encoder.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(|_| Rejected::Undecodable)?;
        encoder.encode_frames(frames).map_err(|_| Rejected::Undecodable)?;
    }
    Ok(Processed { bytes: encoded, content_type: "image/gif", width, height })
}
