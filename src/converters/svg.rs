use std::io::Read;

use image::{DynamicImage, ImageBuffer, Rgba};
use resvg::{
    tiny_skia,
    usvg::{self, Options, Tree},
};

pub fn load_svg<R>(mut reader: R) -> Result<DynamicImage, Box<dyn std::error::Error>>
where
    R: Read,
{
    let mut svg_data = Vec::new();
    reader.read_to_end(&mut svg_data)?;

    // Create options for parsing SVG
    let mut opt = Options::default();

    // allowing text
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    opt.fontdb = std::sync::Arc::new(fontdb);
    opt.text_rendering = usvg::TextRendering::OptimizeLegibility;

    // Parse SVG
    let tree = Tree::from_data(&svg_data, &opt)?;

    // Get size of the SVG
    let pixmap_size = tree.size();
    let width = pixmap_size.width();
    let height = pixmap_size.height();

    // Create a Pixmap to render to
    let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32)
        .ok_or("Failed to create pixmap for svg")?;

    // Render SVG to Pixmap
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    // Convert Pixmap to ImageBuffer
    let image_buffer =
        ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, pixmap.data().to_vec())
            .ok_or("Failed to create image buffer for svg")?;

    // Convert ImageBuffer to DynamicImage
    Ok(DynamicImage::ImageRgba8(image_buffer))
}
