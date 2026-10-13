//! Хранилище картинок (Garage по S3 API, в тестах — память) и их обработка (design D10).

use std::fmt::Display;
use std::io::Cursor;
use std::sync::Arc;

use anyhow::Context as _;
use bytes::Bytes;
use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::{
    AnimationDecoder as _, DynamicImage, Frame, ImageDecoder as _, ImageFormat, ImageReader, Limits,
};
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
const MAX_SIDE: u32 = 4096;
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
    let _written = blobs
        .put_opts(&StorePath::from(key), PutPayload::from(bytes), PutOptions::default())
        .await?;
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

/// Формат не распознан; причина уходит в журнал.
fn unsupported(error: impl Display) -> Rejected {
    tracing::info!(%error, "формат загруженного файла не распознан");
    Rejected::UnsupportedFormat
}

/// Картинку не удалось декодировать или перекодировать; причина уходит в журнал.
fn undecodable(error: impl Display) -> Rejected {
    tracing::info!(%error, "загруженная картинка не обработана");
    Rejected::Undecodable
}

/// Проверяет формат декодированием и перекодирует картинку, удаляя метаданные (EXIF и т. п.).
pub(crate) fn process(upload: &[u8]) -> Result<Processed, Rejected> {
    if upload.len() > MAX_UPLOAD_BYTES {
        return Err(Rejected::TooLarge);
    }
    let format = image::guess_format(upload).map_err(unsupported)?;
    if format == ImageFormat::Gif {
        reencode_gif(upload)
    } else if matches!(format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP) {
        reencode_still(upload, format)
    } else {
        Err(Rejected::UnsupportedFormat)
    }
}

fn limits(side: u32) -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(side);
    limits.max_image_height = Some(side);
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits
}

/// Во что перекодировать: JPEG и WebP остаются собой, остальное — PNG.
fn output_of(format: ImageFormat) -> (ImageFormat, &'static str) {
    if format == ImageFormat::Jpeg {
        (ImageFormat::Jpeg, "image/jpeg")
    } else if format == ImageFormat::WebP {
        (ImageFormat::WebP, "image/webp")
    } else {
        (ImageFormat::Png, "image/png")
    }
}

fn reencode_still(upload: &[u8], format: ImageFormat) -> Result<Processed, Rejected> {
    let mut reader = ImageReader::with_format(Cursor::new(upload), format);
    reader.limits(limits(MAX_SIDE));
    let decoded = reader.decode().map_err(undecodable)?;
    let (output, content_type) = output_of(format);
    let mut encoded = Cursor::new(Vec::new());
    let normalized = if output == ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(decoded.to_rgb8())
    } else {
        decoded
    };
    normalized.write_to(&mut encoded, output).map_err(undecodable)?;
    Ok(Processed {
        bytes: encoded.into_inner(),
        content_type,
        width: normalized.width(),
        height: normalized.height(),
    })
}

fn reencode_gif(upload: &[u8]) -> Result<Processed, Rejected> {
    let mut decoder = GifDecoder::new(Cursor::new(upload)).map_err(undecodable)?;
    decoder.set_limits(limits(MAX_GIF_SIDE)).map_err(undecodable)?;
    let (width, height) = decoder.dimensions();
    let budget = frame_budget(width, height);
    let mut frames = Vec::new();
    for frame in decoder.into_frames() {
        if frames.len() >= budget {
            return Err(Rejected::Undecodable);
        }
        frames.push(frame.map_err(undecodable)?);
    }
    Ok(Processed { bytes: encode_gif(frames)?, content_type: "image/gif", width, height })
}

/// Сколько кадров такого размера укладывается в бюджет пикселей и лимит кадров.
fn frame_budget(width: u32, height: u32) -> usize {
    let frame_pixels = u64::from(width).saturating_mul(u64::from(height)).max(1);
    usize::try_from(MAX_GIF_PIXELS.checked_div(frame_pixels).unwrap_or(0))
        .unwrap_or(0)
        .min(MAX_FRAMES)
}

/// Закодированная анимация или причина отказа.
type Encoded = Result<Vec<u8>, Rejected>;

fn encode_gif(frames: Vec<Frame>) -> Encoded {
    let mut encoded = Vec::new();
    {
        let mut gif = GifEncoder::new(&mut encoded);
        gif.set_repeat(Repeat::Infinite).map_err(undecodable)?;
        gif.encode_frames(frames).map_err(undecodable)?;
    }
    Ok(encoded)
}
