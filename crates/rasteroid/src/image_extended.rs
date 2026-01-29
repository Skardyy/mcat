use std::{error, io::Cursor};

use fast_image_resize::{IntoImageView, Resizer, images::Image};
use image::{DynamicImage, GenericImage, GenericImageView, ImageEncoder, codecs::png::PngEncoder};

use crate::term_misc::dim_to_cells;

use super::term_misc::{self, dim_to_px};

pub trait InlineImage {
    /// fast image resizer, that takes logic units.
    /// # example:
    /// ```
    /// use std::path::Path;
    /// use rasteroid::image_extended::InlineImage;
    ///
    /// let path = Path::new("image.png");
    /// let buf = match std::fs::read(path) {
    ///     Ok(buf) => buf,
    ///     Err(e) => return,
    /// };
    /// let dyn_img = image::load_from_memory(&buf).unwrap();
    /// let (img_data, offset, width, height) = dyn_img.resize_plus(Some("80%"),Some("200c"), false, false).unwrap();
    /// ```
    /// * the offset is for centering the image
    /// * it accepts either `%` (percentage) / `c` (cells) / just a number
    /// * when resize for ascii is true it resizes to cells, if not it resizes to pixels
    /// * pad adds empty pixels so the image will be the exact dimensions specified, while still
    /// maintaining aspect ratio
    fn resize_plus(
        &self,
        width: Option<&str>,
        height: Option<&str>,
        resize_for_ascii: bool,
        pad: bool,
    ) -> Result<(Vec<u8>, u16, u32, u32), Box<dyn error::Error>>;
}

impl InlineImage for DynamicImage {
    fn resize_plus(
        &self,
        width: Option<&str>,
        height: Option<&str>,
        resize_for_ascii: bool,
        pad: bool,
    ) -> Result<(Vec<u8>, u16, u32, u32), Box<dyn error::Error>> {
        let (src_width, src_height) = self.dimensions();
        let width = match width {
            Some(w) => match resize_for_ascii {
                true => dim_to_cells(w, term_misc::SizeDirection::Width)?,
                false => dim_to_px(w, term_misc::SizeDirection::Width)?,
            },
            None => src_width,
        };
        let height = match height {
            Some(h) => match resize_for_ascii {
                true => dim_to_cells(h, term_misc::SizeDirection::Height)? * 2,
                false => dim_to_px(h, term_misc::SizeDirection::Height)?,
            },
            None => src_height,
        };

        let (new_width, new_height) = calc_fit(src_width, src_height, width, height);
        let center = term_misc::center_image(new_width as u16, resize_for_ascii);

        let mut dst_image = Image::new(
            new_width,
            new_height,
            self.pixel_type().ok_or("image is invalid")?,
        );
        let mut resizer = Resizer::new();
        resizer.resize(self, &mut dst_image, None)?;

        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        let encoder = PngEncoder::new(&mut cursor);
        encoder.write_image(
            dst_image.buffer(),
            dst_image.width(),
            dst_image.height(),
            self.color().into(),
        )?;

        if pad && (new_width != width || new_height != height) {
            let img = image::load_from_memory(&buffer)?;
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
            new_img.copy_from(&img, x_offset, y_offset)?;
            let mut cursor = Cursor::new(Vec::new());
            new_img.write_to(&mut cursor, image::ImageFormat::Png)?;
            return Ok((cursor.into_inner(), center, width, height));
        }

        Ok((buffer, center, new_width, new_height))
    }
}

/// Viewport for zooming and moving inside an image.
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

/// Viewport calculated from ZoomPanViewport
/// `x` - the offset from the left
/// `y` - the offset from the top
/// `width` - how many pixels to the right to take
/// `height` - how many pixels down to take
/// `scale_factor` - metadata from the ZoomPanViewport
#[derive(Debug, Clone)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

impl ZoomPanViewport {
    /// container size would be the desired size you're trying to make the viewport fit
    /// the image size would be the original image size
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

    /// won't go below 1.
    /// clamps right after, so you can call `get_viewport`
    pub fn set_zoom(&mut self, zoom: usize) {
        self.zoom = zoom.max(1); // Ensure zoom is at least 1
        self.clamp_pan();
    }

    /// clamps right after, so you can call `get_viewport`
    pub fn set_pan(&mut self, pan_x: i32, pan_y: i32) {
        self.pan_x = pan_x;
        self.pan_y = pan_y;
        self.clamp_pan();
    }

    /// adds the delta to the current pan, will not modify the pan if the final result won't be
    /// within the limits.
    ///
    /// `returns` bool indicating if the pan was modified
    ///
    /// clamps right after, so you can call `get_viewport`
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

    /// crops the image using the viewport
    pub fn apply_to_image(&self, img: &DynamicImage) -> DynamicImage {
        let viewport = self.get_viewport();
        img.crop_imm(viewport.x, viewport.y, viewport.width, viewport.height)
    }

    /// calculates and returns a `Viewport`
    pub fn get_viewport(&self) -> Viewport {
        // Calculate the base scale to fit the image in the container
        let scale_x = self.container_width as f32 / self.image_width as f32;
        let scale_y = self.container_height as f32 / self.image_height as f32;
        let base_scale = scale_x.min(scale_y);
        let scale_factor = base_scale * self.zoom as f32;

        // Calculate the viewport size in image coordinates
        let viewport_width = (self.container_width as f32 / scale_factor).round() as u32;
        let viewport_height = (self.container_height as f32 / scale_factor).round() as u32;

        // Ensure viewport doesn't exceed image dimensions
        let viewport_width = viewport_width.min(self.image_width);
        let viewport_height = viewport_height.min(self.image_height);

        // Calculate the center position with pan offset
        let center_x = (self.image_width as f32 / 2.0) + self.pan_x as f32;
        let center_y = (self.image_height as f32 / 2.0) + self.pan_y as f32;

        // Calculate viewport position (top-left corner)
        let x_f32 = center_x - viewport_width as f32 / 2.0;
        let y_f32 = center_y - viewport_height as f32 / 2.0;

        // Clamp to image boundaries and ensure we don't go negative
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

    /// `returns` the limit to which pan will be applied.
    /// values exceeding those retruned here won't modify the Viewport when calculating
    pub fn get_pan_limits(&self) -> (i32, i32, i32, i32) {
        // Calculate current viewport to determine pan limits
        let scale_x = self.container_width as f32 / self.image_width as f32;
        let scale_y = self.container_height as f32 / self.image_height as f32;
        let base_scale = scale_x.min(scale_y);
        let scale_factor = base_scale * self.zoom as f32;

        let viewport_width = (self.container_width as f32 / scale_factor).round() as u32;
        let viewport_height = (self.container_height as f32 / scale_factor).round() as u32;

        // Ensure viewport doesn't exceed image dimensions
        let viewport_width = viewport_width.min(self.image_width);
        let viewport_height = viewport_height.min(self.image_height);

        // Calculate maximum pan distances
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
        // Calculate current viewport to determine pan limits
        let scale_x = self.container_width as f32 / self.image_width as f32;
        let scale_y = self.container_height as f32 / self.image_height as f32;
        let base_scale = scale_x.min(scale_y);
        let scale_factor = base_scale * self.zoom as f32;

        let viewport_width = (self.container_width as f32 / scale_factor) as u32;
        let viewport_height = (self.container_height as f32 / scale_factor) as u32;

        // Calculate maximum pan distances
        let max_pan_x = (self.image_width - viewport_width) as f32 / 2.0;
        let max_pan_y = (self.image_height - viewport_height) as f32 / 2.0;

        // If the viewport is larger than the image, don't allow panning
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

/// calculates the dimensions of an image into fit bounding box
/// # example:
/// ```
/// use rasteroid::image_extended::calc_fit;
///
/// let (new_width, new_height) = calc_fit(1920, 1080, 800, 400);
/// ```
/// the above will return dimensions close to 800x400 that maintain the aspect ratio of 1920x1080
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
