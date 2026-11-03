//! Built-in palettes (a dark and a light variant per preset) and per-color
//! overrides.

use std::collections::BTreeMap;

use ratatui::style::Color;

use crate::config::ThemePreset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub border: Color,
    pub border_focus: Color,
    pub text: Color,
    pub subtext: Color,
    pub accent: Color,
    pub title: Color,
    pub preedit: Color,
    pub candidate_index: Color,
    pub candidate_hl_bg: Color,
    pub candidate_hl_fg: Color,
    pub comment: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub status_bg: Color,
    pub selection_bg: Color,
    pub key_bg: Color,
}

const fn hex(v: u32) -> Color {
    Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

impl Theme {
    pub const MOCHA: Self = Self {
        bg: hex(0x1e1e2e),
        surface: hex(0x313244),
        border: hex(0x45475a),
        border_focus: hex(0x89b4fa),
        text: hex(0xcdd6f4),
        subtext: hex(0x7f849c),
        accent: hex(0x89b4fa),
        title: hex(0xcba6f7),
        preedit: hex(0x89b4fa),
        candidate_index: hex(0x6c7086),
        candidate_hl_bg: hex(0x89b4fa),
        candidate_hl_fg: hex(0x1e1e2e),
        comment: hex(0x6c7086),
        success: hex(0xa6e3a1),
        warning: hex(0xf9e2af),
        error: hex(0xf38ba8),
        status_bg: hex(0x181825),
        selection_bg: hex(0x313244),
        key_bg: hex(0x45475a),
    };

    pub const LATTE: Self = Self {
        bg: hex(0xeff1f5),
        surface: hex(0xccd0da),
        border: hex(0xbcc0cc),
        border_focus: hex(0x1e66f5),
        text: hex(0x4c4f69),
        subtext: hex(0x8c8fa1),
        accent: hex(0x1e66f5),
        title: hex(0x8839ef),
        preedit: hex(0x1e66f5),
        candidate_index: hex(0x9ca0b0),
        candidate_hl_bg: hex(0x1e66f5),
        candidate_hl_fg: hex(0xeff1f5),
        comment: hex(0x9ca0b0),
        success: hex(0x40a02b),
        warning: hex(0xdf8e1d),
        error: hex(0xd20f39),
        status_bg: hex(0xe6e9ef),
        selection_bg: hex(0xccd0da),
        key_bg: hex(0xccd0da),
    };

    // Rosé Pine colors are from rose-pine/neovim `palette.lua`. The accent is
    // iris, which Rosé Pine's terminal palettes use as magenta. The title is
    // foam on Moon and pine on Dawn: Dawn's foam is too faint on its light base.
    pub const ROSE_PINE_MOON: Self = Self {
        bg: hex(0x232136),
        surface: hex(0x2a273f),
        border: hex(0x56526e),
        border_focus: hex(0xc4a7e7),
        text: hex(0xe0def4),
        subtext: hex(0x908caa),
        accent: hex(0xc4a7e7),
        title: hex(0x9ccfd8),
        preedit: hex(0xc4a7e7),
        candidate_index: hex(0x6e6a86),
        candidate_hl_bg: hex(0xc4a7e7),
        candidate_hl_fg: hex(0x232136),
        comment: hex(0x6e6a86),
        success: hex(0x95b1ac),
        warning: hex(0xf6c177),
        error: hex(0xeb6f92),
        status_bg: hex(0x2a273f),
        selection_bg: hex(0x44415a),
        key_bg: hex(0x44415a),
    };

    pub const ROSE_PINE_DAWN: Self = Self {
        bg: hex(0xfaf4ed),
        surface: hex(0xfffaf3),
        border: hex(0xcecacd),
        border_focus: hex(0x907aa9),
        text: hex(0x464261),
        subtext: hex(0x797593),
        accent: hex(0x907aa9),
        title: hex(0x286983),
        preedit: hex(0x907aa9),
        candidate_index: hex(0x9893a5),
        candidate_hl_bg: hex(0x907aa9),
        candidate_hl_fg: hex(0xfaf4ed),
        comment: hex(0x9893a5),
        success: hex(0x6d8f89),
        warning: hex(0xea9d34),
        error: hex(0xb4637a),
        // Dawn's surface is lighter than its base and barely distinct.
        status_bg: hex(0xf2e9e1),
        selection_bg: hex(0xdfdad9),
        key_bg: hex(0xdfdad9),
    };

    // Tokyo Night colors are from folke/tokyonight.nvim `extras/lua`.
    pub const TOKYO_NIGHT: Self = Self {
        bg: hex(0x1a1b26),
        surface: hex(0x292e42),
        border: hex(0x3b4261),
        border_focus: hex(0x7aa2f7),
        text: hex(0xc0caf5),
        subtext: hex(0x737aa2),
        accent: hex(0x7aa2f7),
        title: hex(0xbb9af7),
        preedit: hex(0x7aa2f7),
        candidate_index: hex(0x565f89),
        candidate_hl_bg: hex(0x7aa2f7),
        candidate_hl_fg: hex(0x1a1b26),
        comment: hex(0x565f89),
        success: hex(0x9ece6a),
        warning: hex(0xe0af68),
        error: hex(0xf7768e),
        status_bg: hex(0x16161e),
        selection_bg: hex(0x283457),
        key_bg: hex(0x3b4261),
    };

    pub const TOKYO_DAY: Self = Self {
        bg: hex(0xe1e2e7),
        surface: hex(0xc4c8da),
        border: hex(0xa8aecb),
        border_focus: hex(0x2e7de9),
        text: hex(0x3760bf),
        subtext: hex(0x68709a),
        accent: hex(0x2e7de9),
        title: hex(0x9854f1),
        preedit: hex(0x2e7de9),
        candidate_index: hex(0x848cb5),
        candidate_hl_bg: hex(0x2e7de9),
        candidate_hl_fg: hex(0xe1e2e7),
        comment: hex(0x848cb5),
        success: hex(0x587539),
        warning: hex(0x8c6c3e),
        error: hex(0xf52a65),
        status_bg: hex(0xd0d5e3),
        selection_bg: hex(0xb7c1e3),
        key_bg: hex(0xc4c8da),
    };

    pub const SOLARIZED_DARK: Self = Self {
        bg: hex(0x002b36),
        surface: hex(0x073642),
        border: hex(0x586e75),
        border_focus: hex(0x268bd2),
        text: hex(0x839496),
        subtext: hex(0x657b83),
        accent: hex(0x268bd2),
        title: hex(0x6c71c4),
        preedit: hex(0x268bd2),
        candidate_index: hex(0x586e75),
        candidate_hl_bg: hex(0x268bd2),
        candidate_hl_fg: hex(0x002b36),
        comment: hex(0x586e75),
        success: hex(0x859900),
        warning: hex(0xb58900),
        error: hex(0xdc322f),
        status_bg: hex(0x073642),
        selection_bg: hex(0x073642),
        key_bg: hex(0x073642),
    };

    pub const SOLARIZED_LIGHT: Self = Self {
        bg: hex(0xfdf6e3),
        surface: hex(0xeee8d5),
        border: hex(0x93a1a1),
        border_focus: hex(0x268bd2),
        text: hex(0x657b83),
        subtext: hex(0x839496),
        accent: hex(0x268bd2),
        title: hex(0x6c71c4),
        preedit: hex(0x268bd2),
        candidate_index: hex(0x93a1a1),
        candidate_hl_bg: hex(0x268bd2),
        candidate_hl_fg: hex(0xfdf6e3),
        comment: hex(0x93a1a1),
        success: hex(0x859900),
        warning: hex(0xb58900),
        error: hex(0xdc322f),
        status_bg: hex(0xeee8d5),
        selection_bg: hex(0xeee8d5),
        key_bg: hex(0xeee8d5),
    };

    pub fn preset(preset: ThemePreset, light: bool) -> Self {
        match (preset, light) {
            (ThemePreset::Catppuccin, false) => Self::MOCHA,
            (ThemePreset::Catppuccin, true) => Self::LATTE,
            (ThemePreset::RosePine, false) => Self::ROSE_PINE_MOON,
            (ThemePreset::RosePine, true) => Self::ROSE_PINE_DAWN,
            (ThemePreset::TokyoNight, false) => Self::TOKYO_NIGHT,
            (ThemePreset::TokyoNight, true) => Self::TOKYO_DAY,
            (ThemePreset::Solarized, false) => Self::SOLARIZED_DARK,
            (ThemePreset::Solarized, true) => Self::SOLARIZED_LIGHT,
        }
    }

    /// Applies `[theme.colors]`; unknown names and bad colors are errors.
    pub fn with_overrides(mut self, colors: &BTreeMap<String, String>) -> anyhow::Result<Self> {
        for (name, value) in colors {
            let c = parse_color(value)
                .ok_or_else(|| anyhow::anyhow!("theme.colors.{name}: invalid color {value:?}"))?;
            let slot = match name.as_str() {
                "bg" => &mut self.bg,
                "surface" => &mut self.surface,
                "border" => &mut self.border,
                "border_focus" => &mut self.border_focus,
                "text" => &mut self.text,
                "subtext" => &mut self.subtext,
                "accent" => &mut self.accent,
                "title" => &mut self.title,
                "preedit" => &mut self.preedit,
                "candidate_index" => &mut self.candidate_index,
                "candidate_hl_bg" => &mut self.candidate_hl_bg,
                "candidate_hl_fg" => &mut self.candidate_hl_fg,
                "comment" => &mut self.comment,
                "success" => &mut self.success,
                "warning" => &mut self.warning,
                "error" => &mut self.error,
                "status_bg" => &mut self.status_bg,
                "selection_bg" => &mut self.selection_bg,
                "key_bg" => &mut self.key_bg,
                _ => anyhow::bail!("theme.colors: unknown color name {name:?}"),
            };
            *slot = c;
        }
        Ok(self)
    }
}

/// `#rrggbb`, or an ANSI name such as `red`, `lightblue`, `reset`.
fn parse_color(s: &str) -> Option<Color> {
    if let Some(h) = s.strip_prefix('#') {
        if h.len() == 6 {
            return u32::from_str_radix(h, 16).ok().map(hex);
        }
        return None;
    }
    s.parse::<Color>().ok()
}

/// Whether an OSC 11 `rgb:rrrr/gggg/bbbb` reply describes a light background.
pub fn is_light_background(reply: &str) -> Option<bool> {
    let body = reply.split("rgb:").nth(1)?;
    let body = body.trim_end_matches(['\x07', '\\', '\x1b']);
    let mut chans = body.split('/').map(|c| {
        let c: String = c.chars().take_while(char::is_ascii_hexdigit).collect();
        let v = u32::from_str_radix(&c, 16).ok()?;
        // Scale 1-4 hex digits to 0..1.
        let max = (1u32 << (4 * c.len() as u32)) - 1;
        Some(v as f64 / max as f64)
    });
    let (r, g, b) = (chans.next()??, chans.next()??, chans.next()??);
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides() {
        let mut m = BTreeMap::new();
        m.insert("accent".to_string(), "#ff0000".to_string());
        m.insert("text".to_string(), "red".to_string());
        let t = Theme::MOCHA.with_overrides(&m).unwrap();
        assert_eq!(t.accent, Color::Rgb(255, 0, 0));
        assert_eq!(t.text, Color::Red);
        m.insert("nope".into(), "#000000".into());
        assert!(Theme::MOCHA.with_overrides(&m).is_err());
    }

    #[test]
    fn presets() {
        use ThemePreset::*;
        for preset in [Catppuccin, RosePine, TokyoNight, Solarized] {
            let dark = Theme::preset(preset, false);
            let light = Theme::preset(preset, true);
            assert_ne!(dark, light, "{preset:?}");
            for t in [dark, light] {
                // Highlighted candidates are drawn in the background color.
                assert_eq!(t.candidate_hl_fg, t.bg, "{preset:?}");
            }
        }
        assert_eq!(Theme::preset(Catppuccin, false), Theme::MOCHA);
        assert_eq!(Theme::preset(RosePine, false), Theme::ROSE_PINE_MOON);
        assert_eq!(Theme::preset(RosePine, true), Theme::ROSE_PINE_DAWN);
    }

    #[test]
    fn osc11_brightness() {
        assert_eq!(
            is_light_background("\x1b]11;rgb:1e1e/1e1e/2e2e\x07"),
            Some(false)
        );
        assert_eq!(
            is_light_background("\x1b]11;rgb:efef/f1f1/f5f5\x1b\\"),
            Some(true)
        );
        assert_eq!(is_light_background("\x1b]11;rgb:ff/ff/ff\x07"), Some(true));
        assert_eq!(is_light_background("garbage"), None);
    }
}
