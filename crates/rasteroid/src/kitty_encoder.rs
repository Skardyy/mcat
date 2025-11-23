use std::{cmp::min, collections::HashMap, error::Error, io::Write, sync::atomic::Ordering};

use base64::{Engine, engine::general_purpose};
use image::GenericImageView;
use shared_memory::ShmemConf;

use crate::{
    Frame,
    term_misc::{self, EnvIdentifiers, image_to_base64, loc_to_terminal, offset_to_terminal},
};

fn transmit_shm(
    data: &[u8],
    mut out: impl Write,
    opts: HashMap<String, String>,
    shm_name: &str,
    tmux: bool,
) -> Result<(), Box<dyn Error>> {
    let mut opts_string = String::with_capacity(opts.len() * 8);
    for (key, value) in opts {
        if !opts_string.is_empty() {
            opts_string.push(',');
        }
        opts_string.push_str(&format!("{key}={value}"));
    }
    let s = data.len();
    opts_string.push_str(&format!(",q=2,t=s,S={s}"));

    let mut shmem = ShmemConf::new().size(s).os_id(shm_name).create()?;
    let shmem_slice = unsafe { shmem.as_slice_mut() };
    shmem_slice[..data.len()].copy_from_slice(&data);
    let shm_name = general_purpose::STANDARD.encode(shm_name);

    let prefix = if tmux {
        "\x1bPtmux;\x1b\x1b_G"
    } else {
        "\x1b_G"
    };
    let suffix = if tmux { "\x1b\x1b\\\x1b\\" } else { "\x1b\\" };

    write!(out, "{prefix}{opts_string};{shm_name}{suffix}")?;

    // will clean the shm if not leaked..
    std::mem::forget(shmem);
    Ok(())
}

fn chunk_base64(
    base64: &str,
    out: &mut impl Write,
    size: usize,
    first_opts: HashMap<String, String>,
    sub_opts: HashMap<String, String>,
    tmux: bool,
) -> Result<(), std::io::Error> {
    // first block
    let mut first_opts_string = String::with_capacity(first_opts.len() * 8);
    for (key, value) in first_opts {
        if !first_opts_string.is_empty() {
            first_opts_string.push(',');
        }
        first_opts_string.push_str(&format!("{key}={value}"));
    }
    if !first_opts_string.is_empty() {
        first_opts_string.push(',');
    }

    // all other blocks
    let mut sub_opts_string = String::with_capacity(sub_opts.len() * 8);
    for (key, value) in sub_opts {
        if !sub_opts_string.is_empty() {
            sub_opts_string.push(',');
        }
        sub_opts_string.push_str(&format!("{key}={value}"));
    }
    if !sub_opts_string.is_empty() {
        sub_opts_string.push(',');
    }

    let prefix = if tmux {
        out.write_all(b"\x1bPtmux;")?;
        "\x1b\x1b_G"
    } else {
        "\x1b_G"
    };
    let suffix = if tmux { "\x1b\x1b\\" } else { "\x1b\\" };

    let total_bytes = base64.len();
    let mut start = 0;

    while start < total_bytes {
        let end = min(start + size, total_bytes);
        let chunk_data = &base64[start..end];
        let more_chunks = (end != total_bytes) as u8;

        let opts = if start == 0 {
            &first_opts_string
        } else {
            &sub_opts_string
        };

        write!(
            out,
            "{prefix}{opts}q=2,m={more_chunks};{chunk_data}{suffix}"
        )?;

        start = end;
    }

    if tmux {
        out.write_all(b"\x1b\\")?;
    }

    Ok(())
}

/// encode an image bytes into inline image
/// should work with only png.
/// you can use crates like `image` to convert images into png
/// # example:
/// ```
/// use std::path::Path;
/// use rasteroid::kitty_encoder::encode_image;
/// use std::io::Write;
///
/// let path = Path::new("image.png");
/// let bytes = match std::fs::read(path) {
///     Ok(bytes) => bytes,
///     Err(e) => return,
/// };
/// let mut stdout = std::io::stdout();
/// encode_image(&bytes, &mut stdout, None, None).unwrap();
/// stdout.flush().unwrap();
/// ```
/// the option offset just offsets the image to the right by the amount of cells you specify
/// the print at is the same just absolute position
pub fn encode_image(
    img: &[u8],
    out: &mut impl Write,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = rand::random::<u32>();
    let mut opts = HashMap::from([
        ("f".to_string(), "100".to_string()),
        ("a".to_string(), "T".to_string()),
        ("i".to_string(), id.to_string()),
    ]);

    let winfo = term_misc::get_wininfo();
    let tmux = winfo.is_tmux;
    let inline = winfo.needs_inline || tmux;
    if inline {
        let data = image::load_from_memory(img)?;
        let (widthpx, heightpx) = data.dimensions();
        let cols =
            term_misc::dim_to_cells(&format!("{widthpx}px"), term_misc::SizeDirection::Width)?;
        let rows =
            term_misc::dim_to_cells(&format!("{heightpx}px"), term_misc::SizeDirection::Height)?;

        opts.insert("U".to_string(), 1.to_string());
        opts.insert("r".to_string(), rows.to_string());
        opts.insert("c".to_string(), cols.to_string());
        let base64 = image_to_base64(img);
        chunk_base64(&base64, out, 4096, opts, HashMap::new(), tmux)?;

        let placement = create_unicode_placeholder(cols, rows, id, offset, print_at)?;
        out.write_all(placement.as_bytes())?;
    } else {
        let center_string = offset_to_terminal(offset);
        let print_at_string = loc_to_terminal(print_at);
        out.write_all(print_at_string.as_ref())?;
        out.write_all(center_string.as_ref())?;
        let base64 = image_to_base64(img);
        chunk_base64(&base64, out, 4096, opts, HashMap::new(), tmux)?;
    }

    Ok(())
}

const DIACRITICS: &[&str] = &[
    "0305", "030D", "030E", "0310", "0312", "033D", "033E", "033F", "0346", "034A", "034B", "034C",
    "0350", "0351", "0352", "0357", "035B", "0363", "0364", "0365", "0366", "0367", "0368", "0369",
    "036A", "036B", "036C", "036D", "036E", "036F", "0483", "0484", "0485", "0486", "0487", "0592",
    "0593", "0594", "0595", "0597", "0598", "0599", "059C", "059D", "059E", "059F", "05A0", "05A1",
    "05A8", "05A9", "05AB", "05AC", "05AF", "05C4", "0610", "0611", "0612", "0613", "0614", "0615",
    "0616", "0617", "0657", "0658", "0659", "065A", "065B", "065D", "065E", "06D6", "06D7", "06D8",
    "06D9", "06DA", "06DB", "06DC", "06DF", "06E0", "06E1", "06E2", "06E4", "06E7", "06E8", "06EB",
    "06EC", "0730", "0732", "0733", "0735", "0736", "073A", "073D", "073F", "0740", "0741", "0743",
    "0745", "0747", "0749", "074A", "07EB", "07EC", "07ED", "07EE", "07EF", "07F0", "07F1", "07F3",
    "0816", "0817", "0818", "0819", "081B", "081C", "081D", "081E", "081F", "0820", "0821", "0822",
    "0823", "0825", "0826", "0827", "0829", "082A", "082B", "082C", "082D", "0951", "0953", "0954",
    "0F82", "0F83", "0F86", "0F87", "135D", "135E", "135F", "17DD", "193A", "1A17", "1A75", "1A76",
    "1A77", "1A78", "1A79", "1A7A", "1A7B", "1A7C", "1B6B", "1B6D", "1B6E", "1B6F", "1B70", "1B71",
    "1B72", "1B73", "1CD0", "1CD1", "1CD2", "1CDA", "1CDB", "1CE0", "1DC0", "1DC1", "1DC3", "1DC4",
    "1DC5", "1DC6", "1DC7", "1DC8", "1DC9", "1DCB", "1DCC", "1DD1", "1DD2", "1DD3", "1DD4", "1DD5",
    "1DD6", "1DD7", "1DD8", "1DD9", "1DDA", "1DDB", "1DDC", "1DDD", "1DDE", "1DDF", "1DE0", "1DE1",
    "1DE2", "1DE3", "1DE4", "1DE5", "1DE6", "1DFE", "20D0", "20D1", "20D4", "20D5", "20D6", "20D7",
    "20DB", "20DC", "20E1", "20E7", "20E9", "20F0", "2CEF", "2CF0", "2CF1", "2DE0", "2DE1", "2DE2",
    "2DE3", "2DE4", "2DE5", "2DE6", "2DE7", "2DE8", "2DE9", "2DEA", "2DEB", "2DEC", "2DED", "2DEE",
    "2DEF", "2DF0", "2DF1", "2DF2", "2DF3", "2DF4", "2DF5", "2DF6", "2DF7", "2DF8", "2DF9", "2DFA",
    "2DFB", "2DFC", "2DFD", "2DFE", "2DFF", "A66F", "A67C", "A67D", "A6F0", "A6F1", "A8E0", "A8E1",
    "A8E2", "A8E3", "A8E4", "A8E5", "A8E6", "A8E7", "A8E8", "A8E9", "A8EA", "A8EB", "A8EC", "A8ED",
    "A8EE", "A8EF", "A8F0", "A8F1", "AAB0", "AAB2", "AAB3", "AAB7", "AAB8", "AABE", "AABF", "AAC1",
    "FE20", "FE21", "FE22", "FE23", "FE24", "FE25", "FE26", "10A0F", "10A38", "1D185", "1D186",
    "1D187", "1D188", "1D189", "1D1AA", "1D1AB", "1D1AC", "1D1AD", "1D242", "1D243", "1D244",
];

const PLACEHOLDER: char = '\u{10EEEE}';

fn get_diacritic(index: usize) -> Option<char> {
    DIACRITICS
        .get(index)
        .and_then(|hex_str| u32::from_str_radix(hex_str, 16).ok())
        .and_then(char::from_u32)
}

pub fn create_unicode_placeholder(
    columns: u32,
    rows: u32,
    image_id: u32,
    offset: Option<u16>,
    print_at: Option<(u16, u16)>,
) -> Result<String, Box<dyn Error>> {
    let mut result = String::new();

    let r = (image_id >> 16) & 255;
    let g = (image_id >> 8) & 255;
    let b = image_id & 255;
    let id = &format!("\x1b[38;2;{};{};{}m", r, g, b);
    result.push_str(id);

    let id_char = get_diacritic(((image_id >> 24) & 255) as usize);
    let is_controlled = print_at.is_some();

    for row in 0..rows {
        let offset_string = term_misc::offset_to_terminal(offset);
        let print_at_for_row = print_at.map(|(x, y)| (x, y + row as u16));
        let print_at_string = loc_to_terminal(print_at_for_row);
        result.push_str(&print_at_string);
        result.push_str(&offset_string);
        result.push_str(id);

        for col in 0..columns {
            result.push(PLACEHOLDER);
            if let Some(row_diacritic) = get_diacritic(row as usize) {
                result.push(row_diacritic);
            }
            if let Some(col_diacritic) = get_diacritic(col as usize) {
                result.push(col_diacritic);
            }
            if let Some(id) = id_char {
                result.push(id);
            }
        }
        if !is_controlled {
            if row < rows - 1 {
                result.push_str("\n");
            }
        }
    }

    result.push_str("\x1b[39m");
    Ok(result)
}

fn process_frame(
    data: &[u8],
    out: &mut impl Write,
    first_opts: HashMap<String, String>,
    sub_opts: Option<HashMap<String, String>>,
    use_shm: bool,
    shm_name: &str,
    tmux: bool,
) -> Result<(), Box<dyn Error>> {
    if use_shm {
        transmit_shm(data, out, first_opts, shm_name, tmux)?;
    } else {
        let base64 = general_purpose::STANDARD.encode(data);
        chunk_base64(
            &base64,
            out,
            4096,
            first_opts,
            sub_opts.unwrap_or_default(),
            tmux,
        )?;
    }
    Ok(())
}

/// encode a video into inline video.
/// recommended to use in conjunction with video parsing library
/// this function differs from encode_frames function by using shared memory objects; and in turns
/// becomes faster, and requires less cpu power, but leaks memory (shm objects will only be cleaned
/// when someone reads the shm objects e.g kitty)
/// # example:
/// first make sure you can supply a iter of Frames (using ffmpeg-sidecar here)
/// ```rust,no_run
/// use ffmpeg_sidecar::command::FfmpegCommand;
/// use rasteroid::Frame;
/// use ffmpeg_sidecar::event::OutputVideoFrame;
/// use rasteroid::kitty_encoder::encode_frames_fast;
/// use rasteroid::kitty_encoder::is_kitty_capable;
/// use rasteroid::image_extended::calc_fit;
/// use ffmpeg_sidecar::event::FfmpegEvent;
///
/// pub struct KittyFrames(pub OutputVideoFrame);
/// impl Frame for KittyFrames {
///     fn width(&self) -> u16 {
///         self.0.width as u16
///     }
///     fn height(&self) -> u16 {
///         self.0.height as u16
///     }
///     fn timestamp(&self) -> f32 {
///         self.0.timestamp
///     }
///     // must be rgb8!
///     fn data(&self) -> &[u8] {
///         &self.0.data
///     }
/// }
/// // next get the frames (taken from ffmpeg-sidecar)
///
/// let mut out = std::io::stdout();
/// let iter = match FfmpegCommand::new() // <- Builder API like `std::process::Command`
///   .testsrc()  // <- Discoverable aliases for FFmpeg args
///   .rawvideo() // <- Convenient argument presets
///   .spawn()    // <- Ordinary `std::process::Child`
///    {
///        Ok(res) => res,
///        Err(e) => return,
/// }.iter().unwrap();   // <- Blocking iterator over logs and output
/// // now convert to compatible frames
/// let mut kitty_frames = iter
///         .filter_map(|event| {
///             if let FfmpegEvent::OutputFrame(frame) = event {
///                 Some(KittyFrames(frame))
///             } else {
///                 None
///             }
///         });
/// let id = rand::random::<u32>();
/// // should be used carefully, leaks memory with lifetime over the app itself. if kitty doesn't
/// // consume the animation, will just sit in /dev/shm or corresponding place for other os and
/// // take alot of memory
/// unsafe { encode_frames_fast(&mut kitty_frames, &mut out, true) };
/// ```
pub unsafe fn encode_frames_fast(
    frames: &mut dyn Iterator<Item = impl Frame>,
    out: &mut impl Write,
    center: bool,
) -> Result<(), Box<dyn Error>> {
    let id = encode_frames_sep(frames, out, center, true)?;

    // if the terminal didn't use a single shm object by now, im cleaning them all myself
    let shm_name = format!("mcat-video-{id}-0");
    if ShmemConf::new().os_id(&shm_name).open().is_ok() {
        let mut index = 0;
        loop {
            let name = format!("mcat-video-{id}-{index}");

            match ShmemConf::new().os_id(&name).open() {
                Ok(mut shmem) => {
                    shmem.set_owner(true);
                    drop(shmem);
                    index += 1;
                }
                Err(_) => break,
            }
        }
    }

    Ok(())
}

/// encode a video into inline video.
/// recommended to use in conjunction with video parsing library
/// # example:
/// first make sure you can supply a iter of Frames (using ffmpeg-sidecar here)
/// ```rust,no_run
/// use ffmpeg_sidecar::command::FfmpegCommand;
/// use rasteroid::Frame;
/// use ffmpeg_sidecar::event::OutputVideoFrame;
/// use rasteroid::kitty_encoder::encode_frames;
/// use rasteroid::kitty_encoder::is_kitty_capable;
/// use rasteroid::image_extended::calc_fit;
/// use ffmpeg_sidecar::event::FfmpegEvent;
///
/// pub struct KittyFrames(pub OutputVideoFrame);
/// impl Frame for KittyFrames {
///     fn width(&self) -> u16 {
///         self.0.width as u16
///     }
///     fn height(&self) -> u16 {
///         self.0.height as u16
///     }
///     fn timestamp(&self) -> f32 {
///         self.0.timestamp
///     }
///     // must be rgb8!
///     fn data(&self) -> &[u8] {
///         &self.0.data
///     }
/// }
/// // next get the frames (taken from ffmpeg-sidecar)
///
/// let mut out = std::io::stdout();
/// let iter = match FfmpegCommand::new() // <- Builder API like `std::process::Command`
///   .testsrc()  // <- Discoverable aliases for FFmpeg args
///   .rawvideo() // <- Convenient argument presets
///   .spawn()    // <- Ordinary `std::process::Child`
///    {
///        Ok(res) => res,
///        Err(e) => return,
/// }.iter().unwrap();   // <- Blocking iterator over logs and output
/// // now convert to compatible frames
/// let mut kitty_frames = iter
///         .filter_map(|event| {
///             if let FfmpegEvent::OutputFrame(frame) = event {
///                 Some(KittyFrames(frame))
///             } else {
///                 None
///             }
///         });
/// let id = rand::random::<u32>();
/// encode_frames(&mut kitty_frames, &mut out, true);
/// ```
pub fn encode_frames(
    frames: &mut dyn Iterator<Item = impl Frame>,
    out: &mut impl Write,
    center: bool,
) -> Result<(), Box<dyn Error>> {
    encode_frames_sep(frames, out, center, false)?;
    Ok(())
}

fn encode_frames_sep(
    frames: &mut dyn Iterator<Item = impl Frame>,
    out: &mut impl Write,
    center: bool,
    use_shm: bool,
) -> Result<u32, Box<dyn Error>> {
    // getting the first frame
    let first = frames.next().ok_or("video doesn't contain any frames")?;
    let width = first.width();
    let height = first.height();

    let mut pre_timestamp = 0.0;
    let id = rand::random::<u32>();
    let shm_name = format!("mcat-video-{id}-");

    let winfo = term_misc::get_wininfo();
    let tmux = winfo.is_tmux;
    let inline = winfo.needs_inline || tmux;
    let prefix = if tmux {
        "\x1bPtmux;\x1b\x1b_G"
    } else {
        "\x1b_G"
    };
    let suffix = if tmux { "\x1b\x1b\\\x1b\\" } else { "\x1b\\" };

    let offset = if center {
        Some(term_misc::center_image(width as u16, false))
    } else {
        None
    };
    if center && !inline {
        let center = offset_to_terminal(offset);
        out.write_all(center.as_bytes())?;
    }

    let i = id.to_string();
    let s = first.width().to_string();
    let v = first.height().to_string();
    let f = "24".to_string();
    let mut opts = HashMap::from([
        ("a".to_string(), "T".to_string()),
        ("f".to_string(), f),
        ("i".to_string(), i),
        ("s".to_string(), s),
        ("v".to_string(), v),
    ]);
    let (rows, cols) = if inline {
        let cols = term_misc::dim_to_cells(&format!("{width}px"), term_misc::SizeDirection::Width)?;
        let rows =
            term_misc::dim_to_cells(&format!("{height}px"), term_misc::SizeDirection::Height)?;
        opts.insert("U".to_string(), 1.to_string());
        opts.insert("r".to_string(), rows.to_string());
        opts.insert("c".to_string(), cols.to_string());
        (rows, cols)
    } else {
        (0, 0)
    };

    // adding the root image
    process_frame(
        &first.data(),
        out,
        opts,
        None,
        use_shm,
        &format!("{shm_name}thumb"),
        tmux,
    )?;

    // starting the animation
    let z = 100;
    write!(out, "{prefix}a=a,s=2,v=1,r=1,i={id},z={z}{suffix}")?;

    let shutdown = term_misc::setup_signal_handler();

    for (c, frame) in frames.enumerate() {
        if shutdown.load(Ordering::SeqCst) {
            break; // clean exit
        }
        let s = frame.width().to_string();
        let v = frame.height().to_string();
        let i = id.to_string();
        let f = "24".to_string();
        let z = ((frame.timestamp() - pre_timestamp) * 1000.0) as u32;
        pre_timestamp = frame.timestamp();

        let first_opts = HashMap::from([
            ("a".to_string(), "f".to_string()),
            ("f".to_string(), f),
            ("i".to_string(), i),
            ("c".to_string(), c.to_string()),
            ("s".to_string(), s),
            ("v".to_string(), v),
            ("z".to_string(), z.to_string()),
        ]);
        let sub_opts = HashMap::from([("a".to_string(), "f".to_string())]);

        if process_frame(
            &frame.data(),
            out,
            first_opts,
            Some(sub_opts),
            use_shm,
            &format!("{shm_name}{c}"),
            tmux,
        )
        .is_err()
        {
            break;
        }
    }

    if inline {
        let placement = create_unicode_placeholder(cols, rows, id, offset, None)?;
        out.write_all(placement.as_bytes())?;
    }
    write!(out, "{prefix}a=a,s=3,v=1,r=1,i={id},z={z}{suffix}")?;
    Ok(id)
}

pub fn delete_all_images(out: &mut impl Write) -> Result<(), std::io::Error> {
    let tmux = term_misc::get_wininfo().is_tmux;
    if tmux {
        out.write_all(b"\x1bPtmux;\x1b\x1b_Ga=d,d=r,x=0,y=4294967295\x1b\x1b\\\x1b\\")
    } else {
        out.write_all(b"\x1b_Ga=d,d=r,x=0,y=4294967295\x1b\\")
    }
}
pub fn delete_single_image(id: u32, out: &mut impl Write) -> Result<(), std::io::Error> {
    let tmux = term_misc::get_wininfo().is_tmux;
    if tmux {
        write!(
            out,
            "\x1bPtmux;\x1b\x1b_Ga=d,d=i,i={id},p={id}\x1b\x1b\\\x1b\\"
        )
    } else {
        write!(out, "\x1b_Ga=d,d=i,i={id},p={id}\x1b\\")
    }
}

/// checks if the current terminal supports Kitty's graphic protocol
/// # example:
/// ```
/// use rasteroid::kitty_encoder::is_kitty_capable;
///
/// let mut env = rasteroid::term_misc::EnvIdentifiers::new();
/// let is_capable = is_kitty_capable(&mut env);
/// println!("Kitty: {}", is_capable);
/// ```
pub fn is_kitty_capable(env: &mut EnvIdentifiers) -> bool {
    env.term_contains("kitty") || env.term_contains("ghostty")
}
