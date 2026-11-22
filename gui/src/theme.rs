use iced::font::{Family, Weight};
use iced::widget::{button, container, progress_bar, rule, scrollable};
use iced::{Background, Border, Color, Font, Theme};

// ==================== TYPOGRAPHY HELPERS ====================

pub const FONT_REGULAR: Font = Font::DEFAULT;

pub const FONT_SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..Font::DEFAULT
};

pub const FONT_BOLD: Font = Font {
    weight: Weight::Bold,
    ..Font::DEFAULT
};

pub const FONT_MONO: Font = Font {
    family: Family::Monospace,
    ..Font::DEFAULT
};

pub const FONT_MONO_BOLD: Font = Font {
    family: Family::Monospace,
    weight: Weight::Bold,
    ..Font::DEFAULT
};

// ==================== LAYACORE PRO STUDIO PALETTE ====================
pub const BG_CANVAS: Color = Color::from_rgb(0.031, 0.043, 0.059);       // #080B0F (Deep Console Well)
pub const BG_SIDEBAR_TOP: Color = Color::from_rgb(0.063, 0.082, 0.110);  // #10151C
pub const BG_SIDEBAR_BOT: Color = Color::from_rgb(0.043, 0.055, 0.075);  // #0B0E13
#[allow(dead_code)]
pub const BG_CARD_TOP: Color = Color::from_rgb(0.086, 0.110, 0.145);     // #161C25 (Anodized Top Edge)
#[allow(dead_code)]
pub const BG_CARD_BOT: Color = Color::from_rgb(0.063, 0.080, 0.106);     // #10141B (Anodized Base)
pub const BG_CARD: Color = Color::from_rgb(0.071, 0.090, 0.122);         // #12171F
pub const BG_CARD_ALT: Color = Color::from_rgb(0.055, 0.071, 0.094);     // #0E1218 (Recessed Meter Well)
pub const BG_CARD_STRIPE: Color = Color::from_rgb(0.078, 0.098, 0.129);  // #141921
pub const BG_HOVER: Color = Color::from_rgb(0.125, 0.157, 0.208);        // #202835

pub const BORDER_SUBTLE: Color = Color::from_rgb(0.145, 0.180, 0.231);   // #252E3B
pub const BORDER_STRONG: Color = Color::from_rgb(0.227, 0.278, 0.353);   // #3A475A

pub const TEXT_PRIMARY: Color = Color::from_rgb(0.953, 0.973, 0.996);    // #F3F8FE
pub const TEXT_SECONDARY: Color = Color::from_rgb(0.753, 0.800, 0.859);  // #C0CCDB
pub const TEXT_MUTED: Color = Color::from_rgb(0.494, 0.549, 0.620);      // #7E8C9E

pub const ACCENT_RED: Color = Color::from_rgb(0.973, 0.318, 0.286);      // #F85149 (LayaCore Crimson)
pub const ACCENT_RED_DIM: Color = Color::from_rgb(0.192, 0.078, 0.090);
pub const ACCENT_ORANGE: Color = Color::from_rgb(0.961, 0.541, 0.255);   // #F58A41
pub const ACCENT_BLUE: Color = Color::from_rgb(0.345, 0.651, 1.000);     // #58A6FF (Loudness / Info)
pub const ACCENT_BLUE_DIM: Color = Color::from_rgb(0.071, 0.129, 0.216);
pub const ACCENT_CYAN: Color = Color::from_rgb(0.216, 0.812, 0.769);     // #37CFC4 (Waveform / Stereo)
pub const ACCENT_GREEN: Color = Color::from_rgb(0.247, 0.745, 0.329);    // #3FBE54 (Pass / Genuine)
pub const ACCENT_GREEN_DIM: Color = Color::from_rgb(0.067, 0.165, 0.098);
pub const ACCENT_AMBER: Color = Color::from_rgb(0.929, 0.725, 0.247);    // #EDB93F (Warning / Spectral)
pub const ACCENT_AMBER_DIM: Color = Color::from_rgb(0.196, 0.149, 0.059);
pub const ACCENT_PURPLE: Color = Color::from_rgb(0.769, 0.588, 1.000);   // #C496FF (MIR / Advisor)
pub const ACCENT_PURPLE_DIM: Color = Color::from_rgb(0.145, 0.102, 0.216);

pub fn layacore_theme() -> Theme {
    Theme::custom(
        "LayaCore Pro Studio".to_string(),
        iced::theme::Palette {
            background: BG_CANVAS,
            text: TEXT_PRIMARY,
            primary: ACCENT_RED,
            success: ACCENT_GREEN,
            warning: ACCENT_AMBER,
            danger: ACCENT_RED,
        },
    )
}

// ==================== CONTAINER STYLES ====================

pub fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SIDEBAR_BOT)),
        border: Border {
            color: BORDER_SUBTLE,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn topbar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SIDEBAR_TOP)),
        border: Border {
            color: BORDER_SUBTLE,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_CARD)),
        border: Border {
            color: BORDER_SUBTLE,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

pub fn card_alt_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_CARD_ALT)),
        border: Border {
            color: BORDER_SUBTLE,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn advice_action_well(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.043, 0.055, 0.075))),
        border: Border {
            color: Color::from_rgb(0.125, 0.155, 0.200),
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

pub fn table_row_style(striped: bool) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(if striped {
            BG_CARD_STRIPE
        } else {
            BG_CARD_ALT
        })),
        border: Border {
            color: BORDER_SUBTLE,
            width: 1.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

pub fn card_accent_style(border_color: Color, bg_color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(bg_color)),
        border: Border {
            color: Color { a: 0.78, ..border_color },
            width: 1.2,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

pub fn accent_strip_style(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

pub fn dropzone_style(hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(if hovered {
            Background::Color(ACCENT_RED_DIM)
        } else {
            Background::Color(BG_CARD)
        }),
        border: Border {
            color: if hovered { ACCENT_RED } else { BORDER_STRONG },
            width: if hovered { 2.0 } else { 1.4 },
            radius: 16.0.into(),
        },
        ..Default::default()
    }
}

pub fn badge_style(bg: Color, border: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: Color { a: 0.65, ..border },
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

pub fn meta_pill_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_CARD_ALT)),
        border: Border {
            color: BORDER_SUBTLE,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

// ==================== RULE & SCROLLABLE STYLES ====================

pub fn subtle_rule(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: BORDER_SUBTLE,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn studio_scrollable(_theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let scroller_color = match status {
        scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. } => BORDER_STRONG,
        _ => BORDER_SUBTLE,
    };
    let rail = scrollable::Rail {
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: Background::Color(scroller_color),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(BG_CARD),
            border: Border {
                color: BORDER_SUBTLE,
                width: 1.0,
                radius: 6.0.into(),
            },
            shadow: Default::default(),
            icon: TEXT_SECONDARY,
        },
    }
}

// ==================== BUTTON STYLES ====================

pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Background::Color(Color::from_rgb(1.0, 0.40, 0.36)),
        button::Status::Pressed => Background::Color(Color::from_rgb(0.80, 0.22, 0.20)),
        button::Status::Disabled => Background::Color(BG_CARD_ALT),
        button::Status::Active => Background::Color(ACCENT_RED),
    };
    button::Style {
        background: Some(bg),
        text_color: if matches!(status, button::Status::Disabled) {
            TEXT_MUTED
        } else {
            Color::WHITE
        },
        border: Border {
            color: Color::from_rgba(1.0, 0.65, 0.62, 0.25),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn secondary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let (bg, border) = match status {
        button::Status::Hovered => (Background::Color(BG_HOVER), BORDER_STRONG),
        button::Status::Pressed => (Background::Color(BG_CARD_ALT), BORDER_SUBTLE),
        button::Status::Disabled => (Background::Color(BG_CARD_ALT), BORDER_SUBTLE),
        button::Status::Active => (Background::Color(BG_CARD_ALT), BORDER_SUBTLE),
    };
    button::Style {
        background: Some(bg),
        text_color: if matches!(status, button::Status::Disabled) {
            TEXT_MUTED
        } else {
            TEXT_PRIMARY
        },
        border: Border {
            color: border,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn nav_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (bg, border, text_color) = if active {
            (
                Background::Color(ACCENT_RED_DIM),
                Color { a: 0.60, ..ACCENT_RED },
                TEXT_PRIMARY,
            )
        } else {
            match status {
                button::Status::Hovered => {
                    (Background::Color(BG_HOVER), BORDER_SUBTLE, TEXT_PRIMARY)
                }
                button::Status::Pressed => {
                    (Background::Color(BG_CARD_ALT), BORDER_SUBTLE, TEXT_PRIMARY)
                }
                button::Status::Disabled => (
                    Background::Color(Color::TRANSPARENT),
                    Color::TRANSPARENT,
                    Color::from_rgb(0.30, 0.34, 0.40),
                ),
                button::Status::Active => (
                    Background::Color(Color::TRANSPARENT),
                    Color::TRANSPARENT,
                    TEXT_SECONDARY,
                ),
            }
        };
        button::Style {
            background: Some(bg),
            text_color,
            border: Border {
                color: border,
                width: if active { 1.0 } else { 0.0 },
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn chip_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (bg, border, text_color) = if active {
            (ACCENT_BLUE_DIM, ACCENT_BLUE, ACCENT_BLUE)
        } else {
            match status {
                button::Status::Hovered => (BG_HOVER, BORDER_STRONG, TEXT_PRIMARY),
                _ => (BG_CARD_ALT, BORDER_SUBTLE, TEXT_SECONDARY),
            }
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color,
            border: Border {
                color: border,
                width: 1.0,
                radius: 14.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn table_header_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: Some(Background::Color(if matches!(status, button::Status::Hovered) {
            BG_HOVER
        } else {
            Color::TRANSPARENT
        })),
        text_color: if active { ACCENT_BLUE } else { TEXT_MUTED },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}

pub fn setting_card_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (bg, border, text_color, width) = if active {
            (Background::Color(BG_CARD_ALT), ACCENT_BLUE, TEXT_PRIMARY, 1.5)
        } else {
            match status {
                button::Status::Hovered => (Background::Color(BG_HOVER), BORDER_STRONG, TEXT_PRIMARY, 1.0),
                _ => (Background::Color(BG_CARD), BORDER_SUBTLE, TEXT_SECONDARY, 1.0),
            }
        };
        button::Style {
            background: Some(bg),
            text_color,
            border: Border {
                color: border,
                width,
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    }
}

#[allow(dead_code)]
pub fn toggle_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (bg, border, text_color) = if active {
            (ACCENT_GREEN_DIM, ACCENT_GREEN, ACCENT_GREEN)
        } else {
            match status {
                button::Status::Hovered => (BG_HOVER, BORDER_STRONG, TEXT_PRIMARY),
                _ => (BG_CARD_ALT, BORDER_SUBTLE, TEXT_MUTED),
            }
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color,
            border: Border {
                color: border,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    }
}

// ==================== PROGRESS BAR STYLE ====================

pub fn accent_progress_bar(_theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(BG_CARD_ALT),
        bar: Background::Color(ACCENT_RED),
        border: Border {
            color: BORDER_SUBTLE,
            width: 1.0,
            radius: 6.0.into(),
        },
    }
}

pub fn colored_bar_style(color: Color) -> impl Fn(&Theme) -> progress_bar::Style {
    move |_theme| progress_bar::Style {
        background: Background::Color(BG_CARD_ALT),
        bar: Background::Color(color),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
    }
}
