use std::{collections::HashMap, env};

use clap::ArgMatches;
use rasteroid::{InlineEncoder, term_misc};

#[derive(Debug, Clone)]
pub struct InlineOptions {
    pub center: bool,
    pub width: Option<String>,
    pub height: Option<String>,
    pub spx: String,
    pub sc: String,
    pub scale: Option<f32>,
    pub zoom: Option<usize>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub inline: bool,
}

impl Default for InlineOptions {
    fn default() -> Self {
        InlineOptions {
            center: true,
            width: Some("80%".into()),
            height: Some("80%".into()),
            spx: "1920x1080".into(),
            sc: "100x20".into(),
            scale: None,
            zoom: None,
            x: None,
            y: None,
            inline: false,
        }
    }
}

impl InlineOptions {
    pub fn extend_from_string(&mut self, s: &str) -> &mut Self {
        let map: HashMap<_, _> = s
            .split(',')
            .filter_map(|pair| {
                let mut split = pair.splitn(2, '=');
                let key = split.next()?.trim();
                let value = split.next()?.trim();
                Some((key, value))
            })
            .collect();

        let get = |key: &str| map.get(key).copied();
        let get_size = |key: &str, default: &Option<String>| match map.get(key) {
            Some(v) => {
                if v.eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(v.to_string())
                }
            }
            None => default.clone(),
        };

        self.width = get_size("width", &self.width);
        self.height = get_size("height", &self.height);
        if let Some(spx) = get("spx") {
            self.spx = spx.to_string();
        }
        if let Some(sc) = get("sc") {
            self.sc = sc.to_string();
        }
        self.scale = get("scale").and_then(|v| v.parse().ok()).or(self.scale);
        self.zoom = get("zoom").and_then(|v| v.parse().ok()).or(self.zoom);
        self.x = get("x").and_then(|v| v.parse().ok()).or(self.x);
        self.y = get("y").and_then(|v| v.parse().ok()).or(self.y);
        self.center = get("center")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(self.center);
        self.inline = get("inline")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(self.inline);
        self
    }
}

#[derive(Clone)]
pub struct LsixOptions {
    pub x_padding: String,
    pub y_padding: String,
    pub min_width: String,
    pub max_width: String,
    pub height: String,
    pub max_items_per_row: usize,
}

impl Default for LsixOptions {
    fn default() -> Self {
        LsixOptions {
            x_padding: "3c".into(),
            y_padding: "2c".into(),
            min_width: "2c".into(),
            max_width: "16c".into(),
            height: "2c".into(),
            max_items_per_row: 20,
        }
    }
}

impl LsixOptions {
    pub fn extend_from_string(&mut self, s: &str) -> &mut Self {
        let map: HashMap<_, _> = s
            .split(',')
            .filter_map(|pair| {
                let mut split = pair.splitn(2, '=');
                let key = split.next()?.trim();
                let value = split.next()?.trim();
                Some((key, value))
            })
            .collect();

        let get = |key: &str| map.get(key).copied();

        if let Some(x_padding) = get("x_padding") {
            self.x_padding = x_padding.to_string();
        }
        if let Some(y_padding) = get("y_padding") {
            self.y_padding = y_padding.to_string();
        }
        if let Some(min_width) = get("min_width") {
            self.min_width = min_width.to_string();
        }
        if let Some(max_width) = get("max_width") {
            self.max_width = max_width.to_string();
        }
        if let Some(height) = get("height") {
            self.height = height.to_string();
        }
        self.max_items_per_row = get("items_per_row")
            .and_then(|v| v.parse().ok())
            .unwrap_or(self.max_items_per_row);
        self
    }
}

#[derive(Clone)]
pub struct McatConfig {
    pub input: Vec<String>,
    pub output: Option<String>,
    pub is_ls: bool,
    pub inline_encoder: InlineEncoder,
    pub ls_options: LsixOptions,
    pub inline_options: InlineOptions,
    pub is_tmux: bool,
    pub silent: bool,
    pub hidden: bool,
    pub ls_hyprlink: bool,
    pub report: bool,
    pub no_linenumbers: bool,
    pub md_image_render: MdImageRender,
    pub yaml_header: bool,
    pub horizontal_image_stacking: bool,
    pub style_html: bool,
    pub theme: String,
    pub fn_and_leave: Option<FnAndLeave>,
    pub pager: String,
    pub color: AlwaysOrNever,
    pub paging: AlwaysOrNever,
    encoder_force: String,
}

#[derive(Clone)]
pub enum AlwaysOrNever {
    Always,
    Never,
    Auto,
}

impl AlwaysOrNever {
    pub fn from_string(s: &str) -> AlwaysOrNever {
        match s.to_lowercase().as_ref() {
            "always" => return AlwaysOrNever::Always,
            "never" => return AlwaysOrNever::Never,
            _ => return AlwaysOrNever::Always,
        }
    }
    pub fn should_use(&self, other: bool) -> bool {
        match self {
            AlwaysOrNever::Always => true,
            AlwaysOrNever::Never => false,
            AlwaysOrNever::Auto => other,
        }
    }
}

#[derive(Clone)]
pub enum FnAndLeave {
    ShellGenerate(String),
    DeleteImages,
    FetchChromium,
    FetchFfmpeg,
    FetchClean,
    Report,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MdImageRender {
    All,
    Small,
    None,
    Auto,
}

impl Default for McatConfig {
    fn default() -> Self {
        McatConfig {
            input: Vec::new(),
            output: None,
            is_ls: false,
            inline_encoder: InlineEncoder::Ascii,
            is_tmux: false,
            ls_options: LsixOptions::default(),
            inline_options: InlineOptions::default(),
            silent: false,
            hidden: false,
            ls_hyprlink: false,
            report: false,
            no_linenumbers: false,
            yaml_header: false,
            md_image_render: MdImageRender::Auto,
            horizontal_image_stacking: false,
            style_html: false,
            theme: "dark".into(),
            fn_and_leave: None,
            encoder_force: String::new(),
            pager: "less -r".into(),
            color: AlwaysOrNever::Auto,
            paging: AlwaysOrNever::Auto,
        }
    }
}

impl McatConfig {
    pub fn extend_from_args(&mut self, opts: &ArgMatches) -> &mut Self {
        self.input = opts
            .get_many::<String>("input")
            .unwrap_or_default()
            .cloned()
            .collect();
        self.is_ls = self.input.get(0).unwrap_or(&"".to_owned()).to_lowercase() == "ls";

        // encoder
        let mut kitty = opts.get_flag("kitty");
        let mut iterm = opts.get_flag("iterm");
        let mut sixel = opts.get_flag("sixel");
        let mut ascii = opts.get_flag("ascii");
        match self.encoder_force.as_ref() {
            "kitty" => kitty = true,
            "iterm" => iterm = true,
            "sixel" => sixel = true,
            "ascii" => ascii = true,
            _ => {}
        }
        let mut env = term_misc::EnvIdentifiers::new();
        self.is_tmux = env.is_tmux();
        self.inline_encoder =
            rasteroid::InlineEncoder::auto_detect(kitty, iterm, sixel, ascii, &mut env);

        // fn and leave
        if let Some(shell) = opts.get_one::<String>("generate-completions") {
            self.fn_and_leave = Some(FnAndLeave::ShellGenerate(shell.clone()));
            return self;
        }
        if opts.get_flag("delete-all-images") {
            self.fn_and_leave = Some(FnAndLeave::DeleteImages);
            return self;
        }
        if opts.get_flag("fetch-chromium") {
            self.fn_and_leave = Some(FnAndLeave::FetchChromium);
            return self;
        }
        if opts.get_flag("fetch-ffmpeg") {
            self.fn_and_leave = Some(FnAndLeave::FetchFfmpeg);
            return self;
        }
        if opts.get_flag("fetch-clean") {
            self.fn_and_leave = Some(FnAndLeave::FetchClean);
            return self;
        }
        self.report = opts
            .get_one::<bool>("report")
            .copied()
            .unwrap_or(self.report);
        if self.report && self.input.is_empty() {
            self.fn_and_leave = Some(FnAndLeave::Report);
            return self;
        }

        // simple Assignment
        if let Some(ls_options) = opts.get_one::<String>("ls-options") {
            self.ls_options.extend_from_string(&ls_options);
        }
        if let Some(inline_options) = opts.get_one::<String>("inline-options") {
            self.inline_options.extend_from_string(&inline_options);
        }
        if opts.get_flag("silent") {
            self.silent = true;
        }
        if opts.get_flag("hidden") {
            self.hidden = true;
        }
        if opts.get_flag("hyprlink") {
            self.ls_hyprlink = true;
        }
        if opts.get_flag("no-linenumbers") {
            self.no_linenumbers = true;
        }
        self.md_image_render = match opts.get_one::<String>("md-image") {
            Some(v) => match v.as_str() {
                "all" => MdImageRender::All,
                "small" => MdImageRender::Small,
                "none" => MdImageRender::None,
                "auto" => MdImageRender::Auto,
                _ => self.md_image_render,
            },
            None => self.md_image_render,
        };
        if opts.get_flag("fast") {
            self.md_image_render = MdImageRender::None
        }
        if opts.get_flag("header") {
            self.yaml_header = true;
        }
        if opts.get_flag("horizontal") {
            self.horizontal_image_stacking = true;
        }
        if opts.get_flag("style-html") {
            self.style_html = true;
        }
        if let Some(theme) = opts.get_one::<String>("theme") {
            self.theme = theme.clone();
        }
        // paging
        if let Some(pager) = opts.get_one::<String>("pager") {
            self.pager = pager.clone();
        }
        if let Some(paging) = opts.get_one::<String>("paging") {
            self.paging = AlwaysOrNever::from_string(paging);
        }
        if opts.get_flag("paging-always") {
            self.paging = AlwaysOrNever::Always
        }
        if opts.get_flag("paging-never") {
            self.paging = AlwaysOrNever::Never
        }
        // color
        if let Some(color) = opts.get_one::<String>("color") {
            self.color = AlwaysOrNever::from_string(color);
        }
        if opts.get_flag("color-always") {
            self.color = AlwaysOrNever::Always
        }
        if opts.get_flag("color-never") {
            self.color = AlwaysOrNever::Never
        }

        // output
        let inline = opts.get_flag("inline");
        let interactive = opts.get_flag("interactive");
        self.output = if inline {
            Some("inline".to_string())
        } else if interactive {
            Some("interactive".to_string())
        } else {
            opts.get_one::<String>("output").cloned()
        };

        self
    }

    pub fn extend_from_env(&mut self) -> &mut Self {
        if let Ok(v) = env::var("MCAT_ENCODER") {
            self.encoder_force = v.to_lowercase();
        }
        if let Ok(v) = env::var("MCAT_PAGER") {
            self.pager = v;
        }
        if let Ok(v) = env::var("MCAT_THEME") {
            self.theme = v;
        }
        if let Ok(v) = env::var("MCAT_INLINE_OPTS") {
            self.inline_options.extend_from_string(&v);
        }
        if let Ok(v) = env::var("MCAT_LS_OPTS") {
            self.ls_options.extend_from_string(&v);
        }
        if let Ok(v) = env::var("MCAT_HYPRLINK") {
            self.ls_hyprlink = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = env::var("MCAT_SILENT") {
            self.silent = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = env::var("MCAT_NO_LINENUMBERS") {
            self.no_linenumbers = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = env::var("MCAT_MD_IMAGE") {
            self.md_image_render = match v.to_lowercase().as_str() {
                "all" => MdImageRender::All,
                "small" => MdImageRender::Small,
                "none" => MdImageRender::None,
                "auto" => MdImageRender::Auto,
                _ => self.md_image_render,
            };
        }

        self
    }
}
