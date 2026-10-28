use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Terminal,
    Sage,
    Violet,
    Paper,
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub selected: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub terminal: bool,
}

impl Theme {
    pub const ALL: [Self; 4] = [Self::Terminal, Self::Sage, Self::Violet, Self::Paper];

    pub fn next(self) -> Self {
        self.cycle(1)
    }

    pub fn previous(self) -> Self {
        self.cycle(Self::ALL.len() - 1)
    }

    fn cycle(self, step: usize) -> Self {
        let index = Self::ALL
            .iter()
            .position(|&theme| theme == self)
            .expect("Every theme is listed in Theme::ALL");
        Self::ALL[(index + step) % Self::ALL.len()]
    }

    pub fn display(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Sage => "Sage",
            Self::Violet => "Violet",
            Self::Paper => "Paper",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Terminal => Palette {
                background: Color::Reset,
                surface: Color::Reset,
                selected: Color::Reset,
                text: Color::Reset,
                muted: Color::Reset,
                faint: Color::DarkGray,
                accent: Color::Magenta,
                terminal: true,
            },
            Self::Sage => Palette {
                background: Color::Rgb(20, 23, 22),
                surface: Color::Rgb(27, 31, 29),
                selected: Color::Rgb(39, 52, 45),
                text: Color::Rgb(222, 228, 222),
                muted: Color::Rgb(147, 158, 149),
                faint: Color::Rgb(82, 96, 87),
                accent: Color::Rgb(169, 202, 162),
                terminal: false,
            },
            // Matches the violet desktop's Kitty palette: default background and
            // foreground, inactive tabs, selection, ANSI bright black and magenta.
            Self::Violet => Palette {
                background: Color::Rgb(22, 22, 30),
                surface: Color::Rgb(39, 34, 49),
                selected: Color::Rgb(81, 67, 99),
                text: Color::Rgb(225, 219, 234),
                muted: Color::Rgb(157, 147, 175),
                faint: Color::Rgb(107, 98, 123),
                accent: Color::Rgb(196, 181, 253),
                terminal: false,
            },
            Self::Paper => Palette {
                background: Color::Rgb(245, 242, 234),
                surface: Color::Rgb(235, 231, 222),
                selected: Color::Rgb(222, 223, 206),
                text: Color::Rgb(41, 45, 40),
                muted: Color::Rgb(101, 107, 96),
                faint: Color::Rgb(155, 159, 146),
                accent: Color::Rgb(83, 104, 74),
                terminal: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycle_wraps_in_both_directions() {
        let mut theme = Theme::default();
        for expected in Theme::ALL {
            assert_eq!(theme, expected);
            assert_eq!(theme.next().previous(), theme);
            theme = theme.next();
        }
        assert_eq!(theme, Theme::Terminal);
        assert_eq!(theme.previous(), Theme::Paper);
    }

    #[test]
    fn terminal_uses_defaults_and_ansi_colors_only() {
        let palette = Theme::Terminal.palette();
        assert!(palette.terminal);
        assert_eq!(palette.background, Color::Reset);
        assert_eq!(palette.surface, Color::Reset);
        assert_eq!(palette.selected, Color::Reset);
        assert_eq!(palette.text, Color::Reset);
        for color in [palette.muted, palette.faint, palette.accent] {
            assert!(!matches!(color, Color::Rgb(..) | Color::Indexed(_)));
        }
    }
}
