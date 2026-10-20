//! Images in responses (R-HTTP-17): decoding off the UI thread, and drawing them.

use std::sync::Arc;

use egui::{RichText, ScrollArea, Ui};
use tortuga_core::Response;

use super::ResponseState;
use crate::theme;

/// Longest side of an image as drawn; bigger images are scaled down to it once, when
/// the response arrives (R-HTTP-17).
pub(super) const MAX_IMAGE_SIDE: u32 = 4096;

/// Images with a longer side are not decoded at all, nor more than this many bytes of
/// pixels, so a hostile response cannot exhaust memory (R-HTTP-17).
const MAX_DECODED_SIDE: u32 = 16_384;
const MAX_DECODED_BYTES: u64 = 512 * 1024 * 1024;

/// A decoded image, ready to upload as a texture.
pub struct ImageBody {
    /// Scaled down to at most [`MAX_IMAGE_SIDE`].
    pub(super) pixels: Arc<egui::ColorImage>,
    /// Width and height of the image as sent.
    pub(super) size: [u32; 2],
    /// "PNG", "JPEG", …
    format: &'static str,
}

impl std::fmt::Debug for ImageBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageBody")
            .field("size", &self.size)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

/// The body as an image, if it is one in a format Tortuga can decode (R-HTTP-17).
/// Decoding is slow for big images: call it off the UI thread.
pub(super) fn decode_image(response: &Response) -> Option<ImageBody> {
    let said_image = response
        .content_type()
        .is_some_and(|ct| ct.trim_start().to_ascii_lowercase().starts_with("image/"));
    // The content type can be missing or wrong; the first bytes tell.
    let format = image::guess_format(&response.body).ok()?;
    if !said_image && std::str::from_utf8(&response.body).is_ok() {
        return None;
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(&response.body[..]), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let size = [decoded.width(), decoded.height()];
    let decoded = if size.iter().any(|&side| side > MAX_IMAGE_SIDE) {
        decoded.thumbnail(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE)
    } else {
        decoded
    };
    let rgba = decoded.to_rgba8();
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );
    Some(ImageBody {
        pixels: Arc::new(pixels),
        size,
        format: image_format_name(format),
    })
}

fn image_format_name(format: image::ImageFormat) -> &'static str {
    match format {
        image::ImageFormat::Png => "PNG",
        image::ImageFormat::Jpeg => "JPEG",
        image::ImageFormat::Gif => "GIF",
        image::ImageFormat::WebP => "WebP",
        image::ImageFormat::Bmp => "BMP",
        image::ImageFormat::Ico => "ICO",
        _ => "Image",
    }
}

/// An uploaded image, with the address of the pixels it was made from.
#[derive(Clone)]
pub(super) struct ImageTexture {
    pixels: usize,
    handle: egui::TextureHandle,
}

impl std::fmt::Debug for ImageTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageTexture")
            .field("id", &self.handle.id())
            .finish_non_exhaustive()
    }
}

/// The image, scaled down to fit the panel, never up (R-HTTP-17).
pub(super) fn image_view(ui: &mut Ui, img: &ImageBody, st: &mut ResponseState) {
    let p = theme::palette(ui);
    let [w, h] = img.size;
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} · {w} × {h} px", img.format))
            .small()
            .color(p.muted),
    );
    let max_side = ui.input(|i| i.max_texture_side);
    if img.pixels.size.iter().any(|&side| side > max_side) {
        theme::hint(ui, "Too large to show here. Use “Save to file” to keep it.");
        return;
    }
    let key = Arc::as_ptr(&img.pixels) as usize;
    if st.texture.as_ref().is_none_or(|t| t.pixels != key) {
        let handle = ui.ctx().load_texture(
            "response-image",
            Arc::clone(&img.pixels),
            egui::TextureOptions::LINEAR,
        );
        st.texture = Some(ImageTexture {
            pixels: key,
            handle,
        });
    }
    let Some(texture) = &st.texture else {
        return;
    };
    let texture = egui::load::SizedTexture::from_handle(&texture.handle);
    ScrollArea::both()
        .id_salt("response-image")
        .auto_shrink(false)
        .show(ui, |ui| {
            let room = ui.available_size() - egui::vec2(4.0, 4.0);
            let scale = (room.x / texture.size.x)
                .min(room.y / texture.size.y)
                .clamp(0.01, 1.0);
            let size = texture.size * scale;
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::from_texture(texture).fit_to_exact_size(size))
                    .on_hover_text(format!("{w} × {h} px, shown at {:.0}%", scale * 100.0));
            });
        });
}
