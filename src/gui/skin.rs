//! Theme definitions and the style functions that give the UI its look.

use iced::border::Radius;
use iced::gradient::Linear;
use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Font, Gradient, Shadow, Theme, Vector};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Neon,
    Glass,
    Matrix,
    Soft,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 4] = [ThemeKind::Neon, ThemeKind::Glass, ThemeKind::Matrix, ThemeKind::Soft];

    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::Neon => "Neon Cyberpunk",
            ThemeKind::Glass => "Midnight Glass",
            ThemeKind::Matrix => "Terminal Matrix",
            ThemeKind::Soft => "Soft Dark",
        }
    }

    pub fn from_name(name: &str) -> ThemeKind {
        Self::ALL.into_iter().find(|k| k.name() == name).unwrap_or(ThemeKind::Neon)
    }
}

impl std::fmt::Display for ThemeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Every colour and metric the views need, derived from one theme.
#[derive(Debug, Clone, Copy)]
pub struct Skin {
    pub kind: ThemeKind,
    pub bg: Color,
    pub bg2: Color,
    pub surface: Color,
    pub surface_hi: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent2: Color,
    pub live: Color,
    pub warn: Color,
    pub danger: Color,
    pub glow: f32,
    pub radius: f32,
    pub font: Font,
}

fn hex(v: u32) -> Color {
    Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// Parses `#RRGGBB` (the `#` is optional).
pub fn parse_hex(text: &str) -> Option<Color> {
    let h = text.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(hex)
}

impl Skin {
    pub fn new(kind: ThemeKind) -> Skin {
        let base = |bg, bg2, surface, surface_hi, border, text, muted, accent, accent2, live, warn, danger, glow, radius| Skin {
            kind,
            bg: hex(bg),
            bg2: hex(bg2),
            surface: hex(surface),
            surface_hi: hex(surface_hi),
            border: hex(border),
            text: hex(text),
            muted: hex(muted),
            accent: hex(accent),
            accent2: hex(accent2),
            live: hex(live),
            warn: hex(warn),
            danger: hex(danger),
            glow,
            radius,
            font: Font::DEFAULT,
        };
        match kind {
            ThemeKind::Neon => base(
                0x0a0614, 0x1a0a33, 0x130d26, 0x1d1340, 0x3d2670, 0xf3ebff, 0x9084b8, 0xff2bd6, 0x19f0ff, 0x39ff88, 0xffd24a,
                0xff3b5c, 1.0, 14.0,
            ),
            ThemeKind::Glass => base(
                0x070b1a, 0x13204a, 0x0f1730, 0x1a2652, 0x2c3d7e, 0xe8eeff, 0x8b98c6, 0x6ea8ff, 0xb08cff, 0x4ade9a, 0xfbbf5b,
                0xff6b86, 0.7, 18.0,
            ),
            ThemeKind::Matrix => Skin {
                font: Font::MONOSPACE,
                ..base(
                    0x020a04, 0x03170b, 0x061a0c, 0x0a2a14, 0x0f5a2a, 0xb8ffc8, 0x4f9a63, 0x19ff6a, 0xa6ff00, 0x19ff6a, 0xe6ff3d,
                    0xff4d3d, 0.8, 3.0,
                )
            },
            ThemeKind::Soft => base(
                0x1e1e2e, 0x181825, 0x262637, 0x313244, 0x45475a, 0xcdd6f4, 0x9399b2, 0xcba6f7, 0x89b4fa, 0xa6e3a1, 0xf9e2af,
                0xf38ba8, 0.25, 12.0,
            ),
        }
    }

    /// A site's chosen colour, or the theme's secondary accent when unset or invalid.
    pub fn site_color(&self, color: Option<&str>) -> Color {
        color.and_then(parse_hex).unwrap_or(self.accent2)
    }

    pub fn theme(&self) -> Theme {
        Theme::custom(
            self.kind.name(),
            iced::theme::Palette {
                background: self.bg,
                text: self.text,
                primary: self.accent,
                success: self.live,
                warning: self.warn,
                danger: self.danger,
            },
        )
    }

    fn gradient(&self, angle: f32, a: Color, b: Color) -> Background {
        Background::Gradient(Gradient::Linear(Linear::new(angle).add_stop(0.0, a).add_stop(1.0, b)))
    }

    fn glow_shadow(&self, color: Color, strength: f32, blur: f32) -> Shadow {
        Shadow { color: with_alpha(color, (strength * self.glow).clamp(0.0, 0.9)), offset: Vector::ZERO, blur_radius: blur }
    }
}

/// Smooth 0..1 oscillation used for pulsing glows.
pub fn pulse(phase: f32) -> f32 {
    0.5 + 0.5 * (phase * 2.0 * PI).sin()
}

pub fn app_bg(s: Skin) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(s.gradient(PI * 0.75, s.bg, s.bg2)),
        text_color: Some(s.text),
        ..Default::default()
    }
}

pub fn header(s: Skin) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(s.gradient(0.0, s.surface_hi, s.surface)),
        border: Border { color: with_alpha(s.accent, 0.35), width: 1.0, radius: Radius::from(s.radius) },
        shadow: s.glow_shadow(s.accent, 0.10, 16.0),
        ..Default::default()
    }
}

/// A rounded panel; `glow` adds a pulsing coloured halo around it.
pub fn card(s: Skin, glow: Option<(Color, f32)>) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let (border, shadow) = match glow {
            Some((c, p)) => (with_alpha(c, 0.55 + 0.35 * p), s.glow_shadow(c, 0.30 + 0.30 * p, 14.0 + 14.0 * p)),
            None => (s.border, Shadow::default()),
        };
        container::Style {
            background: Some(Background::Color(s.surface)),
            border: Border { color: border, width: 1.0, radius: Radius::from(s.radius) },
            shadow,
            ..Default::default()
        }
    }
}

pub fn inset(s: Skin) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(with_alpha(s.bg, 0.55))),
        border: Border { color: with_alpha(s.border, 0.6), width: 1.0, radius: Radius::from(s.radius * 0.6) },
        ..Default::default()
    }
}

pub fn chip(_s: Skin, color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(with_alpha(color, 0.14))),
        border: Border { color: with_alpha(color, 0.65), width: 1.0, radius: Radius::from(999.0) },
        text_color: Some(color),
        ..Default::default()
    }
}

/// A small round colour button; `selected` draws a bright outline.
pub fn swatch(s: Skin, color: Color, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(color)),
            text_color: s.text,
            border: Border {
                color: if selected || hovered { s.text } else { with_alpha(s.border, 0.8) },
                width: if selected { 2.0 } else { 1.0 },
                radius: Radius::from(99.0),
            },
            shadow: Shadow::default(),
            snap: false,
        }
    }
}

pub fn dot(color: Color, glow: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(color)),
        border: Border { radius: Radius::from(99.0), ..Default::default() },
        shadow: Shadow { color: with_alpha(color, 0.9 * glow), offset: Vector::ZERO, blur_radius: 12.0 * glow },
        ..Default::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    Primary,
    Ghost,
    Danger,
    TabActive,
    TabIdle,
}

pub fn btn(s: Skin, kind: BtnKind) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let disabled = status == button::Status::Disabled;
        let radius = Radius::from(s.radius * 0.7);
        let mut style = match kind {
            BtnKind::Primary | BtnKind::TabActive => button::Style {
                background: Some(s.gradient(0.0, s.accent, s.accent2)),
                text_color: s.bg,
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius },
                shadow: s.glow_shadow(s.accent, if hovered { 0.7 } else { 0.4 }, if hovered { 20.0 } else { 12.0 }),
                snap: false,
            },
            BtnKind::Ghost | BtnKind::TabIdle => button::Style {
                background: Some(Background::Color(if hovered { with_alpha(s.accent, 0.16) } else { Color::TRANSPARENT })),
                text_color: if hovered { s.text } else { s.muted },
                border: Border {
                    color: if kind == BtnKind::TabIdle { Color::TRANSPARENT } else { with_alpha(s.accent, if hovered { 0.8 } else { 0.4 }) },
                    width: 1.0,
                    radius,
                },
                shadow: Shadow::default(),
                snap: false,
            },
            BtnKind::Danger => button::Style {
                background: Some(Background::Color(with_alpha(s.danger, if hovered { 0.3 } else { 0.12 }))),
                text_color: s.danger,
                border: Border { color: with_alpha(s.danger, 0.6), width: 1.0, radius },
                shadow: if hovered { s.glow_shadow(s.danger, 0.4, 12.0) } else { Shadow::default() },
                snap: false,
            },
        };
        if disabled {
            style.background = style.background.map(|b| match b {
                Background::Color(c) => Background::Color(with_alpha(c, c.a * 0.4)),
                other => other,
            });
            style.text_color = with_alpha(style.text_color, 0.4);
            style.shadow = Shadow::default();
        }
        style
    }
}

/// A clickable tile or table row; `color` tints the border when it is not the muted colour.
pub fn tile(s: Skin, color: Color, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let tinted = color != s.muted;
        let background = if selected {
            with_alpha(s.accent, 0.18)
        } else if hovered {
            s.surface_hi
        } else {
            s.surface
        };
        let border = if selected {
            s.accent
        } else if tinted {
            with_alpha(color, 0.55)
        } else if hovered {
            with_alpha(s.accent, 0.6)
        } else {
            s.border
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: s.text,
            border: Border { color: border, width: 1.0, radius: Radius::from(s.radius * 0.5) },
            shadow: if tinted { s.glow_shadow(color, 0.25, 8.0) } else { Shadow::default() },
            snap: false,
        }
    }
}

/// Faint row background for dense lists; `shaded` alternates rows.
pub fn stripe(s: Skin, shaded: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: shaded.then(|| Background::Color(with_alpha(s.surface_hi, 0.35))),
        border: Border { radius: Radius::from(3.0), ..Default::default() },
        ..Default::default()
    }
}

/// Thin outline around one compact-view column.
pub fn column_frame(s: Skin) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: Border { color: with_alpha(s.border, 0.6), width: 1.0, radius: Radius::from(s.radius * 0.5) },
        ..Default::default()
    }
}

pub fn input(s: Skin) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, status| {
        let focused = matches!(status, text_input::Status::Focused { .. });
        let hovered = matches!(status, text_input::Status::Hovered | text_input::Status::Focused { is_hovered: true });
        text_input::Style {
            background: Background::Color(with_alpha(s.bg, 0.7)),
            border: Border {
                color: if focused {
                    s.accent
                } else if hovered {
                    with_alpha(s.accent, 0.6)
                } else {
                    s.border
                },
                width: 1.0,
                radius: Radius::from(s.radius * 0.6),
            },
            icon: s.muted,
            placeholder: with_alpha(s.muted, 0.7),
            value: s.text,
            selection: with_alpha(s.accent, 0.35),
        }
    }
}
