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
    /// Fast image resizer that accepts logical size units.
    ///
    /// # Arguments
    /// * `wininfo` - Terminal info used to resolve percentage and cell based sizes
    /// * `width` - Target width, accepts `%` (percentage of terminal), `c` (cells), or a plain number (pixels). `None` to derive from height while preserving aspect ratio
    /// * `height` - Target height, same format as `width`. `None` to derive from width while preserving aspect ratio
    /// * `resize_for_ascii` - When `true`, resizes to cell dimensions instead of pixels. Note: cell height is doubled internally to account for half block character rendering
    /// * `pad` - When `true`, pads the image with empty pixels to reach the exact requested dimensions while maintaining aspect ratio. When `false`, the returned dimensions may be smaller than requested
    ///
    /// # Returns
    /// A tuple of `(image_bytes, width, height)` where:
    /// * `image_bytes` - png encoded resized image
    /// * `width` - final image width in pixels or cells depending on `resize_for_ascii`
    /// * `height` - final image height in pixels or cells depending on `resize_for_ascii`
    ///
    /// # Examples
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

/// Viewport for zooming and panning inside an image.
///
/// Tracks the container size, image size, zoom level, and pan position,
/// and calculates the correct crop region via `get_viewport`.
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

/// A crop region calculated from a `ZoomPanViewport`.
///
/// # Fields
///
/// * `x` - Offset from the left edge of the image in pixels
/// * `y` - Offset from the top edge of the image in pixels
/// * `width` - Number of pixels to take horizontally
/// * `height` - Number of pixels to take vertically
/// * `scale_factor` - The combined base and zoom scale factor used to produce this viewport
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
    /// so `get_viewport` can be called immediately after.
    pub fn set_zoom(&mut self, zoom: usize) {
        self.zoom = zoom.max(1);
        self.clamp_pan();
    }

    /// Sets the pan position directly.
    ///
    /// Pan is clamped to valid limits automatically after setting,
    /// so `get_viewport` can be called immediately after.
    pub fn set_pan(&mut self, pan_x: i32, pan_y: i32) {
        self.pan_x = pan_x;
        self.pan_y = pan_y;
        self.clamp_pan();
    }

    /// Adds a delta to the current pan position, only if the result stays within the pan limits.
    ///
    /// Pan is clamped automatically after adjusting,
    /// so `get_viewport` can be called immediately after.
    ///
    /// # Arguments
    /// * `delta_x` - Horizontal pan delta in image pixels, negative moves left, positive moves right
    /// * `delta_y` - Vertical pan delta in image pixels, negative moves up, positive moves down
    ///
    /// # Returns
    /// `true` if either axis was modified, `false` if the delta would exceed the pan limits on both axes
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

    /// Calculates and returns the current `Viewport` based on container size, image size, zoom, and pan.
    ///
    /// The viewport represents the region of the source image that should be displayed,
    /// scaled to fit the container while respecting the current zoom and pan offsets.
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

    /// Returns the valid pan range for the current zoom level.
    ///
    /// Pan values outside this range will have no effect on the viewport.
    ///
    /// # Returns
    /// A tuple of `(min_pan_x, max_pan_x, min_pan_y, max_pan_y)` in image pixels
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

/// Calculates the largest dimensions that fit within a bounding box while preserving aspect ratio.
///
/// # Arguments
/// * `src_width` - Original image width in pixels
/// * `src_height` - Original image height in pixels
/// * `dst_width` - Bounding box width in pixels
/// * `dst_height` - Bounding box height in pixels
///
/// # Returns
/// A `(width, height)` tuple that fits within the bounding box while maintaining the source aspect ratio
///
/// # Examples
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
