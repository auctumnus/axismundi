use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use std::io::Cursor;
use webp_animation::Encoder as WebpAnimEncoder;

use crate::err::{AppResult, internal_error};

/// converts an animated (or static) gif to an animated webp.
/// returns the encoded webp bytes.
pub(super) fn convert_gif_to_animated_webp(data: &[u8]) -> AppResult<Vec<u8>> {
    let decoder = GifDecoder::new(Cursor::new(data))
        .map_err(|e| internal_error(format!("failed to decode gif: {e}")))?;

    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|e| internal_error(format!("failed to collect gif frames: {e}")))?;

    if frames.is_empty() {
        return Err(internal_error("gif has no frames"));
    }

    let (width, height) = frames[0].buffer().dimensions();

    let mut encoder = WebpAnimEncoder::new((width, height))
        .map_err(|e| internal_error(format!("failed to create webp encoder: {e}")))?;

    let mut timestamp_ms: i32 = 0;

    for frame in &frames {
        let (numer, denom) = frame.delay().numer_denom_ms();
        let frame_duration_ms = if denom == 0 { 100 } else { (numer / denom) as i32 };

        encoder
            .add_frame(frame.buffer().as_raw(), timestamp_ms)
            .map_err(|e| internal_error(format!("failed to add webp frame: {e}")))?;

        timestamp_ms += frame_duration_ms;
    }

    let webp_data = encoder
        .finalize(timestamp_ms)
        .map_err(|e| internal_error(format!("failed to finalize webp: {e}")))?;

    Ok(webp_data.to_vec())
}
