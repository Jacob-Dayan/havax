use crossterm::style::Color;

pub const TAB_SIZE: usize = 4;

pub fn to_ratatui_color(color: Color) -> ratatui::style::Color {
    match color {
        Color::Reset => ratatui::style::Color::Reset,
        Color::Black => ratatui::style::Color::Black,
        Color::DarkGrey => ratatui::style::Color::DarkGray,
        Color::Red => ratatui::style::Color::Red,
        Color::DarkRed => ratatui::style::Color::LightRed,
        Color::Green => ratatui::style::Color::Green,
        Color::DarkGreen => ratatui::style::Color::LightGreen,
        Color::Yellow => ratatui::style::Color::Yellow,
        Color::DarkYellow => ratatui::style::Color::LightYellow,
        Color::Blue => ratatui::style::Color::Blue,
        Color::DarkBlue => ratatui::style::Color::LightBlue,
        Color::Magenta => ratatui::style::Color::Magenta,
        Color::DarkMagenta => ratatui::style::Color::LightMagenta,
        Color::Cyan => ratatui::style::Color::Cyan,
        Color::DarkCyan => ratatui::style::Color::LightCyan,
        Color::White => ratatui::style::Color::White,
        Color::Grey => ratatui::style::Color::Gray,
        Color::Rgb { r, g, b } => ratatui::style::Color::Rgb(r, g, b),
        Color::AnsiValue(v) => ratatui::style::Color::Indexed(v),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub selection_bg: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub gutter_fg: Color,
    pub current_line_gutter: Color,

    // Syntax highlighting colors
    pub keyword: Color,
    pub r#type: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub doc_comment: Color,
    pub r#macro: Color,
    pub function: Color,
    pub attribute: Color,
    pub lifetime: Color,
    pub operator: Color,

    // Helix mode badges
    pub badge_nor_bg: Color,
    pub badge_ins_bg: Color,
    pub badge_cmd_bg: Color,
    pub badge_goto_bg: Color,
    pub badge_text: Color,

    pub picker_border: Color,
}

impl Theme {
    pub fn one_half_dark() -> Self {
        Self {
            bg: Color::Rgb {
                r: 40,
                g: 44,
                b: 52,
            }, // #282c34
            fg: Color::Rgb {
                r: 171,
                g: 178,
                b: 191,
            }, // #abb2bf
            selection_bg: Color::Rgb {
                r: 62,
                g: 68,
                b: 81,
            }, // #3e4451
            status_bg: Color::Rgb {
                r: 33,
                g: 37,
                b: 43,
            }, // #21252b
            status_fg: Color::Rgb {
                r: 171,
                g: 178,
                b: 191,
            }, // #abb2bf
            gutter_fg: Color::Rgb {
                r: 92,
                g: 99,
                b: 112,
            }, // #5c6370
            current_line_gutter: Color::Rgb {
                r: 220,
                g: 223,
                b: 228,
            }, // Bright text
            keyword: Color::Rgb {
                r: 198,
                g: 120,
                b: 221,
            }, // #c678dd (Purple)
            r#type: Color::Rgb {
                r: 229,
                g: 192,
                b: 123,
            }, // #e5c07b (Yellow)
            string: Color::Rgb {
                r: 152,
                g: 195,
                b: 121,
            }, // #98c379 (Green)
            number: Color::Rgb {
                r: 209,
                g: 154,
                b: 102,
            }, // #d19a66 (Orange)
            comment: Color::Rgb {
                r: 92,
                g: 99,
                b: 112,
            }, // #5c6370 (Gray)
            doc_comment: Color::Rgb {
                r: 86,
                g: 182,
                b: 194,
            }, // #56b6c2 (Cyan)
            r#macro: Color::Rgb {
                r: 224,
                g: 108,
                b: 117,
            }, // #e06c75 (Red)
            function: Color::Rgb {
                r: 97,
                g: 175,
                b: 239,
            }, // #61afef (Blue)
            attribute: Color::Rgb {
                r: 229,
                g: 192,
                b: 123,
            }, // #e5c07b
            lifetime: Color::Rgb {
                r: 224,
                g: 108,
                b: 117,
            }, // #e06c75
            operator: Color::Rgb {
                r: 86,
                g: 182,
                b: 194,
            }, // #56b6c2
            badge_nor_bg: Color::Rgb {
                r: 97,
                g: 175,
                b: 239,
            }, // Blue
            badge_ins_bg: Color::Rgb {
                r: 152,
                g: 195,
                b: 121,
            }, // Green
            badge_cmd_bg: Color::Rgb {
                r: 209,
                g: 154,
                b: 102,
            }, // Orange
            badge_goto_bg: Color::Rgb {
                r: 198,
                g: 120,
                b: 221,
            }, // Purple
            badge_text: Color::Rgb {
                r: 40,
                g: 44,
                b: 52,
            }, // Dark text
            picker_border: Color::Rgb {
                r: 152,
                g: 195,
                b: 121,
            },
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            bg: Color::Rgb {
                r: 24,
                g: 24,
                b: 37,
            }, // #181825
            fg: Color::Rgb {
                r: 205,
                g: 214,
                b: 244,
            }, // #cdd6f4
            selection_bg: Color::Rgb {
                r: 69,
                g: 71,
                b: 90,
            }, // #45475a
            status_bg: Color::Rgb {
                r: 49,
                g: 50,
                b: 68,
            }, // #313244
            status_fg: Color::Rgb {
                r: 245,
                g: 224,
                b: 220,
            }, // #f5e0dc
            gutter_fg: Color::Rgb {
                r: 88,
                g: 91,
                b: 112,
            }, // #585b70
            current_line_gutter: Color::Rgb {
                r: 205,
                g: 214,
                b: 244,
            },
            keyword: Color::Rgb {
                r: 203,
                g: 166,
                b: 247,
            }, // Mauve
            r#type: Color::Rgb {
                r: 249,
                g: 226,
                b: 175,
            }, // Yellow
            string: Color::Rgb {
                r: 166,
                g: 227,
                b: 161,
            }, // Green
            number: Color::Rgb {
                r: 250,
                g: 179,
                b: 135,
            }, // Peach
            comment: Color::Rgb {
                r: 108,
                g: 112,
                b: 134,
            }, // Overlay0
            doc_comment: Color::Rgb {
                r: 148,
                g: 226,
                b: 213,
            }, // Teal
            r#macro: Color::Rgb {
                r: 137,
                g: 220,
                b: 235,
            }, // Sky
            function: Color::Rgb {
                r: 137,
                g: 180,
                b: 250,
            }, // Blue
            attribute: Color::Rgb {
                r: 243,
                g: 139,
                b: 168,
            }, // Red
            lifetime: Color::Rgb {
                r: 245,
                g: 194,
                b: 231,
            }, // Pink
            operator: Color::Rgb {
                r: 137,
                g: 220,
                b: 235,
            }, // Sky
            badge_nor_bg: Color::Rgb {
                r: 137,
                g: 180,
                b: 250,
            },
            badge_ins_bg: Color::Rgb {
                r: 166,
                g: 227,
                b: 161,
            },
            badge_cmd_bg: Color::Rgb {
                r: 250,
                g: 179,
                b: 135,
            },
            badge_goto_bg: Color::Rgb {
                r: 203,
                g: 166,
                b: 247,
            },
            badge_text: Color::Rgb {
                r: 17,
                g: 17,
                b: 27,
            },
            picker_border: Color::Rgb {
                r: 166,
                g: 227,
                b: 161,
            },
        }
    }

    pub fn dracula() -> Self {
        Self {
            bg: Color::Rgb {
                r: 40,
                g: 42,
                b: 54,
            },
            fg: Color::Rgb {
                r: 248,
                g: 248,
                b: 242,
            },
            selection_bg: Color::Rgb {
                r: 68,
                g: 71,
                b: 90,
            },
            status_bg: Color::Rgb {
                r: 33,
                g: 34,
                b: 44,
            },
            status_fg: Color::Rgb {
                r: 248,
                g: 248,
                b: 242,
            },
            gutter_fg: Color::Rgb {
                r: 98,
                g: 114,
                b: 164,
            },
            current_line_gutter: Color::Rgb {
                r: 248,
                g: 248,
                b: 242,
            },
            keyword: Color::Rgb {
                r: 255,
                g: 121,
                b: 198,
            }, // Pink
            r#type: Color::Rgb {
                r: 139,
                g: 233,
                b: 253,
            }, // Cyan
            string: Color::Rgb {
                r: 241,
                g: 250,
                b: 140,
            }, // Yellow
            number: Color::Rgb {
                r: 189,
                g: 147,
                b: 249,
            }, // Purple
            comment: Color::Rgb {
                r: 98,
                g: 114,
                b: 164,
            }, // Comment
            doc_comment: Color::Rgb {
                r: 139,
                g: 233,
                b: 253,
            },
            r#macro: Color::Rgb {
                r: 80,
                g: 250,
                b: 123,
            }, // Green
            function: Color::Rgb {
                r: 80,
                g: 250,
                b: 123,
            }, // Green
            attribute: Color::Rgb {
                r: 255,
                g: 184,
                b: 108,
            }, // Orange
            lifetime: Color::Rgb {
                r: 255,
                g: 121,
                b: 198,
            },
            operator: Color::Rgb {
                r: 255,
                g: 121,
                b: 198,
            },
            badge_nor_bg: Color::Rgb {
                r: 189,
                g: 147,
                b: 249,
            },
            badge_ins_bg: Color::Rgb {
                r: 80,
                g: 250,
                b: 123,
            },
            badge_cmd_bg: Color::Rgb {
                r: 255,
                g: 184,
                b: 108,
            },
            badge_goto_bg: Color::Rgb {
                r: 255,
                g: 121,
                b: 198,
            },
            badge_text: Color::Rgb {
                r: 40,
                g: 42,
                b: 54,
            },
            picker_border: Color::Rgb {
                r: 189,
                g: 147,
                b: 249,
            },
        }
    }

    pub fn nord() -> Self {
        Self {
            bg: Color::Rgb {
                r: 46,
                g: 52,
                b: 64,
            },
            fg: Color::Rgb {
                r: 216,
                g: 222,
                b: 233,
            },
            selection_bg: Color::Rgb {
                r: 67,
                g: 76,
                b: 94,
            },
            status_bg: Color::Rgb {
                r: 41,
                g: 46,
                b: 57,
            },
            status_fg: Color::Rgb {
                r: 216,
                g: 222,
                b: 233,
            },
            gutter_fg: Color::Rgb {
                r: 76,
                g: 86,
                b: 106,
            },
            current_line_gutter: Color::Rgb {
                r: 236,
                g: 239,
                b: 244,
            },
            keyword: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            },
            r#type: Color::Rgb {
                r: 143,
                g: 188,
                b: 187,
            },
            string: Color::Rgb {
                r: 163,
                g: 190,
                b: 140,
            },
            number: Color::Rgb {
                r: 180,
                g: 142,
                b: 173,
            },
            comment: Color::Rgb {
                r: 94,
                g: 111,
                b: 142,
            },
            doc_comment: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            r#macro: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            function: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            attribute: Color::Rgb {
                r: 235,
                g: 203,
                b: 139,
            },
            lifetime: Color::Rgb {
                r: 180,
                g: 142,
                b: 173,
            },
            operator: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            },
            badge_nor_bg: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            },
            badge_ins_bg: Color::Rgb {
                r: 163,
                g: 190,
                b: 140,
            },
            badge_cmd_bg: Color::Rgb {
                r: 208,
                g: 135,
                b: 112,
            },
            badge_goto_bg: Color::Rgb {
                r: 180,
                g: 142,
                b: 173,
            },
            badge_text: Color::Rgb {
                r: 46,
                g: 52,
                b: 64,
            },
            picker_border: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
        }
    }

    /// Atom One Dark — distinct from One Half Dark with warmer tones
    pub fn one_dark() -> Self {
        Self {
            bg: Color::Rgb {
                r: 33,
                g: 37,
                b: 43,
            }, // #21252b
            fg: Color::Rgb {
                r: 171,
                g: 178,
                b: 191,
            }, // #abb2bf
            selection_bg: Color::Rgb {
                r: 55,
                g: 60,
                b: 72,
            }, // #373c48
            status_bg: Color::Rgb {
                r: 24,
                g: 27,
                b: 33,
            }, // #181b21
            status_fg: Color::Rgb {
                r: 150,
                g: 157,
                b: 170,
            }, // #969daa
            gutter_fg: Color::Rgb {
                r: 76,
                g: 82,
                b: 99,
            }, // #4c5263
            current_line_gutter: Color::Rgb {
                r: 200,
                g: 204,
                b: 212,
            },
            keyword: Color::Rgb {
                r: 198,
                g: 120,
                b: 221,
            }, // #c678dd
            r#type: Color::Rgb {
                r: 229,
                g: 192,
                b: 123,
            }, // #e5c07b
            string: Color::Rgb {
                r: 152,
                g: 195,
                b: 121,
            }, // #98c379
            number: Color::Rgb {
                r: 209,
                g: 154,
                b: 102,
            }, // #d19a66
            comment: Color::Rgb {
                r: 92,
                g: 99,
                b: 112,
            }, // #5c6370
            doc_comment: Color::Rgb {
                r: 106,
                g: 115,
                b: 130,
            },
            r#macro: Color::Rgb {
                r: 86,
                g: 182,
                b: 194,
            }, // #56b6c2
            function: Color::Rgb {
                r: 97,
                g: 175,
                b: 239,
            }, // #61afef
            attribute: Color::Rgb {
                r: 209,
                g: 154,
                b: 102,
            },
            lifetime: Color::Rgb {
                r: 198,
                g: 120,
                b: 221,
            },
            operator: Color::Rgb {
                r: 86,
                g: 182,
                b: 194,
            },
            badge_nor_bg: Color::Rgb {
                r: 97,
                g: 175,
                b: 239,
            },
            badge_ins_bg: Color::Rgb {
                r: 152,
                g: 195,
                b: 121,
            },
            badge_cmd_bg: Color::Rgb {
                r: 209,
                g: 154,
                b: 102,
            },
            badge_goto_bg: Color::Rgb {
                r: 198,
                g: 120,
                b: 221,
            },
            badge_text: Color::Rgb {
                r: 33,
                g: 37,
                b: 43,
            },
            picker_border: Color::Rgb {
                r: 97,
                g: 175,
                b: 239,
            },
        }
    }

    /// Gruvbox Dark
    pub fn gruvbox_dark() -> Self {
        Self {
            bg: Color::Rgb {
                r: 40,
                g: 40,
                b: 40,
            }, // #282828
            fg: Color::Rgb {
                r: 235,
                g: 219,
                b: 178,
            }, // #ebdbb2
            selection_bg: Color::Rgb {
                r: 80,
                g: 73,
                b: 69,
            }, // #504945
            status_bg: Color::Rgb {
                r: 29,
                g: 32,
                b: 33,
            }, // #1d2021
            status_fg: Color::Rgb {
                r: 189,
                g: 174,
                b: 147,
            }, // #bdae93
            gutter_fg: Color::Rgb {
                r: 124,
                g: 111,
                b: 100,
            }, // #7c6f64
            current_line_gutter: Color::Rgb {
                r: 235,
                g: 219,
                b: 178,
            },
            keyword: Color::Rgb {
                r: 251,
                g: 73,
                b: 52,
            }, // #fb4934 (red)
            r#type: Color::Rgb {
                r: 250,
                g: 189,
                b: 47,
            }, // #fabd2f (yellow)
            string: Color::Rgb {
                r: 184,
                g: 187,
                b: 38,
            }, // #b8bb26 (green)
            number: Color::Rgb {
                r: 211,
                g: 134,
                b: 155,
            }, // #d3869b (purple)
            comment: Color::Rgb {
                r: 146,
                g: 131,
                b: 116,
            }, // #928374
            doc_comment: Color::Rgb {
                r: 168,
                g: 153,
                b: 132,
            },
            r#macro: Color::Rgb {
                r: 131,
                g: 165,
                b: 152,
            }, // #83a598 (aqua)
            function: Color::Rgb {
                r: 131,
                g: 165,
                b: 152,
            }, // #83a598
            attribute: Color::Rgb {
                r: 254,
                g: 128,
                b: 25,
            }, // #fe8019 (orange)
            lifetime: Color::Rgb {
                r: 211,
                g: 134,
                b: 155,
            },
            operator: Color::Rgb {
                r: 235,
                g: 219,
                b: 178,
            },
            badge_nor_bg: Color::Rgb {
                r: 131,
                g: 165,
                b: 152,
            },
            badge_ins_bg: Color::Rgb {
                r: 184,
                g: 187,
                b: 38,
            },
            badge_cmd_bg: Color::Rgb {
                r: 254,
                g: 128,
                b: 25,
            },
            badge_goto_bg: Color::Rgb {
                r: 211,
                g: 134,
                b: 155,
            },
            badge_text: Color::Rgb {
                r: 40,
                g: 40,
                b: 40,
            },
            picker_border: Color::Rgb {
                r: 131,
                g: 165,
                b: 152,
            },
        }
    }

    /// One Half Light — light theme variant
    pub fn one_half_light() -> Self {
        Self {
            bg: Color::Rgb {
                r: 250,
                g: 250,
                b: 250,
            }, // #fafafa
            fg: Color::Rgb {
                r: 56,
                g: 58,
                b: 66,
            }, // #383a42
            selection_bg: Color::Rgb {
                r: 216,
                g: 222,
                b: 233,
            }, // #d8dee9
            status_bg: Color::Rgb {
                r: 230,
                g: 232,
                b: 236,
            }, // #e6e8ec
            status_fg: Color::Rgb {
                r: 56,
                g: 58,
                b: 66,
            },
            gutter_fg: Color::Rgb {
                r: 157,
                g: 165,
                b: 180,
            }, // #9da5b4
            current_line_gutter: Color::Rgb {
                r: 56,
                g: 58,
                b: 66,
            },
            keyword: Color::Rgb {
                r: 166,
                g: 38,
                b: 164,
            }, // #a626a4 (magenta)
            r#type: Color::Rgb {
                r: 193,
                g: 132,
                b: 1,
            }, // #c18401 (orange-yellow)
            string: Color::Rgb {
                r: 80,
                g: 161,
                b: 79,
            }, // #50a14f (green)
            number: Color::Rgb {
                r: 193,
                g: 132,
                b: 1,
            }, // #c18401
            comment: Color::Rgb {
                r: 160,
                g: 161,
                b: 167,
            }, // #a0a1a7
            doc_comment: Color::Rgb {
                r: 130,
                g: 131,
                b: 137,
            },
            r#macro: Color::Rgb {
                r: 1,
                g: 132,
                b: 188,
            }, // #0184bc (cyan)
            function: Color::Rgb {
                r: 64,
                g: 120,
                b: 242,
            }, // #4078f2 (blue)
            attribute: Color::Rgb {
                r: 193,
                g: 132,
                b: 1,
            },
            lifetime: Color::Rgb {
                r: 166,
                g: 38,
                b: 164,
            },
            operator: Color::Rgb {
                r: 56,
                g: 58,
                b: 66,
            },
            badge_nor_bg: Color::Rgb {
                r: 64,
                g: 120,
                b: 242,
            },
            badge_ins_bg: Color::Rgb {
                r: 80,
                g: 161,
                b: 79,
            },
            badge_cmd_bg: Color::Rgb {
                r: 193,
                g: 132,
                b: 1,
            },
            badge_goto_bg: Color::Rgb {
                r: 166,
                g: 38,
                b: 164,
            },
            badge_text: Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            picker_border: Color::Rgb {
                r: 64,
                g: 120,
                b: 242,
            },
        }
    }

    pub fn from_name(name: &str) -> Self {
        if let Some(theme) = Self::load_custom_theme(name) {
            return theme;
        }
        match name.to_lowercase().replace('_', "-").as_str() {
            "one-half-dark" | "onehalfdark" => Self::one_half_dark(),
            "one-dark" | "onedark" | "atom-one-dark" => Self::one_dark(),
            "one-half-light" | "one-light" | "light" => Self::one_half_light(),
            "catppuccin" | "catppuccin-mocha" | "mocha" => Self::catppuccin_mocha(),
            "dracula" => Self::dracula(),
            "nord" => Self::nord(),
            "gruvbox" | "gruvbox-dark" => Self::gruvbox_dark(),
            _ => Self::one_half_dark(),
        }
    }

    pub fn load_custom_theme(name: &str) -> Option<Self> {
        let clean_name = name.trim_end_matches(".toml");
        let mut search_dirs = Vec::new();

        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            let home_path = std::path::PathBuf::from(home);
            search_dirs.push(home_path.join(".config/havax/themes"));
            search_dirs.push(home_path.join(".havax/themes"));
            search_dirs.push(home_path.join(".config/helix/themes"));
            search_dirs.push(home_path.join(".helix/themes"));
        }
        if let Ok(cur) = std::env::current_dir() {
            search_dirs.push(cur.join(".havax/themes"));
            search_dirs.push(cur.join(".helix/themes"));
            search_dirs.push(cur.join("themes"));
        }

        for dir in search_dirs {
            let direct_file = dir.join(format!("{clean_name}.toml"));
            if let Ok(content) = std::fs::read_to_string(&direct_file)
                && let Some(theme) = Self::from_toml_str(&content)
            {
                return Some(theme);
            }
            let direct_no_ext = dir.join(clean_name);
            if direct_no_ext.is_file()
                && let Ok(content) = std::fs::read_to_string(&direct_no_ext)
                && let Some(theme) = Self::from_toml_str(&content)
            {
                return Some(theme);
            }
        }
        None
    }

    pub fn from_toml_str(content: &str) -> Option<Self> {
        let table: toml::Table = toml::from_str(content).ok()?;

        let mut theme = if let Some(inherited) = table.get("inherits").and_then(|v| v.as_str()) {
            Self::from_name(inherited)
        } else {
            Self::default()
        };

        let mut palette = std::collections::HashMap::new();
        if let Some(pal_table) = table.get("palette").and_then(|v| v.as_table()) {
            for (k, v) in pal_table {
                if let Some(s) = v.as_str() {
                    palette.insert(k.as_str(), s);
                }
            }
        }

        for (k, v) in &table {
            if k == "inherits" || k == "palette" {
                continue;
            }
            let (fg_opt, bg_opt) = parse_entry_colors(v, &palette);
            match k.as_str() {
                "ui.background" => {
                    if let Some(c) = bg_opt.or(fg_opt) {
                        theme.bg = c;
                    }
                }
                "ui.text" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.fg = c;
                    }
                }
                "ui.selection" | "ui.selection.primary" => {
                    if let Some(c) = bg_opt.or(fg_opt) {
                        theme.selection_bg = c;
                    }
                }
                "ui.statusline" => {
                    if let Some(c) = bg_opt {
                        theme.status_bg = c;
                    }
                    if let Some(c) = fg_opt {
                        theme.status_fg = c;
                    }
                }
                "ui.linenr" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.gutter_fg = c;
                    }
                }
                "ui.linenr.selected" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.current_line_gutter = c;
                    }
                }
                "keyword" | "keyword.control" | "keyword.storage" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.keyword = c;
                    }
                }
                "type" | "type.builtin" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.r#type = c;
                    }
                }
                "string" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.string = c;
                    }
                }
                "number" | "constant.numeric" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.number = c;
                    }
                }
                "comment" | "comment.line" | "comment.block" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.comment = c;
                    }
                }
                "comment.doc" | "doc_comment" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.doc_comment = c;
                    }
                }
                "function" | "function.method" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.function = c;
                    }
                }
                "function.macro" | "macro" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.r#macro = c;
                    }
                }
                "attribute" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.attribute = c;
                    }
                }
                "operator" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.operator = c;
                    }
                }
                "ui.popup" | "ui.menu" => {
                    if let Some(c) = fg_opt.or(bg_opt) {
                        theme.picker_border = c;
                    }
                }
                _ => {}
            }
        }

        Some(theme)
    }
}

fn parse_color_val(
    val: &toml::Value,
    palette: &std::collections::HashMap<&str, &str>,
) -> Option<Color> {
    if let Some(s) = val.as_str() {
        let resolved = palette.get(s).copied().unwrap_or(s);
        parse_color_string(resolved)
    } else {
        None
    }
}

fn parse_color_string(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb { r, g, b });
        } else if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some(Color::Rgb { r, g, b });
        }
    }
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::DarkRed),
        "green" => Some(Color::DarkGreen),
        "yellow" => Some(Color::DarkYellow),
        "blue" => Some(Color::DarkBlue),
        "magenta" | "purple" => Some(Color::DarkMagenta),
        "cyan" => Some(Color::DarkCyan),
        "white" | "gray" | "grey" => Some(Color::Grey),
        "light_red" | "lightred" => Some(Color::Red),
        "light_green" | "lightgreen" => Some(Color::Green),
        "light_yellow" | "lightyellow" => Some(Color::Yellow),
        "light_blue" | "lightblue" => Some(Color::Blue),
        "light_magenta" | "lightmagenta" => Some(Color::Magenta),
        "light_cyan" | "lightcyan" => Some(Color::Cyan),
        "light_white" | "lightwhite" => Some(Color::White),
        _ => None,
    }
}

fn parse_entry_colors(
    val: &toml::Value,
    palette: &std::collections::HashMap<&str, &str>,
) -> (Option<Color>, Option<Color>) {
    if let Some(table) = val.as_table() {
        let fg = table.get("fg").and_then(|v| parse_color_val(v, palette));
        let bg = table.get("bg").and_then(|v| parse_color_val(v, palette));
        (fg, bg)
    } else if let Some(c) = parse_color_val(val, palette) {
        (Some(c), None)
    } else {
        (None, None)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::one_half_dark()
    }
}

// Global default theme references for backward compatibility
pub const BG_COLOR: Color = Color::Rgb {
    r: 40,
    g: 44,
    b: 52,
};
pub const FG_COLOR: Color = Color::Rgb {
    r: 171,
    g: 178,
    b: 191,
};
pub const SELECTION_BG: Color = Color::Rgb {
    r: 62,
    g: 68,
    b: 81,
};
pub const STATUS_BG: Color = Color::Rgb {
    r: 33,
    g: 37,
    b: 43,
};
pub const STATUS_FG: Color = Color::Rgb {
    r: 171,
    g: 178,
    b: 191,
};
pub const GUTTER_FG: Color = Color::Rgb {
    r: 92,
    g: 99,
    b: 112,
};
pub const CURRENT_LINE_GUTTER: Color = Color::Rgb {
    r: 220,
    g: 223,
    b: 228,
};

pub const COLOR_KEYWORD: Color = Color::Rgb {
    r: 198,
    g: 120,
    b: 221,
};
pub const COLOR_TYPE: Color = Color::Rgb {
    r: 229,
    g: 192,
    b: 123,
};
pub const COLOR_STRING: Color = Color::Rgb {
    r: 152,
    g: 195,
    b: 121,
};
pub const COLOR_NUMBER: Color = Color::Rgb {
    r: 209,
    g: 154,
    b: 102,
};
pub const COLOR_COMMENT: Color = Color::Rgb {
    r: 92,
    g: 99,
    b: 112,
};
pub const COLOR_DOC_COMMENT: Color = Color::Rgb {
    r: 86,
    g: 182,
    b: 194,
};
pub const COLOR_MACRO: Color = Color::Rgb {
    r: 224,
    g: 108,
    b: 117,
};
pub const COLOR_FN: Color = Color::Rgb {
    r: 97,
    g: 175,
    b: 239,
};
pub const COLOR_MODULE: Color = Color::Rgb {
    r: 229,
    g: 192,
    b: 123,
};
pub const COLOR_ATTR: Color = Color::Rgb {
    r: 229,
    g: 192,
    b: 123,
};
pub const COLOR_LIFETIME: Color = Color::Rgb {
    r: 224,
    g: 108,
    b: 117,
};
pub const COLOR_OPERATOR: Color = Color::Rgb {
    r: 86,
    g: 182,
    b: 194,
};

pub const RAINBOW_COLORS: [Color; 6] = [
    Color::Rgb {
        r: 249,
        g: 226,
        b: 175,
    }, // Gold / Yellow
    Color::Rgb {
        r: 203,
        g: 166,
        b: 247,
    }, // Lavender / Purple
    Color::Rgb {
        r: 137,
        g: 220,
        b: 235,
    }, // Cyan / Sky
    Color::Rgb {
        r: 137,
        g: 180,
        b: 250,
    }, // Blue
    Color::Rgb {
        r: 245,
        g: 194,
        b: 231,
    }, // Pink / Peach
    Color::Rgb {
        r: 166,
        g: 227,
        b: 161,
    }, // Green
];

pub const BADGE_NOR_BG: Color = Color::Rgb {
    r: 97,
    g: 175,
    b: 239,
};
pub const BADGE_INS_BG: Color = Color::Rgb {
    r: 152,
    g: 195,
    b: 121,
};
pub const BADGE_CMD_BG: Color = Color::Rgb {
    r: 209,
    g: 154,
    b: 102,
};
pub const BADGE_GOTO_BG: Color = Color::Rgb {
    r: 198,
    g: 120,
    b: 221,
};
pub const BADGE_TEXT: Color = Color::Rgb {
    r: 40,
    g: 44,
    b: 52,
};
pub const PICKER_BORDER: Color = Color::Rgb {
    r: 152,
    g: 195,
    b: 121,
};
