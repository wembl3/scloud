use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeName {
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

impl ThemeName {
    pub const ALL: [ThemeName; 9] = [
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

    pub fn colors(&self) -> ThemeColors {
        match self {
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
                gauge_fg: Color::Rgb(76, 217, 123),
                visualizer_low: Color::Rgb(76, 217, 123),
                visualizer_mid: Color::Rgb(224, 175, 104),
                visualizer_high: Color::Rgb(247, 118, 142),
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
                gauge_fg: Color::Rgb(166, 227, 161),
                visualizer_low: Color::Rgb(166, 227, 161),
                visualizer_mid: Color::Rgb(249, 226, 175),
                visualizer_high: Color::Rgb(243, 139, 168),
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
                gauge_fg: Color::Rgb(80, 250, 123),
                visualizer_low: Color::Rgb(80, 250, 123),
                visualizer_mid: Color::Rgb(255, 184, 108),
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
                visualizer_mid: Color::Rgb(224, 175, 104),
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
                visualizer_low: Color::Rgb(163, 190, 140),
                visualizer_mid: Color::Rgb(235, 203, 139),
                visualizer_high: Color::Rgb(191, 97, 106),
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
                visualizer_low: Color::Rgb(184, 187, 38),
                visualizer_mid: Color::Rgb(254, 128, 25),
                visualizer_high: Color::Rgb(251, 73, 52),
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
                gauge_fg: Color::Rgb(255, 0, 127),
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
                gauge_fg: Color::Rgb(166, 226, 46),
                visualizer_low: Color::Rgb(166, 226, 46),
                visualizer_mid: Color::Rgb(230, 219, 116),
                visualizer_high: Color::Rgb(249, 38, 114),
            },
            ThemeName::System => ThemeColors {
                name: "System / Terminal",
                title: "System Theme",
                bg: Color::Reset,
                bg_widget: Color::Reset,
                border: Color::Cyan,
                border_active: Color::LightCyan,
                text: Color::Reset,
                text_dim: Color::DarkGray,
                primary: Color::Cyan,
                secondary: Color::Yellow,
                accent: Color::Magenta,
                success: Color::Green,
                warning: Color::Yellow,
                error: Color::Red,
                highlight_bg: Color::Blue,
                highlight_fg: Color::White,
                gauge_bg: Color::Reset,
                gauge_fg: Color::Green,
                visualizer_low: Color::Green,
                visualizer_mid: Color::Yellow,
                visualizer_high: Color::Magenta,
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
}
