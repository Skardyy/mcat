use clap::{
    Parser, ValueEnum,
    builder::{
        Styles,
        styling::{AnsiColor, Effects},
    },
};
use clap_complete::Shell;
use rasteroid::{
    RasterEncoder,
    term_misc::{self, EnvIdentifiers, Wininfo},
};

#[derive(Parser)]
#[command(
    name = "mcat",
    version,
    author,
    about = "Terminal image, video, and Markdown viewer",
    color = clap::ColorChoice::Always,
    styles = get_styles(),
)]
pub struct McatConfig {
    /// Input source (file/dir/url/ls)
    #[arg(num_args = 1..)]
    pub input: Vec<String>,

    // ## Core Options ##
    /// Color theme
    #[arg(long, short = 't', help_heading = "Core Options",
          env = "MCAT_THEME", default_value_t = Theme::Github)]
    pub theme: Theme,

    /// Output format
    #[arg(
        long,
        short = 'o',
        value_name = "format",
        help_heading = "Core Options",
        default_value_if("inline", "true", "inline"),
        default_value_if("interactive", "true", "interactive")
    )]
    pub output: Option<OutputFormat>,

    /// Shortcut for --output inline
    #[arg(short = 'i', help_heading = "Core Options")]
    inline: bool,

    /// Shortcut for --output interactive
    #[arg(short = 'I', help_heading = "Core Options")]
    interactive: bool,

    /// Show capabilities and terminal info
    #[arg(long, help_heading = "Core Options")]
    pub report: bool,

    /// Remove loading bars
    #[arg(long, help_heading = "Core Options")]
    pub silent: bool,

    /// Pixel bounding box for image rendering, auto-detected by default
    /// (e.g. 1920x1080, 1920xauto, autox1080)
    #[arg(
        long,
        value_name = "WxH",
        help_heading = "Core Options",
        default_value = "autoxauto"
    )]
    pub spx: String,

    /// Bounding box in columns x rows, auto-detected by default, affects both
    /// image sizing and markdown rendering
    /// (e.g. 100x20, 100xauto, autox20)
    #[arg(
        long,
        value_name = "WxH",
        help_heading = "Core Options",
        default_value = "autoxauto"
    )]
    pub sc: String,

    /// Scale multiplier applied over both spx and sc width
    #[arg(
        long,
        value_name = "float",
        help_heading = "Core Options",
        default_value_t = 1.0
    )]
    pub scalex: f32,

    /// Scale multiplier applied over both spx and sc height
    #[arg(
        long,
        value_name = "float",
        help_heading = "Core Options",
        default_value_t = 1.0
    )]
    pub scaley: f32,

    // ## Markdown Viewing ##
    /// Disable line numbers in code blocks
    #[arg(long, help_heading = "Markdown Viewing")]
    pub no_linenumbers: bool,

    /// What images to render in the markdown
    #[arg(long = "md-image", value_name = "mode", help_heading = "Markdown Viewing",
        default_value_t = MdImageMode::Auto,
        default_value_if("fast", "true", "none"),
        env = "MCAT_MD_IMAGE")]
    pub md_image: MdImageMode,

    /// Shortcut for --md-image none
    #[arg(short = 'f', help_heading = "Markdown Viewing")]
    fast: bool,

    /// Shows YAML headers too
    #[arg(long, help_heading = "Markdown Viewing")]
    pub header: bool,

    #[arg(long = "color", help_heading = "Markdown Viewing",
        hide = true,
        default_value_t = ColorMode::Auto,
        default_value_if("color_always", "true", "always"),
        default_value_if("color_never", "true", "never"))]
    pub color: ColorMode,

    /// Force ANSI formatting on
    #[arg(short = 'c', help_heading = "Markdown Viewing")]
    color_always: bool,

    /// Force ANSI formatting off
    #[arg(short = 'C', help_heading = "Markdown Viewing")]
    color_never: bool,

    /// Modify the default pager
    #[arg(
        long,
        value_name = "command",
        help_heading = "Markdown Viewing",
        env = "MCAT_PAGER",
        default_value = "less -r"
    )]
    pub pager: String,

    #[arg(long = "paging", help_heading = "Markdown Viewing",
        hide = true,
        default_value_t = PagingMode::Auto,
        default_value_if("paging_always", "true", "always"),
        default_value_if("paging_never", "true", "never"))]
    pub paging: PagingMode,

    /// Force paging on
    #[arg(short = 'p', help_heading = "Markdown Viewing")]
    paging_always: bool,

    /// Force paging off
    #[arg(short = 'P', help_heading = "Markdown Viewing")]
    paging_never: bool,

    // ## Image/Video Viewing ##
    /// Use Kitty image protocol
    #[arg(long, help_heading = "Image/Video Viewing")]
    kitty: bool,

    /// Use iTerm2 image protocol
    #[arg(long, help_heading = "Image/Video Viewing")]
    iterm: bool,

    /// Use Sixel image protocol
    #[arg(long, help_heading = "Image/Video Viewing")]
    sixel: bool,

    /// Use ASCII art output
    #[arg(long, help_heading = "Image/Video Viewing")]
    ascii: bool,

    /// Disable centering the image in the terminal
    #[arg(long, help_heading = "Image/Video Viewing")]
    pub no_center: bool,

    /// Image render width (e.g. 80% of terminal, 40c columns, 1920px pixels)
    #[arg(
        long,
        value_name = "size",
        help_heading = "Image/Video Viewing",
        default_value = "80%"
    )]
    pub img_width: String,

    /// Image render height (e.g. 80% of terminal, 40c rows, 1080px pixels)
    #[arg(
        long,
        value_name = "size",
        help_heading = "Image/Video Viewing",
        default_value = "40%"
    )]
    pub img_height: String,

    /// Image zoom level
    #[arg(long, value_name = "level", help_heading = "Image/Video Viewing")]
    pub img_zoom: Option<usize>,

    /// X offset from top-left corner in pixels
    #[arg(long, value_name = "pixels", help_heading = "Image/Video Viewing")]
    pub img_x_offset: Option<i32>,

    /// Y offset from top-left corner in pixels
    #[arg(long, value_name = "pixels", help_heading = "Image/Video Viewing")]
    pub img_y_offset: Option<i32>,

    // ## Conversion ##
    /// Add styling to HTML output
    #[arg(long, help_heading = "Conversion")]
    pub style_html: bool,

    // ## Directory Listing ##
    /// Include hidden files
    #[arg(long, short = 'a', help_heading = "Directory Listing")]
    pub hidden: bool,

    /// Add hyperlinks to file names
    #[arg(long, help_heading = "Directory Listing")]
    pub hyprlink: bool,

    /// Sort method
    #[arg(long, help_heading = "Directory Listing",
        default_value_t = SortMode::Name,
        default_value_if("sort_type", "true", "type"),
        default_value_if("sort_size", "true", "size"))]
    pub sort: SortMode,

    /// Shortcut for --sort type
    #[arg(short = 'X', help_heading = "Directory Listing")]
    sort_type: bool,

    /// Shortcut for --sort size
    #[arg(short = 'S', help_heading = "Directory Listing")]
    sort_size: bool,

    /// Reverse the order of items
    #[arg(long, short = 'r', help_heading = "Directory Listing")]
    pub reverse: bool,

    /// Cell x padding (e.g. 3c columns, 10% of the terminal, 100px pixels)
    #[arg(
        long,
        value_name = "size",
        help_heading = "Directory Listing",
        default_value = "3c"
    )]
    pub ls_x_padding: String,

    /// Cell y padding (e.g. 2c rows, 10% of the terminal, 100px pixels)
    #[arg(
        long,
        value_name = "size",
        help_heading = "Directory Listing",
        default_value = "2c"
    )]
    pub ls_y_padding: String,

    /// Minimum cell width (e.g. 2c columns, 10% of the terminal, 100px pixels)
    #[arg(
        long,
        value_name = "size",
        help_heading = "Directory Listing",
        default_value = "2c"
    )]
    pub ls_min_width: String,

    /// Maximum cell width (e.g. 16c columns, 10% of the terminal, 100px pixels)
    #[arg(
        long,
        value_name = "size",
        help_heading = "Directory Listing",
        default_value = "16c"
    )]
    pub ls_max_width: String,

    /// Cell height (e.g. 2c rows, 10% of the terminal, 100px pixels)
    #[arg(
        long,
        value_name = "size",
        help_heading = "Directory Listing",
        default_value = "2c"
    )]
    pub ls_height: String,

    /// Maximum items per row
    #[arg(
        long,
        value_name = "count",
        help_heading = "Directory Listing",
        default_value_t = 20
    )]
    pub ls_items_per_row: usize,

    // ## System Operations ##
    /// Generate shell completions
    #[arg(long, value_name = "shell", help_heading = "System Operations")]
    pub generate: Option<Shell>,

    /// Download and prepare chromium
    #[arg(long, help_heading = "System Operations")]
    pub fetch_chromium: bool,

    /// Download and prepare ffmpeg
    #[arg(long, help_heading = "System Operations")]
    pub fetch_ffmpeg: bool,

    /// Clean up local binaries
    #[arg(long, help_heading = "System Operations")]
    pub fetch_clean: bool,

    // ## Runtime ##
    #[arg(skip)]
    pub wininfo: Option<Wininfo>,

    #[arg(skip)]
    pub env_id: Option<EnvIdentifiers>,

    #[arg(skip)]
    pub encoder: Option<RasterEncoder>,
}

impl McatConfig {
    pub fn finalize(&mut self) -> anyhow::Result<()> {
        let env = term_misc::EnvIdentifiers::new();
        let spx = Some(self.spx.as_ref());
        let sc = Some(self.sc.as_ref());
        let wininfo = Wininfo::new(spx, sc, Some(self.scalex), Some(self.scaley), &env)?;

        let encoder = if self.kitty {
            RasterEncoder::Kitty
        } else if self.iterm {
            RasterEncoder::Iterm
        } else if self.sixel {
            RasterEncoder::Sixel
        } else if self.ascii {
            RasterEncoder::Ascii
        } else {
            RasterEncoder::auto_detect(&env)
        };

        self.env_id = Some(env);
        self.wininfo = Some(wininfo);
        self.encoder = Some(encoder);
        Ok(())
    }
}

fn get_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Blue.on_default())
        .placeholder(AnsiColor::Yellow.on_default())
        .usage(AnsiColor::Magenta.on_default())
}

#[derive(ValueEnum, Clone)]
pub enum Theme {
    Catppuccin,
    Nord,
    Monokai,
    Dracula,
    Gruvbox,
    OneDark,
    Solarized,
    TokyoNight,
    MakuraiLight,
    MakuraiDark,
    Ayu,
    AyuMirage,
    Github,
    Synthwave,
    Material,
    RosePine,
    Kanagawa,
    Vscode,
    Everforest,
    Autumn,
    Spring,
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.to_possible_value().unwrap().get_name().fmt(f)
    }
}

#[derive(ValueEnum, Clone, PartialEq)]
pub enum OutputFormat {
    Html,
    Md,
    Image,
    Video,
    Inline,
    Interactive,
}

#[derive(ValueEnum, Clone, PartialEq, Default)]
pub enum ColorMode {
    Never,
    Always,
    #[default]
    Auto,
}

impl std::fmt::Display for ColorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.to_possible_value().unwrap().get_name().fmt(f)
    }
}

#[derive(ValueEnum, Clone, PartialEq, Default)]
pub enum PagingMode {
    Never,
    Always,
    #[default]
    Auto,
}

impl std::fmt::Display for PagingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.to_possible_value().unwrap().get_name().fmt(f)
    }
}

#[derive(ValueEnum, Clone, Default)]
pub enum MdImageMode {
    All,
    Small,
    None,
    #[default]
    Auto,
}

impl std::fmt::Display for MdImageMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.to_possible_value().unwrap().get_name().fmt(f)
    }
}

#[derive(ValueEnum, Clone, Default)]
pub enum ImageProtocol {
    #[default]
    Auto,
    Kitty,
    Iterm,
    Sixel,
    Ascii,
}

impl std::fmt::Display for ImageProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.to_possible_value().unwrap().get_name().fmt(f)
    }
}

#[derive(ValueEnum, Clone, Default)]
pub enum SortMode {
    #[default]
    Name,
    Size,
    Time,
    Type,
}

impl std::fmt::Display for SortMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.to_possible_value().unwrap().get_name().fmt(f)
    }
}
