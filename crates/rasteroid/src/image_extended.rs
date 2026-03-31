use fast_image_resize::{IntoImageView, Resizer, images::Image};
use image::{
    DynamicImage, GenericImage, GenericImageView, ImageEncoder,
    codecs::pnm::{PnmEncoder, PnmSubtype},
};

use crate::{
    error::RasterError,
    term_misc::{self, Wininfo},
};

pub trait InlineImage {
    /// Resizes an image using logical size units (%, cells, pixels).
    ///
    /// # Arguments
    /// * `wininfo` - Terminal info used to resolve percentage and cell based sizes
    /// * `width` - Target width: `%` (percentage of terminal), `c` (cells), or plain number (pixels). `None` derives from height keeping aspect ratio
    /// * `height` - Target height, same format as `width`. `None` derives from width keeping aspect ratio
    /// * `resize_for_ascii` - If `true`, works in cell dimensions instead of pixels. Cell height is doubled to account for half block rendering
    /// * `pad` - If `true`, pads with empty pixels to fill the exact requested size while keeping aspect ratio. If `false`, result may be smaller than requested
    ///
    /// ```
    /// use std::path::Path;
    /// use rasteroid::image_extended::InlineImage;
    /// use rasteroid::term_misc::{EnvIdentifiers, Wininfo};
    ///
    /// let path = Path::new("image.png");
    /// let buf = match std::fs::read(path) {
    ///     Ok(buf) => buf,
    ///     Err(e) => return,
    /// };
    /// let env = EnvIdentifiers::new();
    /// let wininfo = Wininfo::new(None, None, None, None, &env).unwrap();
    /// let dyn_img = image::load_from_memory(&buf).unwrap();
    /// let img = dyn_img.resize_plus(&wininfo, Some("80%"), Some("200c"), false, false).unwrap();
    /// ```
    fn resize_plus(
        &self,
        wininfo: &Wininfo,
        width: Option<&str>,
        height: Option<&str>,
        resize_for_ascii: bool,
        pad: bool,
    ) -> Result<DynamicImage, RasterError>;
}

impl InlineImage for DynamicImage {
    fn resize_plus(
        &self,
        wininfo: &Wininfo,
        width: Option<&str>,
        height: Option<&str>,
        resize_for_ascii: bool,
        pad: bool,
    ) -> Result<DynamicImage, RasterError> {
        let (src_width, src_height) = self.dimensions();
        let width = match width {
            Some(w) => match resize_for_ascii {
                true => wininfo.dim_to_cells(w, term_misc::SizeDirection::Width)?,
                false => wininfo.dim_to_px(w, term_misc::SizeDirection::Width)?,
            },
            None => src_width,
        };
        let height = match height {
            Some(h) => match resize_for_ascii {
                true => wininfo.dim_to_cells(h, term_misc::SizeDirection::Height)? * 2,
                false => wininfo.dim_to_px(h, term_misc::SizeDirection::Height)?,
            },
            None => src_height,
        };

        let (new_width, new_height) = calc_fit(src_width, src_height, width, height);

        let mut dst_image = Image::new(
            new_width,
            new_height,
            self.pixel_type().ok_or(RasterError::InvalidImage)?,
        );
        let mut resizer = Resizer::new();
        resizer.resize(self, &mut dst_image, None)?;

        let mut buf = Vec::new();
        PnmEncoder::new(&mut buf)
            .with_subtype(PnmSubtype::ArbitraryMap)
            .write_image(
                dst_image.buffer(),
                dst_image.width(),
                dst_image.height(),
                self.color().into(),
            )?;
        let resized = image::load_from_memory(&buf)?;

        if pad && (new_width != width || new_height != height) {
            let mut new_img = DynamicImage::new_rgba8(width, height);
            let x_offset = if width == new_width {
                0
            } else {
                (width - new_width) / 2
            };
            let y_offset = if height == new_height {
                0
            } else {
                (height - new_height) / 2
            };
            new_img.copy_from(&resized, x_offset, y_offset)?;
            return Ok(new_img);
        }

        Ok(resized)
    }
}

/// Handles zoom and pan state for viewing an image inside a fixed container.
///
/// Tracks container size, image size, zoom level, and pan offset.
/// Call [`Self::get_viewport`] to get the crop region for the current state.
#[derive(Debug, Clone)]
pub struct ZoomPanViewport {
    container_width: u32,
    container_height: u32,
    image_width: u32,
    image_height: u32,
    zoom: usize,
    pan_x: i32,
    pan_y: i32,
}

/// Crop region produced by [`ZoomPanViewport::get_viewport`].
///
/// All values are in source image pixels.
/// `scale_factor` is the combined base + zoom scale used to produce this region.
#[derive(Debug, Clone)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

impl ZoomPanViewport {
    pub fn new(
        container_width: u32,
        container_height: u32,
        image_width: u32,
        image_height: u32,
    ) -> Self {
        Self {
            container_width,
            container_height,
            image_width,
            image_height,
            zoom: 1,
            pan_x: 0,
            pan_y: 0,
        }
    }

    /// Sets the zoom level, minimum value is 1.
    ///
    /// Pan is clamped automatically after setting zoom,
    /// so [`Self::get_viewport`] can be called immediately after.
    pub fn set_zoom(&mut self, zoom: usize) {
        self.zoom = zoom.max(1);
        self.clamp_pan();
    }

    /// Sets the pan position directly.
    ///
    /// Pan is clamped to valid limits automatically after setting,
    /// so [`Self::get_viewport`] can be called immediately after.
    pub fn set_pan(&mut self, pan_x: i32, pan_y: i32) {
        self.pan_x = pan_x;
        self.pan_y = pan_y;
        self.clamp_pan();
    }

    /// Shifts the pan by the given delta, clamped to valid limits.
    ///
    /// Returns `true` if either axis changed, `false` if both would go out of bounds.
    pub fn adjust_pan(&mut self, delta_x: i32, delta_y: i32) -> bool {
        let mut modified = false;
        let (x1, x2, y1, y2) = self.get_pan_limits();
        let new_pan_x = self.pan_x() + delta_x;
        if new_pan_x >= x1 && new_pan_x <= x2 && delta_x != 0 {
            self.pan_x = new_pan_x;
            modified = true;
        }
        let new_pan_y = self.pan_y() + delta_y;
        if new_pan_y >= y1 && new_pan_y <= y2 && delta_y != 0 {
            self.pan_y = new_pan_y;
            modified = true;
        }
        if modified {
            self.clamp_pan();
            return true;
        }
        false
    }

    /// Crops the image to the region defined by the current viewport.
    pub fn apply_to_image(&self, img: &DynamicImage) -> DynamicImage {
        let viewport = self.get_viewport();
        img.crop_imm(viewport.x, viewport.y, viewport.width, viewport.height)
    }

    /// Returns the source image region that should be visible given the current zoom and pan.
    pub fn get_viewport(&self) -> Viewport {
        let scale_x = self.container_width as f32 / self.image_width as f32;
        let scale_y = self.container_height as f32 / self.image_height as f32;
        let base_scale = scale_x.min(scale_y);
        let scale_factor = base_scale * self.zoom as f32;

        let viewport_width = (self.container_width as f32 / scale_factor).round() as u32;
        let viewport_height = (self.container_height as f32 / scale_factor).round() as u32;
        let viewport_width = viewport_width.min(self.image_width);
        let viewport_height = viewport_height.min(self.image_height);

        let center_x = (self.image_width as f32 / 2.0) + self.pan_x as f32;
        let center_y = (self.image_height as f32 / 2.0) + self.pan_y as f32;

        let x_f32 = center_x - viewport_width as f32 / 2.0;
        let y_f32 = center_y - viewport_height as f32 / 2.0;

        let x = x_f32
            .max(0.0)
            .min((self.image_width - viewport_width) as f32) as u32;
        let y = y_f32
            .max(0.0)
            .min((self.image_height - viewport_height) as f32) as u32;

        Viewport {
            x,
            y,
            width: viewport_width,
            height: viewport_height,
            scale_factor,
        }
    }

    /// Returns `(min_pan_x, max_pan_x, min_pan_y, max_pan_y)` for the current zoom level.
    pub fn get_pan_limits(&self) -> (i32, i32, i32, i32) {
        let scale_x = self.container_width as f32 / self.image_width as f32;
        let scale_y = self.container_height as f32 / self.image_height as f32;
        let base_scale = scale_x.min(scale_y);
        let scale_factor = base_scale * self.zoom as f32;

        let viewport_width = (self.container_width as f32 / scale_factor).round() as u32;
        let viewport_height = (self.container_height as f32 / scale_factor).round() as u32;
        let viewport_width = viewport_width.min(self.image_width);
        let viewport_height = viewport_height.min(self.image_height);

        let max_pan_x = if viewport_width >= self.image_width {
            0
        } else {
            ((self.image_width - viewport_width) as f32 / 2.0) as i32
        };

        let max_pan_y = if viewport_height >= self.image_height {
            0
        } else {
            ((self.image_height - viewport_height) as f32 / 2.0) as i32
        };

        (-max_pan_x, max_pan_x, -max_pan_y, max_pan_y)
    }

    fn clamp_pan(&mut self) {
        let scale_x = self.container_width as f32 / self.image_width as f32;
        let scale_y = self.container_height as f32 / self.image_height as f32;
        let base_scale = scale_x.min(scale_y);
        let scale_factor = base_scale * self.zoom as f32;

        let viewport_width = (self.container_width as f32 / scale_factor) as u32;
        let viewport_height = (self.container_height as f32 / scale_factor) as u32;

        let max_pan_x = (self.image_width - viewport_width) as f32 / 2.0;
        let max_pan_y = (self.image_height - viewport_height) as f32 / 2.0;

        if viewport_width >= self.image_width {
            self.pan_x = 0;
        } else {
            self.pan_x = self.pan_x.max(-(max_pan_x as i32)).min(max_pan_x as i32);
        }

        if viewport_height >= self.image_height {
            self.pan_y = 0;
        } else {
            self.pan_y = self.pan_y.max(-(max_pan_y as i32)).min(max_pan_y as i32);
        }
    }

    /// get the current zoom level
    pub fn zoom(&self) -> usize {
        self.zoom
    }

    /// get the current pan x
    pub fn pan_x(&self) -> i32 {
        self.pan_x
    }

    /// get the current pan y
    pub fn pan_y(&self) -> i32 {
        self.pan_y
    }

    /// get the current container size
    pub fn container_size(&self) -> (u32, u32) {
        (self.container_width, self.container_height)
    }

    /// get the current image size
    pub fn image_size(&self) -> (u32, u32) {
        (self.image_width, self.image_height)
    }

    /// Update container size
    pub fn update_container_size(&mut self, width: u32, height: u32) {
        self.container_width = width;
        self.container_height = height;
        self.clamp_pan();
    }

    /// Update image size
    pub fn update_image_size(&mut self, width: u32, height: u32) {
        self.image_width = width;
        self.image_height = height;
        self.clamp_pan();
    }
}

/// Returns the largest `(width, height)` that fits inside `dst_width x dst_height`
/// while keeping the aspect ratio of `src_width x src_height`.
///
/// ```
/// use rasteroid::image_extended::calc_fit;
///
/// // wide image into a square box - constrained by width
/// let (w, h) = calc_fit(1920, 1080, 800, 800);
/// assert_eq!(w, 800);
/// assert!(h < 800);
///
/// // tall image into a square box - constrained by height
/// let (w, h) = calc_fit(1080, 1920, 800, 800);
/// assert!(w < 800);
/// assert_eq!(h, 800);
/// ```
pub fn calc_fit(src_width: u32, src_height: u32, dst_width: u32, dst_height: u32) -> (u32, u32) {
    let src_ar = src_width as f32 / src_height as f32;
    let dst_ar = dst_width as f32 / dst_height as f32;

    if src_ar > dst_ar {
        // Image is wider than target: scale by width
        let scaled_height = (dst_width as f32 / src_ar).round() as u32;
        (dst_width, scaled_height)
    } else {
        // Image is taller than target: scale by height
        let scaled_width = (dst_height as f32 * src_ar).round() as u32;
        (scaled_width, dst_height)
    }
}
