use std::str::FromStr;

use syntect::highlighting::{Color, ScopeSelectors, StyleModifier, Theme, ThemeSettings};

#[derive(Debug, Clone)]
pub struct ThemeColor {
    value: String,
    color: Color,
    pub bg: String,
    pub fg: String,
}

impl From<&str> for ThemeColor {
    fn from(hex_color: &str) -> Self {
        let color = hex_to_rgba(&hex_color);
        let (r, g, b) = (color.r, color.g, color.b);

        ThemeColor {
            value: hex_color.to_owned(),
            color,
            bg: format!("\x1b[48;2;{};{};{}m", r, g, b),
            fg: format!("\x1b[38;2;{};{};{}m", r, g, b),
        }
    }
}

impl From<&str> for CustomTheme {
    fn from(s: &str) -> Self {
        match s {
            "catppuccin" => CustomTheme::catppuccin(),
            "nord" => CustomTheme::nord(),
            "monokai" => CustomTheme::monokai(),
            "dracula" => CustomTheme::dracula(),
            "gruvbox" => CustomTheme::gruvbox(),
            "one_dark" => CustomTheme::one_dark(),
            "solarized" => CustomTheme::solarized(),
            "tokyo_night" => CustomTheme::tokyo_night(),
            "makurai_light" => CustomTheme::makurai_light(),
            "makurai_dark" => CustomTheme::makurai_dark(),
            "ayu" => CustomTheme::ayu(),
            "ayu_mirage" => CustomTheme::ayu_mirage(),
            "github" => CustomTheme::github(),
            "synthwave" => CustomTheme::synthwave(),
            "material" => CustomTheme::material(),
            "rose_pine" => CustomTheme::rose_pine(),
            "kanagawa" => CustomTheme::kanagawa(),
            "vscode" => CustomTheme::vscode(),
            "everforest" => CustomTheme::everforest(),
            "autumn" => CustomTheme::autumn(),
            "spring" => CustomTheme::spring(),
            _ => CustomTheme::github(),
        }
    }
}

impl From<String> for CustomTheme {
    fn from(s: String) -> Self {
        CustomTheme::from(s.as_ref())
    }
}

#[derive(Debug, Clone)]
pub struct CustomTheme {
    pub keyword: ThemeColor,
    pub function: ThemeColor,
    pub string: ThemeColor,
    pub module: ThemeColor,
    pub constant: ThemeColor,
    pub comment: ThemeColor,
    pub foreground: ThemeColor,
    pub guide: ThemeColor,
    pub background: ThemeColor,
    pub surface: ThemeColor,
    pub border: ThemeColor,
    pub keyword_bg: ThemeColor,

    pub red: ThemeColor,
    pub green: ThemeColor,
    pub blue: ThemeColor,
    pub cyan: ThemeColor,
    pub yellow: ThemeColor,

    #[allow(dead_code)]
    magenta: ThemeColor,
    #[allow(dead_code)]
    white: ThemeColor,
    pub black: ThemeColor,
}

fn hex_to_rgba(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    Color { r, g, b, a: 255 }
}

impl CustomTheme {
    pub fn autumn() -> Self {
        CustomTheme {
            keyword: "#fc6501".into(),
            keyword_bg: "#2A1A0D".into(),
            function: "#fac25a".into(),
            string: "#a1cd32".into(),
            module: "#fc4c4c".into(),
            constant: "#FF6B9D".into(),
            comment: "#5C6773".into(),
            foreground: "#FFFFFF".into(),
            guide: "#2D3640".into(),
            background: "#14161f".into(),
            surface: "#1E2129".into(),
            border: "#5C6773".into(),

            red: "#fc4c4c".into(),
            green: "#a1cd32".into(),
            blue: "#5abffa".into(),
            cyan: "#5abffa".into(),
            magenta: "#FF6B9D".into(),
            yellow: "#fac25a".into(),
            white: "#FFFFFF".into(),
            black: "#2e3339".into(),
        }
    }

    pub fn spring() -> Self {
        CustomTheme {
            keyword: "#FFB347".into(),
            keyword_bg: "#2A1F0D".into(),
            function: "#D4FF59".into(),
            string: "#37dbb5".into(),
            module: "#66E6FF".into(),
            constant: "#D8A5FF".into(),
            comment: "#5C6773".into(),
            foreground: "#FFFFFF".into(),
            guide: "#2D3640".into(),
            background: "#14161f".into(),
            surface: "#1E2129".into(),
            border: "#5C6773".into(),

            red: "#FF5555".into(),
            green: "#D4FF59".into(),
            blue: "#66E6FF".into(),
            cyan: "#66E6FF".into(),
            magenta: "#D2A6FF".into(),
            yellow: "#FFB347".into(),
            white: "#FFFFFF".into(),
            black: "#2e3339".into(),
        }
    }

    pub fn makurai_dark() -> Self {
        CustomTheme {
            keyword: "#FF7733".into(),
            keyword_bg: "#261810".into(),
            function: "#FFEE99".into(),
            string: "#95FB79".into(),
            module: "#82AAFF".into(),
            constant: "#D2A6FF".into(),
            comment: "#5C6773".into(),
            foreground: "#FFFFFF".into(),
            guide: "#2D3640".into(),
            background: "#14161f".into(),
            surface: "#1E212A".into(),
            border: "#5C6773".into(),

            red: "#FF5555".into(),
            green: "#95FB79".into(),
            blue: "#82AAFF".into(),
            cyan: "#66D9EF".into(),
            magenta: "#FF77FF".into(),
            yellow: "#FFEE99".into(),
            white: "#FFFFFF".into(),
            black: "#14161f".into(),
        }
    }

    pub fn makurai_light() -> Self {
        CustomTheme {
            keyword: "#E35043".into(),
            keyword_bg: "#FDF2F1".into(),
            function: "#3D76F3".into(),
            string: "#51A150".into(),
            module: "#AB31A9".into(),
            constant: "#976700".into(),
            comment: "#A0A1A7".into(),
            foreground: "#323640".into(),
            guide: "#D1D5DB".into(),
            background: "#f8f8fc".into(),
            surface: "#E8E8F0".into(),
            border: "#7e8a9e".into(),

            red: "#E35043".into(),
            green: "#51A150".into(),
            blue: "#3D76F3".into(),
            cyan: "#00BFCF".into(),
            magenta: "#AB31A9".into(),
            yellow: "#FFCC00".into(),
            white: "#FFFFFF".into(),
            black: "#000000".into(),
        }
    }

    pub fn monokai() -> Self {
        CustomTheme {
            keyword: "#F92672".into(),
            keyword_bg: "#2D1A1F".into(),
            function: "#A6E22E".into(),
            string: "#E6DB74".into(),
            module: "#66D9EF".into(),
            constant: "#AE81FF".into(),
            comment: "#75715E".into(),
            foreground: "#F8F8F2".into(),
            guide: "#3E3D32".into(),
            background: "#272822".into(),
            surface: "#343429".into(),
            border: "#49483E".into(),

            red: "#F92672".into(),
            green: "#A6E22E".into(),
            blue: "#66D9EF".into(),
            cyan: "#66D9EF".into(),
            magenta: "#AE81FF".into(),
            yellow: "#E6DB74".into(),
            white: "#F8F8F2".into(),
            black: "#272822".into(),
        }
    }

    pub fn catppuccin() -> Self {
        CustomTheme {
            keyword: "#CBA6F7".into(),
            keyword_bg: "#2A1F33".into(),
            function: "#89B4FA".into(),
            string: "#A6E3A1".into(),
            module: "#89DCEB".into(),
            constant: "#F38BA8".into(),
            comment: "#7F849C".into(),
            foreground: "#CDD6F4".into(),
            guide: "#45475A".into(),
            background: "#1E1E2E".into(),
            surface: "#2A2A3A".into(),
            border: "#45475A".into(),

            red: "#F38BA8".into(),
            green: "#A6E3A1".into(),
            blue: "#89B4FA".into(),
            cyan: "#89DCEB".into(),
            magenta: "#CBA6F7".into(),
            yellow: "#F9E2AF".into(),
            white: "#CDD6F4".into(),
            black: "#1E1E2E".into(),
        }
    }

    pub fn tokyo_night() -> Self {
        CustomTheme {
            keyword: "#BB9AF7".into(),
            keyword_bg: "#261F2D".into(),
            function: "#7AA2F7".into(),
            string: "#9ECE6A".into(),
            module: "#2AC3DE".into(),
            constant: "#FF9E64".into(),
            comment: "#565F89".into(),
            foreground: "#C0CAF5".into(),
            guide: "#3B4261".into(),
            background: "#1A1B26".into(),
            surface: "#24283B".into(),
            border: "#414868".into(),

            red: "#F7768E".into(),
            green: "#9ECE6A".into(),
            blue: "#7AA2F7".into(),
            cyan: "#2AC3DE".into(),
            magenta: "#BB9AF7".into(),
            yellow: "#E0AF68".into(),
            white: "#C0CAF5".into(),
            black: "#1A1B26".into(),
        }
    }

    pub fn dracula() -> Self {
        CustomTheme {
            keyword: "#FF79C6".into(),
            keyword_bg: "#2D1B26".into(),
            function: "#50FA7B".into(),
            string: "#F1FA8C".into(),
            module: "#8BE9FD".into(),
            constant: "#BD93F9".into(),
            comment: "#6272A4".into(),
            foreground: "#F8F8F2".into(),
            guide: "#44475A".into(),
            background: "#282A36".into(),
            surface: "#353746".into(),
            border: "#44475A".into(),

            red: "#FF5555".into(),
            green: "#50FA7B".into(),
            blue: "#8BE9FD".into(),
            cyan: "#8BE9FD".into(),
            magenta: "#FF79C6".into(),
            yellow: "#F1FA8C".into(),
            white: "#F8F8F2".into(),
            black: "#282A36".into(),
        }
    }

    pub fn nord() -> Self {
        CustomTheme {
            keyword: "#81A1C1".into(),
            keyword_bg: "#1C2329".into(),
            function: "#88C0D0".into(),
            string: "#A3BE8C".into(),
            module: "#8FBCBB".into(),
            constant: "#B48EAD".into(),
            comment: "#616E88".into(),
            foreground: "#D8DEE9".into(),
            guide: "#434C5E".into(),
            background: "#272E37".into(),
            surface: "#323A47".into(),
            border: "#434C5E".into(),

            red: "#BF616A".into(),
            green: "#A3BE8C".into(),
            blue: "#81A1C1".into(),
            cyan: "#88C0D0".into(),
            magenta: "#B48EAD".into(),
            yellow: "#EBCB8B".into(),
            white: "#D8DEE9".into(),
            black: "#2E3440".into(),
        }
    }

    pub fn gruvbox() -> Self {
        CustomTheme {
            keyword: "#FB4934".into(),
            keyword_bg: "#2B1A18".into(),
            function: "#FABD2F".into(),
            string: "#B8BB26".into(),
            module: "#83A598".into(),
            constant: "#D3869B".into(),
            comment: "#928374".into(),
            foreground: "#EBDBB2".into(),
            guide: "#504945".into(),
            background: "#282828".into(),
            surface: "#3C3836".into(),
            border: "#665C54".into(),

            red: "#FB4934".into(),
            green: "#B8BB26".into(),
            blue: "#83A598".into(),
            cyan: "#8EC07C".into(),
            magenta: "#D3869B".into(),
            yellow: "#FABD2F".into(),
            white: "#EBDBB2".into(),
            black: "#282828".into(),
        }
    }

    pub fn solarized() -> Self {
        CustomTheme {
            keyword: "#268BD2".into(),
            keyword_bg: "#0A2935".into(),
            function: "#B58900".into(),
            string: "#2AA198".into(),
            module: "#859900".into(),
            constant: "#D33682".into(),
            comment: "#586E75".into(),
            foreground: "#839496".into(),
            guide: "#073642".into(),
            background: "#002B36".into(),
            surface: "#0E3A47".into(),
            border: "#586E75".into(),

            red: "#DC322F".into(),
            green: "#859900".into(),
            blue: "#268BD2".into(),
            cyan: "#2AA198".into(),
            magenta: "#D33682".into(),
            yellow: "#B58900".into(),
            white: "#EEE8D5".into(),
            black: "#002B36".into(),
        }
    }

    pub fn one_dark() -> Self {
        CustomTheme {
            keyword: "#C678DD".into(),
            keyword_bg: "#2A1F2D".into(),
            function: "#61AFEF".into(),
            string: "#98C379".into(),
            module: "#56B6C2".into(),
            constant: "#E06C75".into(),
            comment: "#5C6370".into(),
            foreground: "#ABB2BF".into(),
            guide: "#3E4451".into(),
            background: "#282C34".into(),
            surface: "#353B45".into(),
            border: "#3E4451".into(),

            red: "#E06C75".into(),
            green: "#98C379".into(),
            blue: "#61AFEF".into(),
            cyan: "#56B6C2".into(),
            magenta: "#C678DD".into(),
            yellow: "#E5C07B".into(),
            white: "#ABB2BF".into(),
            black: "#282C34".into(),
        }
    }

    pub fn github() -> Self {
        CustomTheme {
            keyword: "#FF7B72".into(),
            keyword_bg: "#2B1618".into(),
            function: "#D2A8FF".into(),
            string: "#A5D6FF".into(),
            module: "#FFA657".into(),
            constant: "#79C0FF".into(),
            comment: "#8B949E".into(),
            foreground: "#F0F6FC".into(),
            guide: "#30363D".into(),
            background: "#0D1117".into(),
            surface: "#1C2128".into(),
            border: "#30363D".into(),

            red: "#FF7B72".into(),
            green: "#56D364".into(),
            blue: "#79C0FF".into(),
            cyan: "#A5D6FF".into(),
            magenta: "#D2A8FF".into(),
            yellow: "#FFA657".into(),
            white: "#F0F6FC".into(),
            black: "#0D1117".into(),
        }
    }

    pub fn material() -> Self {
        CustomTheme {
            keyword: "#C792EA".into(),
            keyword_bg: "#2F2A37".into(),
            function: "#82AAFF".into(),
            string: "#C3E88D".into(),
            module: "#FFCB6B".into(),
            constant: "#F78C6C".into(),
            comment: "#676E95".into(),
            foreground: "#A6ACCD".into(),
            guide: "#4E5579".into(),
            background: "#292D3E".into(),
            surface: "#32374D".into(),
            border: "#444267".into(),
            red: "#F07178".into(),
            green: "#C3E88D".into(),
            blue: "#82AAFF".into(),
            cyan: "#89DDFF".into(),
            magenta: "#C792EA".into(),
            yellow: "#FFCB6B".into(),
            white: "#FFFFFF".into(),
            black: "#292D3E".into(),
        }
    }

    pub fn ayu() -> Self {
        CustomTheme {
            keyword: "#FF8F40".into(),
            keyword_bg: "#1A1209".into(),
            function: "#FFB454".into(),
            string: "#AAD94C".into(),
            module: "#59C2FF".into(),
            constant: "#D2A6FF".into(),
            comment: "#ACB6BF8C".into(),
            foreground: "#BFBDB6".into(),
            guide: "#1F2430".into(),
            background: "#0A0E14".into(),
            surface: "#151A21".into(),
            border: "#1F2430".into(),

            red: "#F28779".into(),
            green: "#AAD94C".into(),
            blue: "#59C2FF".into(),
            cyan: "#95E6CB".into(),
            magenta: "#D2A6FF".into(),
            yellow: "#FFB454".into(),
            white: "#BFBDB6".into(),
            black: "#0A0E14".into(),
        }
    }

    pub fn ayu_mirage() -> Self {
        CustomTheme {
            keyword: "#FFA759".into(),
            keyword_bg: "#221A0D".into(),
            function: "#FFD580".into(),
            string: "#BAE67E".into(),
            module: "#73D0FF".into(),
            constant: "#D4BFFF".into(),
            comment: "#5C6773".into(),
            foreground: "#CBCCC6".into(),
            guide: "#242936".into(),
            background: "#1F2430".into(),
            surface: "#2A313F".into(),
            border: "#343B4C".into(),

            red: "#FF6666".into(),
            green: "#BAE67E".into(),
            blue: "#73D0FF".into(),
            cyan: "#95E6CB".into(),
            magenta: "#D4BFFF".into(),
            yellow: "#FFD580".into(),
            white: "#CBCCC6".into(),
            black: "#1F2430".into(),
        }
    }

    pub fn synthwave() -> Self {
        CustomTheme {
            keyword: "#FF7EDB".into(),
            keyword_bg: "#2B1929".into(),
            function: "#36F9F6".into(),
            string: "#E6DB74".into(),
            module: "#FE4450".into(),
            constant: "#FF8CC8".into(),
            comment: "#848077".into(),
            foreground: "#F8F8F2".into(),
            guide: "#2A2139".into(),
            background: "#262335".into(),
            surface: "#342949".into(),
            border: "#495495".into(),

            red: "#FE4450".into(),
            green: "#72F1B8".into(),
            blue: "#36F9F6".into(),
            cyan: "#36F9F6".into(),
            magenta: "#FF7EDB".into(),
            yellow: "#FEE715".into(),
            white: "#F8F8F2".into(),
            black: "#262335".into(),
        }
    }

    pub fn rose_pine() -> Self {
        CustomTheme {
            keyword: "#C4A7E7".into(),
            keyword_bg: "#24202E".into(),
            function: "#9CCFD8".into(),
            string: "#F6C177".into(),
            module: "#EBBCBA".into(),
            constant: "#EB6F92".into(),
            comment: "#6E6A86".into(),
            foreground: "#E0DEF4".into(),
            guide: "#26233A".into(),
            background: "#191724".into(),
            surface: "#21202E".into(),
            border: "#403D52".into(),

            red: "#EB6F92".into(),
            green: "#31748F".into(),
            blue: "#9CCFD8".into(),
            cyan: "#9CCFD8".into(),
            magenta: "#C4A7E7".into(),
            yellow: "#F6C177".into(),
            white: "#E0DEF4".into(),
            black: "#191724".into(),
        }
    }

    pub fn kanagawa() -> Self {
        CustomTheme {
            keyword: "#957FB8".into(),
            keyword_bg: "#1E1A22".into(),
            function: "#7AA89F".into(),
            string: "#98BB6C".into(),
            module: "#7FB4CA".into(),
            constant: "#D27E99".into(),
            comment: "#727169".into(),
            foreground: "#DCD7BA".into(),
            guide: "#2A2A37".into(),
            background: "#1F1F28".into(),
            surface: "#2A2A37".into(),
            border: "#54546D".into(),

            red: "#C34043".into(),
            green: "#76946A".into(),
            blue: "#7E9CD8".into(),
            cyan: "#6A9589".into(),
            magenta: "#938AA9".into(),
            yellow: "#C0A36E".into(),
            white: "#DCD7BA".into(),
            black: "#1F1F28".into(),
        }
    }

    pub fn everforest() -> Self {
        CustomTheme {
            keyword: "#E67E80".into(),
            keyword_bg: "#2B1F20".into(),
            function: "#A7C080".into(),
            string: "#DBBC7F".into(),
            module: "#7FBBB3".into(),
            constant: "#D699B6".into(),
            comment: "#7A8478".into(),
            foreground: "#D3C6AA".into(),
            guide: "#3D484D".into(),
            background: "#2D353B".into(),
            surface: "#384148".into(),
            border: "#504945".into(),

            red: "#E67E80".into(),
            green: "#A7C080".into(),
            blue: "#7FBBB3".into(),
            cyan: "#83C092".into(),
            magenta: "#D699B6".into(),
            yellow: "#DBBC7F".into(),
            white: "#D3C6AA".into(),
            black: "#2D353B".into(),
        }
    }

    pub fn vscode() -> Self {
        CustomTheme {
            keyword: "#569CD6".into(),
            keyword_bg: "#142129".into(),
            function: "#DCDCAA".into(),
            string: "#CE9178".into(),
            module: "#4EC9B0".into(),
            constant: "#B5CEA8".into(),
            comment: "#6A9955".into(),
            foreground: "#D4D4D4".into(),
            guide: "#404040".into(),
            background: "#1E1E1E".into(),
            surface: "#2D2D30".into(),
            border: "#3E3E42".into(),

            red: "#F44747".into(),
            green: "#6A9955".into(),
            blue: "#569CD6".into(),
            cyan: "#4EC9B0".into(),
            magenta: "#C586C0".into(),
            yellow: "#DCDCAA".into(),
            white: "#D4D4D4".into(),
            black: "#1E1E1E".into(),
        }
    }

    pub fn to_syntect_theme(&self) -> Theme {
        let mut settings = ThemeSettings::default();
        settings.foreground = Some(self.foreground.color);
        settings.background = Some(self.surface.color);
        settings.guide = Some(self.guide.color);

        let mut theme = Theme {
            name: None,
            author: None,
            settings,
            scopes: vec![],
        };

        fn create_selectors(selectors: &str) -> ScopeSelectors {
            ScopeSelectors::from_str(selectors).unwrap_or_default()
        }
        fn create_style(color: Color) -> StyleModifier {
            StyleModifier {
                foreground: Some(color),
                background: None,
                font_style: None,
            }
        }

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("keyword, storage.modifier, storage.type"),
            style: create_style(self.keyword.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("entity.name.function, support.function, variable.function"),
            style: create_style(self.function.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors(
                "module, struct, enum, generic, path, meta.path, entity.name.tag, support.type, meta.import-name",
            ),
            style: create_style(self.module.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors(
                "string, punctuation.string, constant.other.color, punctuation.definition.string",
            ),
            style: create_style(self.string.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("constant, keyword.other.unit, support.constant"),
            style: create_style(self.constant.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("comment, punctuation.comment, punctuation.definition.comment"),
            style: create_style(self.comment.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors(
                "variable, operator, punctuation, block, support.type.property-name, punctuation.definition, keyword.operator",
            ),
            style: create_style(self.foreground.color),
        });

        theme
    }

    pub fn to_html_style(&self) -> String {
        let root_css = format!(
            r#"
:root {{
  --keyword: {};
  --function: {};
  --type: {};
  --constant: {};
  --comment: {};
  --foreground: {};
  
  /* UI Colors */
  --background: {};
  --surface: {};
  --border: {};
}}
"#,
            self.keyword.value,
            self.function.value,
            self.module.value,
            self.constant.value,
            self.comment.value,
            self.foreground.value,
            self.background.value,
            self.surface.value,
            self.border.value
        );
        let full_css = include_str!("../../assets/style.css");
        format!("{full_css}\n\n{root_css}")
    }
}
