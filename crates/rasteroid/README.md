# rasteroid

A Rust library for displaying images and videos inline in terminal emulators, part of the [mcat](https://github.com/Skardyy/mcat) project.

[![Crates.io](https://img.shields.io/crates/v/rasteroid.svg)](https://crates.io/crates/rasteroid)
[![Documentation](https://docs.rs/rasteroid/badge.svg)](https://docs.rs/rasteroid)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Auto Detection

| Protocol | Terminal Emulators |
|----------|-------------------|
| Kitty    | Kitty, Ghostty    |
| iTerm2   | iTerm2, WezTerm, Mintty, Rio, Warp, Konsole |
| Sixel    | Foot, Windows Terminal, sixel-tmux |

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rasteroid = "0.1.1"
```

## Usage

### Basic Usage

```rust
use std::fs::File;
use std::io::{self, Read};
use rasteroid::{InlineEncoder, inline_an_image};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load an image file
    let mut file = File::open("image.png")?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    
    // Auto-detect terminal and display image
    let encoder = InlineEncoder::auto_detect(false, false, false);
    inline_an_image(&buffer, io::stdout(), None, &encoder)?;
    
    Ok(())
}
```

### Specifying Encoder Type

```rust
use std::io;
use rasteroid::{InlineEncoder, inline_an_image};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load image data
    let mut file = File::open("image.png")?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    
    // Explicitly choose Kitty protocol
    let encoder = InlineEncoder::Kitty;
    
    // Center the image and display it
    let center_offset = Some(10); // 10 columns from left
    inline_an_image(&buffer, io::stdout(), center_offset, &encoder)?;
    
    Ok(())
}
```

### Working with Image Transformations

```rust
use image::io::Reader as ImageReader;
use rasteroid::image_extended::InlineImage;
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load an image
    let img = ImageReader::open("photo.jpg")?.decode()?;
    
    // Resize to 50% of terminal width, auto height
    let (resized_data, center_offset) = img.resize_plus(Some("50%"), None)?;
    
    // Display with auto-detected protocol
    let encoder = mcat_rasteroid::InlineEncoder::auto_detect(false, false, false);
    mcat_rasteroid::inline_an_image(&resized_data, io::stdout(), Some(center_offset), &encoder)?;
    
    Ok(())
}
```

### Zoom and Pan

```rust
use image::io::Reader as ImageReader;
use rasteroid::image_extended::InlineImage;
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load image
    let img = ImageReader::open("large_map.png")?.decode()?;
    
    // Zoom in (level 3) and pan right (+2) and down (+1)
    let zoomed = img.zoom_pan(Some(3), Some(2), Some(1));
    
    // Resize to fit terminal width
    let (resized_data, center_offset) = zoomed.resize_plus(Some("80%"), None, false, false)?;
    
    // Display
    let encoder = mcat_rasteroid::InlineEncoder::auto_detect(false, false, false);
    mcat_rasteroid::inline_an_image(&resized_data, io::stdout(), Some(center_offset), &encoder)?;
    
    Ok(())
}
```

## Dimension Specification

When resizing images, you can specify dimensions in various formats:

- `"800px"` - Absolute pixel size
- `"50%"` - Percentage of terminal size
- `"40c"` - Terminal cell count
- `"800"` - Raw number (interpreted as pixels)

## Video Support

rasteroid supports displaying video frames using the Kitty / Iterm protocols:

the following is how mcat uses it:

#### For Iterm
```rust
fn video_to_gif(input: impl AsRef<str>) -> Result<Vec<u8>, Box<dyn error::Error>> {
    let input = input.as_ref();
    if input.ends_with(".gif") {
        let path = Path::new(input);
        let bytes = fs::read(path)?;
        return Ok(bytes);
    }

    let mut command =
        match fetch_manager::get_ffmpeg() {
            Some(c) => c,
            None => return Err(
                "ffmpeg isn't installed. either install it manually, or call `mcat --fetch-ffmpeg`"
                    .into(),
            ),
        };
    command
        .hwaccel("auto")
        .input(input)
        .format("gif")
        .output("-");

    let mut child = command.spawn()?;
    let mut stdout = child
        .take_stdout()
        .ok_or("failed to get stdout for ffmpeg")?;

    let mut output_bytes = Vec::new();
    stdout.read_to_end(&mut output_bytes)?;

    child.wait()?; // ensure process finishes cleanly

    Ok(output_bytes)
}
```

#### For Kitty:
```rs
fn video_to_frames(
    input: impl AsRef<str>,
) -> Result<Box<dyn Iterator<Item = OutputVideoFrame>>, Box<dyn error::Error>> {
    let input = input.as_ref();
    if !ffmpeg_sidecar::command::ffmpeg_is_installed() {
        eprintln!("ffmpeg isn't installed, installing.. it may take a little");
        ffmpeg_sidecar::download::auto_download()?;
    }

    let mut command =
        match fetch_manager::get_ffmpeg() {
            Some(c) => c,
            None => return Err(
                "ffmpeg isn't installed. either install it manually, or call `mcat --fetch-ffmpeg`"
                    .into(),
            ),
        };
    command.hwaccel("auto").input(input).rawvideo();

    let mut child = command.spawn()?;
    let frames = child.iter()?.filter_frames();

    Ok(Box::new(frames))
}
``` 

#### Finally: 
```rs
// OutputVideoFrame is from ffmpeg-sidecar (you can use whatever suits you, just needs to impl frame)
pub struct KittyFrames(pub OutputVideoFrame);
impl Frame for KittyFrames {
    fn width(&self) -> u16 {
        self.0.width as u16
    }
    fn height(&self) -> u16 {
        self.0.height as u16
    }
    fn timestamp(&self) -> f32 {
        self.0.timestamp
    }
    fn data(&self) -> &[u8] {
        &self.0.data
    }
}

pub fn inline_a_video(
    input: impl AsRef<str>,
    out: &mut impl Write,
    inline_encoder: &rasteroid::InlineEncoder,
    center: bool,
) -> Result<(), Box<dyn error::Error>> {
    match inline_encoder {
        rasteroid::InlineEncoder::Kitty => {
            let frames = video_to_frames(input)?;
            let mut kitty_frames = frames.map(KittyFrames);
            let id = rand::random::<u32>();
            rasteroid::kitty_encoder::encode_frames(&mut kitty_frames, out, id, center)?;
            Ok(())
        }
        rasteroid::InlineEncoder::Iterm => {
            let gif = video_to_gif(input)?;
            let dyn_img = image::load_from_memory_with_format(&gif, image::ImageFormat::Gif)?;
            let offset = match center {
                true => Some(rasteroid::term_misc::center_image(dyn_img.width() as u16)),
                false => None,
            };
            rasteroid::iterm_encoder::encode_image(&gif, out, offset)?;
            Ok(())
        }
        rasteroid::InlineEncoder::Sixel => Err("Cannot view videos in sixel".into()),
    }
}
```

## Terminal Size Utilities

rasteroid provides utilities for working with terminal dimensions:

```rust
use rasteroid::term_misc::{init_winsize, break_size_string, get_winsize, SizeDirection, dim_to_px};

fn main() {
    // Initialize with fallback values
    let spx = break_size_string("1920x1080").unwrap();
    let sc = break_size_string("100x30").unwrap();
    let scalex = None;
    let scaley = None;
    let is_tmux = false; // encodes image and videos for tmux
    let needs_inline = false; // makes image and videos in kitty use unicode placeholder see https://sw.kovidgoyal.net/kitty/graphics-protocol/#unicode-placeholders
    init_winsize(&spx, &sc, scalex, scaley, is_tmux, needs_inline).unwrap();
    
    // Get window size
    let winsize = get_winsize();
    println!("Terminal is {} columns by {} rows", winsize.sc_width, winsize.sc_height);
    println!("Terminal is {} pixels by {} pixels", winsize.spx_width, winsize.spx_height);
    
    // Convert dimensions
    let width_px = dim_to_px("50%", SizeDirection::Width).unwrap();
    println!("50% of terminal width is {} pixels", width_px);
}
```

## License

This project is licensed under the MIT License - see the LICENSE under mcat for details.
