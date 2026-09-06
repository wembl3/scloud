use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::SystemTime;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeName {
    Matugen,
    Btop,
    CatppuccinMocha,
    Dracula,
    TokyoNight,
    Nord,
    Gruvbox,
    Cyberpunk,
    Monokai,
    System,
    Default,
}

impl Default for ThemeName {
    fn default() -> Self {
        ThemeName::Btop
    }
}

#[derive(Clone)]
pub struct ThemeColors {
    pub name: &'static str,
    pub title: &'static str,
    pub bg: Color,
    pub bg_widget: Color,
    pub border: Color,
    pub border_active: Color,
    pub text: Color,
    pub text_dim: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub gauge_bg: Color,
    pub gauge_fg: Color,
    pub visualizer_low: Color,
    pub visualizer_mid: Color,
    pub visualizer_high: Color,
}

pub fn matugen_candidate_paths() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let home_path = PathBuf::from(home);
    vec![
        home_path.join(".local/state/serpantinum/qs_matugen_colors.json"),
        home_path.join(".local/state/serpantinum/qs_colors.json"),
        home_path.join(".config/sc-player/matugen.json"),
        home_path.join(".config/kitty/colors.conf"),
        home_path.join(".config/kitty/kitty-matugen-colors.conf"),
        home_path.join(".config/nvim/matugen_colors.lua"),
    ]
}

pub fn get_matugen_mtime() -> Option<SystemTime> {
    let mut latest: Option<SystemTime> = None;
    for p in matugen_candidate_paths() {
        if let Ok(meta) = fs::metadata(&p) {
            if let Ok(m) = meta.modified() {
                latest = match latest {
                    None => Some(m),
                    Some(cur) => Some(std::cmp::max(cur, m)),
                };
            }
        }
    }
    latest
}

pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

#[derive(Debug, Deserialize, Default)]
struct MatugenRawJson {
    bg: Option<String>,
    bg_widget: Option<String>,
    border: Option<String>,
    border_active: Option<String>,
    text: Option<String>,
    text_dim: Option<String>,
    primary: Option<String>,
    secondary: Option<String>,
    accent: Option<String>,
    success: Option<String>,
    warning: Option<String>,
    error: Option<String>,
    highlight_bg: Option<String>,
    highlight_fg: Option<String>,
    gauge_bg: Option<String>,
    gauge_fg: Option<String>,
    visualizer_low: Option<String>,
    visualizer_mid: Option<String>,
    visualizer_high: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct QsRawJson {
    base: Option<String>,
    crust: Option<String>,
    surface0: Option<String>,
    surface1: Option<String>,
    surface2: Option<String>,
    text: Option<String>,
    subtext0: Option<String>,
    subtext1: Option<String>,
    blue: Option<String>,
    sapphire: Option<String>,
    peach: Option<String>,
    green: Option<String>,
    red: Option<String>,
    mauve: Option<String>,
    teal: Option<String>,
}

fn parse_candidate_path(path: &PathBuf) -> Option<ThemeColors> {
    let content = fs::read_to_string(path).ok()?;
    let path_str = path.to_string_lossy();

    if path_str.ends_with(".json") {
        if let Ok(raw) = serde_json::from_str::<MatugenRawJson>(&content) {
            if raw.primary.is_some() || raw.bg.is_some() {
                let bg = raw.bg.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(39, 29, 26));
                let bg_widget = raw.bg_widget.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(50, 40, 36));
                let border = raw.border.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(61, 50, 47));
                let border_active = raw.border_active.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(255, 181, 156));
                let text = raw.text.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(241, 223, 217));
                let text_dim = raw.text_dim.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(160, 141, 135));
                let primary = raw.primary.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(255, 181, 156));
                let secondary = raw.secondary.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(231, 189, 176));
                let accent = raw.accent.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(214, 198, 141));
                let success = raw.success.as_deref().and_then(parse_hex_color).unwrap_or(primary);
                let warning = raw.warning.as_deref().and_then(parse_hex_color).unwrap_or(accent);
                let error = raw.error.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(255, 180, 171));
                let highlight_bg = raw.highlight_bg.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(114, 53, 31));
                let highlight_fg = raw.highlight_fg.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(241, 223, 217));
                let gauge_bg = raw.gauge_bg.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(20, 12, 10));
                let gauge_fg = raw.gauge_fg.as_deref().and_then(parse_hex_color).unwrap_or(primary);
                let visualizer_low = raw.visualizer_low.as_deref().and_then(parse_hex_color).unwrap_or(primary);
                let visualizer_mid = raw.visualizer_mid.as_deref().and_then(parse_hex_color).unwrap_or(accent);
                let visualizer_high = raw.visualizer_high.as_deref().and_then(parse_hex_color).unwrap_or(error);

                return Some(ThemeColors {
                    name: "Matugen (Wallpaper)",
                    title: "Matugen",
                    bg,
                    bg_widget,
                    border,
                    border_active,
                    text,
                    text_dim,
                    primary,
                    secondary,
                    accent,
                    success,
                    warning,
                    error,
                    highlight_bg,
                    highlight_fg,
                    gauge_bg,
                    gauge_fg,
                    visualizer_low,
                    visualizer_mid,
                    visualizer_high,
                });
            }
        }

        if let Ok(qs) = serde_json::from_str::<QsRawJson>(&content) {
            if qs.blue.is_some() || qs.surface0.is_some() || qs.base.is_some() {
                let primary = qs.blue.as_deref().or(qs.mauve.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(255, 181, 156));
                let secondary = qs.green.as_deref().or(qs.teal.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(231, 189, 176));
                let accent = qs.peach.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(214, 198, 141));
                let bg = qs.surface0.as_deref().or(qs.base.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(39, 29, 26));
                let bg_widget = qs.surface1.as_deref().or(qs.surface0.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(50, 40, 36));
                let border = qs.surface2.as_deref().or(qs.subtext1.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(61, 50, 47));
                let border_active = primary;
                let text = qs.text.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(241, 223, 217));
                let text_dim = qs.subtext1.as_deref().or(qs.subtext0.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(160, 141, 135));
                let success = primary;
                let warning = accent;
                let error = qs.red.as_deref().and_then(parse_hex_color).unwrap_or(Color::Rgb(255, 180, 171));
                let highlight_bg = qs.sapphire.as_deref().or(qs.surface2.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(114, 53, 31));
                let highlight_fg = text;
                let gauge_bg = qs.crust.as_deref().or(qs.base.as_deref()).and_then(parse_hex_color).unwrap_or(Color::Rgb(20, 12, 10));
                let gauge_fg = primary;
                let visualizer_low = primary;
                let visualizer_mid = accent;
                let visualizer_high = error;

                return Some(ThemeColors {
                    name: "Matugen (Wallpaper)",
                    title: "Matugen",
                    bg,
                    bg_widget,
                    border,
                    border_active,
                    text,
                    text_dim,
                    primary,
                    secondary,
                    accent,
                    success,
                    warning,
                    error,
                    highlight_bg,
                    highlight_fg,
                    gauge_bg,
                    gauge_fg,
                    visualizer_low,
                    visualizer_mid,
                    visualizer_high,
                });
            }
        }
    }

    if path_str.ends_with(".conf") {
        let mut map = HashMap::new();
        for line in content.lines() {
            let t = line.trim();
            if t.starts_with('#') || t.is_empty() {
                continue;
            }
            let parts: Vec<&str> = t.split_whitespace().collect();
            if parts.len() >= 2 {
                map.insert(parts[0].to_string(), parts[1].to_string());
            }
        }

        let bg = map.get("background").and_then(|h| parse_hex_color(h)).unwrap_or(Color::Rgb(39, 29, 26));
        let bg_widget = map.get("selection_background")
            .or_else(|| map.get("color0"))
            .or_else(|| map.get("active_tab_background"))
            .and_then(|h| parse_hex_color(h))
            .unwrap_or(Color::Rgb(50, 40, 36));
        let border = map.get("inactive_border_color")
            .or_else(|| map.get("color0"))
            .and_then(|h| parse_hex_color(h))
            .unwrap_or(Color::Rgb(61, 50, 47));
        let primary = map.get("cursor")
            .or_else(|| map.get("active_border_color"))
            .or_else(|| map.get("color2"))
            .and_then(|h| parse_hex_color(h))
            .unwrap_or(Color::Rgb(255, 181, 156));
        let border_active = primary;
        let text = map.get("foreground")
            .or_else(|| map.get("color15"))
            .and_then(|h| parse_hex_color(h))
            .unwrap_or(Color::Rgb(241, 223, 217));
        let text_dim = map.get("color8")
            .or_else(|| map.get("color7"))
            .and_then(|h| parse_hex_color(h))
            .unwrap_or(Color::Rgb(160, 141, 135));
        let secondary = map.get("color4")
            .or_else(|| map.get("color12"))
            .and_then(|h| parse_hex_color(h))
            .unwrap_or(Color::Rgb(231, 189, 176));
        let accent = map.get("url_color")
            .or_else(|| map.get("color3"))
            .and_then(|h| parse_hex_color(h))
            .unwrap_or(Color::Rgb(214, 198, 141));
        let success = map.get("color2").or_else(|| map.get("color10")).and_then(|h| parse_hex_color(h)).unwrap_or(primary);
        let warning = map.get("color3").or_else(|| map.get("color11")).and_then(|h| parse_hex_color(h)).unwrap_or(accent);
        let error = map.get("color1").or_else(|| map.get("bell_border_color")).and_then(|h| parse_hex_color(h)).unwrap_or(Color::Rgb(255, 180, 171));
        let highlight_bg = map.get("selection_background").and_then(|h| parse_hex_color(h)).unwrap_or(Color::Rgb(114, 53, 31));
        let highlight_fg = map.get("selection_foreground").and_then(|h| parse_hex_color(h)).unwrap_or(Color::Rgb(255, 219, 207));

        return Some(ThemeColors {
            name: "Matugen (Wallpaper)",
            title: "Matugen",
            bg,
            bg_widget,
            border,
            border_active,
            text,
            text_dim,
            primary,
            secondary,
            accent,
            success,
            warning,
            error,
            highlight_bg,
            highlight_fg,
            gauge_bg: bg,
            gauge_fg: primary,
            visualizer_low: primary,
            visualizer_mid: accent,
            visualizer_high: error,
        });
    }

    None
}

fn parse_matugen_from_disk() -> ThemeColors {
    let mut candidates: Vec<(SystemTime, PathBuf)> = Vec::new();
    for p in matugen_candidate_paths() {
        if let Ok(meta) = fs::metadata(&p) {
            if let Ok(m) = meta.modified() {
                candidates.push((m, p));
            }
        }
    }
    // Sort descending by modified time (most recent first)
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    for (_mtime, path) in candidates {
        if let Some(colors) = parse_candidate_path(&path) {
            return colors;
        }
    }

    // Default warm Material You palette
    ThemeColors {
        name: "Matugen (Wallpaper)",
        title: "Matugen",
        bg: Color::Rgb(39, 29, 26),
        bg_widget: Color::Rgb(50, 40, 36),
        border: Color::Rgb(61, 50, 47),
        border_active: Color::Rgb(255, 181, 156),
        text: Color::Rgb(241, 223, 217),
        text_dim: Color::Rgb(160, 141, 135),
        primary: Color::Rgb(255, 181, 156),
        secondary: Color::Rgb(231, 189, 176),
        accent: Color::Rgb(214, 198, 141),
        success: Color::Rgb(255, 181, 156),
        warning: Color::Rgb(214, 198, 141),
        error: Color::Rgb(255, 180, 171),
        highlight_bg: Color::Rgb(114, 53, 31),
        highlight_fg: Color::Rgb(241, 223, 217),
        gauge_bg: Color::Rgb(20, 12, 10),
        gauge_fg: Color::Rgb(255, 181, 156),
        visualizer_low: Color::Rgb(255, 181, 156),
        visualizer_mid: Color::Rgb(214, 198, 141),
        visualizer_high: Color::Rgb(255, 180, 171),
    }
}

static MATUGEN_CACHE: RwLock<Option<(Option<SystemTime>, ThemeColors)>> = RwLock::new(None);

pub fn load_matugen_colors() -> ThemeColors {
    let current_mtime = get_matugen_mtime();
    if let Ok(guard) = MATUGEN_CACHE.read() {
        if let Some((cached_mtime, ref colors)) = *guard {
            if cached_mtime == current_mtime && current_mtime.is_some() {
                return colors.clone();
            }
        }
    }

    let fresh = parse_matugen_from_disk();
    if let Ok(mut guard) = MATUGEN_CACHE.write() {
        *guard = Some((current_mtime, fresh.clone()));
    }
    fresh
}

impl ThemeName {
    pub const ALL: [ThemeName; 10] = [
        ThemeName::Matugen,
        ThemeName::Btop,
        ThemeName::CatppuccinMocha,
        ThemeName::Dracula,
        ThemeName::TokyoNight,
        ThemeName::Nord,
        ThemeName::Gruvbox,
        ThemeName::Cyberpunk,
        ThemeName::Monokai,
        ThemeName::System,
    ];

    pub const PRESETS: [ThemeName; 9] = [
        ThemeName::Btop,
        ThemeName::CatppuccinMocha,
        ThemeName::Dracula,
        ThemeName::TokyoNight,
        ThemeName::Nord,
        ThemeName::Gruvbox,
        ThemeName::Cyberpunk,
        ThemeName::Monokai,
        ThemeName::System,
    ];

    pub fn next(&self) -> Self {
        let idx = Self::ALL.iter().position(|t| t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(&self) -> Self {
        let idx = Self::ALL.iter().position(|t| t == self).unwrap_or(0);
        if idx == 0 {
            Self::ALL[Self::ALL.len() - 1]
        } else {
            Self::ALL[idx - 1]
        }
    }

    pub fn next_preset(&self) -> Self {
        let idx = Self::PRESETS.iter().position(|t| t == self).unwrap_or(0);
        Self::PRESETS[(idx + 1) % Self::PRESETS.len()]
    }

    pub fn prev_preset(&self) -> Self {
        let idx = Self::PRESETS.iter().position(|t| t == self).unwrap_or(0);
        if idx == 0 {
            Self::PRESETS[Self::PRESETS.len() - 1]
        } else {
            Self::PRESETS[idx - 1]
        }
    }

    pub fn is_matugen(&self) -> bool {
        matches!(self, ThemeName::Matugen)
    }

    pub fn colors(&self) -> ThemeColors {
        match self {
            ThemeName::Matugen => load_matugen_colors(),
            ThemeName::Btop | ThemeName::Default => ThemeColors {
                name: "btop Default",
                title: "btop Navy",
                bg: Color::Rgb(21, 24, 40),              // #151828
                bg_widget: Color::Rgb(27, 31, 52),       // #1b1f34
                border: Color::Rgb(61, 76, 117),         // #3d4c75
                border_active: Color::Rgb(125, 207, 255),// #7dcfff
                text: Color::Rgb(207, 201, 194),         // #cfc9c2
                text_dim: Color::Rgb(86, 95, 137),       // #565f89
                primary: Color::Rgb(125, 207, 255),      // #7dcfff
                secondary: Color::Rgb(187, 154, 247),    // #bb9af7
                accent: Color::Rgb(255, 121, 198),       // #ff79c6
                success: Color::Rgb(76, 217, 123),       // #4cd97b
                warning: Color::Rgb(224, 175, 104),      // #e0af68
                error: Color::Rgb(247, 118, 142),        // #f7768e
                highlight_bg: Color::Rgb(43, 51, 82),    // #2b3352
                highlight_fg: Color::Rgb(125, 207, 255),
                gauge_bg: Color::Rgb(21, 24, 40),
                gauge_fg: Color::Rgb(125, 207, 255),
                visualizer_low: Color::Rgb(125, 207, 255),
                visualizer_mid: Color::Rgb(187, 154, 247),
                visualizer_high: Color::Rgb(255, 121, 198),
            },
            ThemeName::CatppuccinMocha => ThemeColors {
                name: "Catppuccin Mocha",
                title: "Catppuccin Mocha",
                bg: Color::Rgb(30, 30, 46),              // #1e1e2e Base
                bg_widget: Color::Rgb(24, 24, 37),       // #181825 Mantle
                border: Color::Rgb(137, 180, 250),       // Blue
                border_active: Color::Rgb(203, 166, 247),// Mauve
                text: Color::Rgb(205, 214, 244),         // Text
                text_dim: Color::Rgb(108, 112, 134),     // Overlay0
                primary: Color::Rgb(137, 180, 250),      // Blue
                secondary: Color::Rgb(245, 194, 231),    // Pink
                accent: Color::Rgb(203, 166, 247),       // Mauve
                success: Color::Rgb(166, 227, 161),      // Green
                warning: Color::Rgb(250, 179, 135),      // Peach
                error: Color::Rgb(243, 139, 168),        // Red
                highlight_bg: Color::Rgb(49, 50, 68),    // Surface0
                highlight_fg: Color::Rgb(203, 166, 247), // Mauve
                gauge_bg: Color::Rgb(49, 50, 68),
                gauge_fg: Color::Rgb(137, 180, 250),
                visualizer_low: Color::Rgb(137, 180, 250),
                visualizer_mid: Color::Rgb(203, 166, 247),
                visualizer_high: Color::Rgb(245, 194, 231),
            },
            ThemeName::Dracula => ThemeColors {
                name: "Dracula",
                title: "Dracula Dark",
                bg: Color::Rgb(40, 42, 54),              // #282a36
                bg_widget: Color::Rgb(33, 34, 44),       // #21222c
                border: Color::Rgb(189, 147, 249),       // Purple
                border_active: Color::Rgb(255, 121, 198),// Pink
                text: Color::Rgb(248, 248, 242),         // White
                text_dim: Color::Rgb(98, 114, 164),      // Comment
                primary: Color::Rgb(189, 147, 249),      // Purple
                secondary: Color::Rgb(139, 233, 253),    // Cyan
                accent: Color::Rgb(255, 121, 198),       // Pink
                success: Color::Rgb(80, 250, 123),       // Green
                warning: Color::Rgb(255, 184, 108),      // Orange
                error: Color::Rgb(255, 85, 85),          // Red
                highlight_bg: Color::Rgb(68, 71, 90),    // Current line
                highlight_fg: Color::Rgb(139, 233, 253), // Cyan
                gauge_bg: Color::Rgb(40, 42, 54),
                gauge_fg: Color::Rgb(189, 147, 249),
                visualizer_low: Color::Rgb(139, 233, 253),
                visualizer_mid: Color::Rgb(189, 147, 249),
                visualizer_high: Color::Rgb(255, 121, 198),
            },
            ThemeName::TokyoNight => ThemeColors {
                name: "Tokyo Night",
                title: "Tokyo Night",
                bg: Color::Rgb(26, 27, 38),              // #1a1b26
                bg_widget: Color::Rgb(22, 22, 30),       // #16161e
                border: Color::Rgb(122, 162, 247),       // Blue
                border_active: Color::Rgb(187, 154, 247),// Purple
                text: Color::Rgb(192, 202, 245),         // Foreground
                text_dim: Color::Rgb(86, 95, 137),       // Dark Gray
                primary: Color::Rgb(122, 162, 247),      // Blue
                secondary: Color::Rgb(125, 207, 255),    // Cyan
                accent: Color::Rgb(187, 154, 247),       // Purple
                success: Color::Rgb(158, 206, 106),      // Green
                warning: Color::Rgb(224, 175, 104),      // Yellow/Orange
                error: Color::Rgb(247, 118, 142),        // Red
                highlight_bg: Color::Rgb(40, 52, 90),    // Dark Blue
                highlight_fg: Color::Rgb(125, 207, 255),
                gauge_bg: Color::Rgb(30, 32, 48),
                gauge_fg: Color::Rgb(122, 162, 247),
                visualizer_low: Color::Rgb(125, 207, 255),
                visualizer_mid: Color::Rgb(187, 154, 247),
                visualizer_high: Color::Rgb(247, 118, 142),
            },
            ThemeName::Nord => ThemeColors {
                name: "Nord",
                title: "Nordic Frost",
                bg: Color::Rgb(46, 52, 64),              // #2e3440 Polar Night
                bg_widget: Color::Rgb(40, 46, 57),       // #282e39
                border: Color::Rgb(136, 192, 208),       // Frost Cyan
                border_active: Color::Rgb(129, 161, 193),// Frost Blue
                text: Color::Rgb(236, 239, 244),         // Snow Storm
                text_dim: Color::Rgb(76, 86, 106),       // Polar Night
                primary: Color::Rgb(136, 192, 208),      // Frost Cyan
                secondary: Color::Rgb(143, 188, 187),    // Frost Teal
                accent: Color::Rgb(180, 142, 173),       // Aurora Purple
                success: Color::Rgb(163, 190, 140),      // Aurora Green
                warning: Color::Rgb(235, 203, 139),      // Aurora Yellow
                error: Color::Rgb(191, 97, 106),         // Aurora Red
                highlight_bg: Color::Rgb(59, 66, 82),    // Polar Night
                highlight_fg: Color::Rgb(136, 192, 208),
                gauge_bg: Color::Rgb(46, 52, 64),
                gauge_fg: Color::Rgb(136, 192, 208),
                visualizer_low: Color::Rgb(136, 192, 208),
                visualizer_mid: Color::Rgb(129, 161, 193),
                visualizer_high: Color::Rgb(180, 142, 173),
            },
            ThemeName::Gruvbox => ThemeColors {
                name: "Gruvbox Dark",
                title: "Gruvbox Dark",
                bg: Color::Rgb(40, 40, 40),              // #282828 Dark0
                bg_widget: Color::Rgb(29, 32, 33),       // #1d2021 Dark0_hard
                border: Color::Rgb(250, 189, 47),        // Bright Yellow
                border_active: Color::Rgb(254, 128, 25), // Bright Orange
                text: Color::Rgb(235, 219, 178),         // Light Foreground
                text_dim: Color::Rgb(146, 131, 116),     // Gray
                primary: Color::Rgb(250, 189, 47),       // Yellow
                secondary: Color::Rgb(142, 192, 124),    // Aqua
                accent: Color::Rgb(211, 134, 155),       // Purple
                success: Color::Rgb(184, 187, 38),       // Green
                warning: Color::Rgb(254, 128, 25),       // Orange
                error: Color::Rgb(251, 73, 52),          // Red
                highlight_bg: Color::Rgb(60, 56, 54),    // Dark 2
                highlight_fg: Color::Rgb(250, 189, 47),
                gauge_bg: Color::Rgb(40, 40, 40),
                gauge_fg: Color::Rgb(250, 189, 47),
                visualizer_low: Color::Rgb(142, 192, 124),
                visualizer_mid: Color::Rgb(250, 189, 47),
                visualizer_high: Color::Rgb(254, 128, 25),
            },
            ThemeName::Cyberpunk => ThemeColors {
                name: "Cyberpunk",
                title: "Cyberpunk Neon",
                bg: Color::Rgb(16, 5, 32),               // #100520 Deep Neon Purple
                bg_widget: Color::Rgb(24, 10, 48),       // #180a30
                border: Color::Rgb(0, 240, 255),         // Neon Cyan
                border_active: Color::Rgb(255, 0, 127),  // Hot Pink
                text: Color::Rgb(0, 240, 255),           // Neon Cyan
                text_dim: Color::Rgb(160, 60, 200),      // Purple
                primary: Color::Rgb(0, 240, 255),        // Neon Cyan
                secondary: Color::Rgb(255, 0, 127),      // Hot Pink
                accent: Color::Rgb(255, 230, 0),         // Electric Yellow
                success: Color::Rgb(0, 255, 102),        // Bright Green
                warning: Color::Rgb(255, 230, 0),        // Electric Yellow
                error: Color::Rgb(255, 0, 85),           // Neon Red
                highlight_bg: Color::Rgb(58, 20, 95),    // Vivid Purple
                highlight_fg: Color::Rgb(255, 230, 0),
                gauge_bg: Color::Rgb(24, 10, 48),
                gauge_fg: Color::Rgb(0, 240, 255),
                visualizer_low: Color::Rgb(0, 240, 255),
                visualizer_mid: Color::Rgb(255, 230, 0),
                visualizer_high: Color::Rgb(255, 0, 127),
            },
            ThemeName::Monokai => ThemeColors {
                name: "Monokai Pro",
                title: "Monokai Pro",
                bg: Color::Rgb(39, 40, 34),              // #272822
                bg_widget: Color::Rgb(30, 31, 26),       // #1e1f1a
                border: Color::Rgb(166, 226, 46),        // Green
                border_active: Color::Rgb(249, 38, 114), // Pink
                text: Color::Rgb(248, 248, 242),         // White
                text_dim: Color::Rgb(117, 113, 94),      // Gray
                primary: Color::Rgb(102, 217, 239),      // Blue
                secondary: Color::Rgb(174, 129, 255),    // Purple
                accent: Color::Rgb(249, 38, 114),        // Pink
                success: Color::Rgb(166, 226, 46),       // Green
                warning: Color::Rgb(230, 219, 116),      // Yellow
                error: Color::Rgb(249, 38, 114),         // Red
                highlight_bg: Color::Rgb(62, 61, 50),
                highlight_fg: Color::Rgb(166, 226, 46),
                gauge_bg: Color::Rgb(39, 40, 34),
                gauge_fg: Color::Rgb(102, 217, 239),
                visualizer_low: Color::Rgb(102, 217, 239),
                visualizer_mid: Color::Rgb(174, 129, 255),
                visualizer_high: Color::Rgb(249, 38, 114),
            },
            ThemeName::System => ThemeColors {
                name: "System / Terminal",
                title: "System Theme",
                bg: Color::Reset,
                bg_widget: Color::Reset,
                border: Color::DarkGray,
                border_active: Color::Cyan,
                text: Color::Reset,
                text_dim: Color::DarkGray,
                primary: Color::Cyan,
                secondary: Color::Yellow,
                accent: Color::Magenta,
                success: Color::Green,
                warning: Color::Yellow,
                error: Color::Red,
                highlight_bg: Color::Rgb(65, 70, 89),
                highlight_fg: Color::Rgb(221, 225, 249),
                gauge_bg: Color::Reset,
                gauge_fg: Color::Cyan,
                visualizer_low: Color::Cyan,
                visualizer_mid: Color::Magenta,
                visualizer_high: Color::LightMagenta,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_cycling() {
        let first = ThemeName::CatppuccinMocha;
        let mut curr = first;
        for _ in 0..ThemeName::ALL.len() {
            curr = curr.next();
        }
        assert_eq!(curr, first);

        let mut prev_curr = first;
        for _ in 0..ThemeName::ALL.len() {
            prev_curr = prev_curr.prev();
        }
        assert_eq!(prev_curr, first);
    }

    #[test]
    fn test_all_themes_have_colors() {
        for theme in ThemeName::ALL {
            let c = theme.colors();
            assert!(!c.name.is_empty());
            assert!(!c.title.is_empty());
        }
    }

    #[test]
    fn test_theme_serde() {
        for theme in ThemeName::ALL {
            let json = serde_json::to_string(&theme).unwrap();
            let de: ThemeName = serde_json::from_str(&json).unwrap();
            assert_eq!(theme, de);
        }
    }

    #[test]
    fn test_preset_cycling() {
        let first = ThemeName::Btop;
        let mut curr = first;
        for _ in 0..ThemeName::PRESETS.len() {
            curr = curr.next_preset();
        }
        assert_eq!(curr, first);

        let mut prev_curr = first;
        for _ in 0..ThemeName::PRESETS.len() {
            prev_curr = prev_curr.prev_preset();
        }
        assert_eq!(prev_curr, first);
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#111318"), Some(Color::Rgb(17, 19, 24)));
        assert_eq!(parse_hex_color("a5c8fe"), Some(Color::Rgb(165, 200, 254)));
        assert_eq!(parse_hex_color("invalid"), None);
    }

    #[test]
    fn test_is_matugen() {
        assert!(ThemeName::Matugen.is_matugen());
        for p in ThemeName::PRESETS {
            assert!(!p.is_matugen());
        }
    }

    #[test]
    fn test_matugen_colors_load() {
        let colors = ThemeName::Matugen.colors();
        assert_eq!(colors.name, "Matugen (Wallpaper)");
        assert_eq!(colors.title, "Matugen");
        assert!(matches!(colors.primary, Color::Rgb(_, _, _)));
    }
}
