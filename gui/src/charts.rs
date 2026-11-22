use std::f32::consts::PI;

use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Action, Cache, Event, Frame, Geometry, Path, Stroke, Text};
use iced::widget::text::Alignment as TextAlign;
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};

use layacore::{BandEnergy, WaveformBucket};

use crate::theme::{
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_GREEN, ACCENT_ORANGE, ACCENT_PURPLE, ACCENT_RED,
    BG_CARD_ALT, BORDER_STRONG, BORDER_SUBTLE, FONT_BOLD, FONT_MONO, FONT_MONO_BOLD, FONT_SEMIBOLD,
    TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

fn hover_redraw_update<Message>(
    was_inside: &mut bool,
    event: &Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> Option<Action<Message>> {
    match event {
        Event::Mouse(mouse::Event::CursorLeft) => {
            if *was_inside {
                *was_inside = false;
                return Some(Action::request_redraw());
            }
        }
        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            let inside = cursor.is_over(bounds);
            if inside != *was_inside {
                *was_inside = inside;
                return Some(Action::request_redraw());
            }
        }
        _ => {}
    }
    None
}

// ==================== 1. VERTICAL BROADCAST LED METER RACK ====================

pub struct BroadcastMeterRack<'a> {
    pub left_peak_dbfs: f32,
    pub left_rms_dbfs: f32,
    pub right_peak_dbfs: f32,
    pub right_rms_dbfs: f32,
    pub left_clipped: bool,
    pub right_clipped: bool,
    pub integrated_lufs: f32,
    pub short_term_lufs: f32,
    pub momentary_lufs: f32,
    pub true_peak_dbtp: f32,
    pub cache: &'a Cache,
}

impl<'a, Message> canvas::Program<Message> for BroadcastMeterRack<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            let w = bounds.width;
            let h = bounds.height;

            // Recessed hardware well
            frame.fill_rectangle(Point::ORIGIN, Size::new(w, h), BG_CARD_ALT);
        frame.stroke(
            &Path::rectangle(Point::new(0.5, 0.5), Size::new(w - 1.0, h - 1.0)),
            Stroke {
                style: canvas::Style::Solid(BORDER_SUBTLE),
                width: 1.0,
                ..Default::default()
            },
        );

        let top_y = 24.0;
        let bot_y = (h - 28.0).max(top_y + 40.0);
        let meter_h = bot_y - top_y;

        // Column X layout inside rack (5 vertical LED columns + dB scale rail)
        // Left side: dB labels (0..28), L (32..50), R (54..72), divider, INT (92..112), ST (118..136), MOM (142..160)
        let scale_w = 30.0;
        let usable = (w - scale_w - 14.0).max(90.0);
        let col_w = ((usable - 28.0) / 5.0).clamp(11.0, 22.0);
        let gap_tight = 4.0;
        let gap_group = 14.0;

        let x_l = scale_w + 4.0;
        let x_r = x_l + col_w + gap_tight;
        let x_div = x_r + col_w + gap_group * 0.5;
        let x_int = x_r + col_w + gap_group;
        let x_st = x_int + col_w + gap_tight;
        let x_mom = x_st + col_w + gap_tight;

        // Vertical group divider
        frame.stroke(
            &Path::line(Point::new(x_div, 8.0), Point::new(x_div, h - 8.0)),
            Stroke {
                style: canvas::Style::Solid(BORDER_SUBTLE),
                width: 1.0,
                ..Default::default()
            },
        );

        // Top CLIP / TRUE PEAK indicator lamps above L & R
        let draw_lamp = |frame: &mut Frame, x: f32, hot: bool| {
            let lamp_col = if hot {
                ACCENT_RED
            } else {
                Color::from_rgba(0.3, 0.35, 0.42, 0.35)
            };
            frame.fill_rectangle(Point::new(x, 8.0), Size::new(col_w, 8.0), lamp_col);
            if hot {
                frame.stroke(
                    &Path::rectangle(Point::new(x - 1.0, 7.0), Size::new(col_w + 2.0, 10.0)),
                    Stroke {
                        style: canvas::Style::Solid(Color { a: 0.45, ..ACCENT_RED }),
                        width: 1.5,
                        ..Default::default()
                    },
                );
            }
        };
        draw_lamp(&mut *frame, x_l, self.left_clipped || self.true_peak_dbtp > 0.0);
        draw_lamp(&mut *frame, x_r, self.right_clipped || self.true_peak_dbtp > 0.0);

        // LUFS group header badge
        frame.fill_text(Text {
            content: "LUFS".to_string(),
            position: Point::new((x_int + x_mom + col_w) * 0.5, 12.0),
            color: ACCENT_BLUE,
            size: 9.0.into(),
            font: FONT_MONO_BOLD,
            align_x: TextAlign::Center,
            align_y: Vertical::Center,
            ..Default::default()
        });

        // Calibrated dB scale (-54 dB .. 0 dB)
        let db_min = -54.0_f32;
        let db_max = 0.0_f32;
        let db_to_y = |db: f32| {
            let norm = ((db - db_min) / (db_max - db_min)).clamp(0.0, 1.0);
            bot_y - norm * meter_h
        };

        for &tick in &[0.0_f32, -3.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0] {
            let y = db_to_y(tick);
            frame.stroke(
                &Path::line(Point::new(scale_w - 3.0, y), Point::new(w - 6.0, y)),
                Stroke {
                    style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.055)),
                    width: 1.0,
                    ..Default::default()
                },
            );
            frame.fill_text(Text {
                content: format!("{:.0}", tick),
                position: Point::new(scale_w - 5.0, y),
                color: if tick == 0.0 { ACCENT_RED } else { TEXT_MUTED },
                size: 8.5.into(),
                font: FONT_MONO,
                align_x: TextAlign::Right,
                align_y: Vertical::Center,
                ..Default::default()
            });
        }

        // Helper: segmented LED bar (28 discrete studio LED bricks)
        let segments = 28usize;
        let seg_pitch = meter_h / segments as f32;
        let seg_h = (seg_pitch - 1.6).max(1.5);

        let draw_led_column = |frame: &mut Frame,
                               x: f32,
                               peak_db: f32,
                               rms_db: Option<f32>,
                               is_lufs: bool| {
            for s in 0..segments {
                let frac_bot = s as f32 / segments as f32;
                let seg_db = db_min + frac_bot * (db_max - db_min);
                let y = bot_y - (s as f32 + 1.0) * seg_pitch + 0.8;

                let base_color = if is_lufs {
                    if seg_db >= -8.0 {
                        ACCENT_RED
                    } else if seg_db >= -14.0 {
                        ACCENT_AMBER
                    } else if seg_db >= -23.0 {
                        ACCENT_BLUE
                    } else {
                        ACCENT_CYAN
                    }
                } else if seg_db >= -3.0 {
                    ACCENT_RED
                } else if seg_db >= -12.0 {
                    ACCENT_AMBER
                } else {
                    ACCENT_GREEN
                };

                let lit_rms = rms_db.map(|r| seg_db <= r).unwrap_or(false);
                let lit_peak = seg_db <= peak_db;

                let col = if lit_rms || (rms_db.is_none() && lit_peak) {
                    base_color
                } else if lit_peak {
                    Color { a: 0.45, ..base_color }
                } else {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.045)
                };

                frame.fill_rectangle(Point::new(x, y), Size::new(col_w, seg_h), col);
            }

            // Crisp bright peak-hold cap
            if peak_db > db_min {
                let py = db_to_y(peak_db);
                let cap_col = if !is_lufs && peak_db >= -1.0 {
                    ACCENT_RED
                } else {
                    TEXT_PRIMARY
                };
                frame.fill_rectangle(Point::new(x, (py - 1.0).max(top_y)), Size::new(col_w, 2.2), cap_col);
            }
        };

        draw_led_column(&mut *frame, x_l, self.left_peak_dbfs, Some(self.left_rms_dbfs), false);
        draw_led_column(&mut *frame, x_r, self.right_peak_dbfs, Some(self.right_rms_dbfs), false);
        draw_led_column(&mut *frame, x_int, self.integrated_lufs, None, true);
        draw_led_column(&mut *frame, x_st, self.short_term_lufs, None, true);
        draw_led_column(&mut *frame, x_mom, self.momentary_lufs, None, true);

        // Reference target marker lines across LUFS columns (-14 Spotify, -23 EBU)
        for &(target_db, col) in &[(-14.0_f32, ACCENT_GREEN), (-23.0_f32, ACCENT_PURPLE)] {
            let ty = db_to_y(target_db);
            frame.stroke(
                &Path::line(Point::new(x_int - 2.0, ty), Point::new(x_mom + col_w + 2.0, ty)),
                Stroke {
                    style: canvas::Style::Solid(col),
                    width: 1.4,
                    ..Default::default()
                },
            );
        }

        // Bottom column captions
        for &(x, lbl) in &[
            (x_l, "L"),
            (x_r, "R"),
            (x_int, "INT"),
            (x_st, "ST"),
            (x_mom, "MOM"),
        ] {
            frame.fill_text(Text {
                content: lbl.to_string(),
                position: Point::new(x + col_w * 0.5, bot_y + 13.0),
                color: TEXT_SECONDARY,
                size: 9.0.into(),
                font: FONT_MONO_BOLD,
                align_x: TextAlign::Center,
                align_y: Vertical::Center,
                ..Default::default()
            });
        }

        });

        vec![geom]
    }
}

// ==================== 2. STEREO FIELD VECTOR RADAR ====================

pub struct StereoFieldScope<'a> {
    pub width_percent: f32,
    pub lr_balance_percent: f32, // -100 right .. +100 left
    pub phase_correlation: f32,
    pub cache: &'a Cache,
}

impl<'a, Message> canvas::Program<Message> for StereoFieldScope<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            let w = bounds.width;
            let h = bounds.height;

            frame.fill_rectangle(Point::ORIGIN, Size::new(w, h), BG_CARD_ALT);
            frame.stroke(
                &Path::rectangle(Point::new(0.5, 0.5), Size::new(w - 1.0, h - 1.0)),
                Stroke {
                    style: canvas::Style::Solid(BORDER_SUBTLE),
                    width: 1.0,
                    ..Default::default()
                },
            );

            let origin = Point::new(w * 0.5, h - 16.0);
            let radius = (h - 32.0).min(w * 0.44).max(24.0);

            // Concentric semi-circular radar arcs (180° -> 360°)
            for &r_frac in &[0.35_f32, 0.68, 1.0] {
                let arc = build_arc_path(origin, radius * r_frac, 180.0, 180.0, 40);
                frame.stroke(
                    &arc,
                    Stroke {
                        style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.08)),
                        width: 1.0,
                        ..Default::default()
                    },
                );
            }

            // Radial guide spokes (-45° L, 0° C, +45° R)
            for &deg in &[225.0_f32, 270.0, 315.0] {
                let rad = deg * PI / 180.0;
                frame.stroke(
                    &Path::line(
                        origin,
                        Point::new(origin.x + radius * rad.cos(), origin.y + radius * rad.sin()),
                    ),
                    Stroke {
                        style: canvas::Style::Solid(Color::from_rgba(
                            1.0,
                            1.0,
                            1.0,
                            if deg == 270.0 { 0.16 } else { 0.07 },
                        )),
                        width: 1.0,
                        ..Default::default()
                    },
                );
            }

            // Illuminated Stereo Spread Fan
            // Center angle = 270° (straight up). Positive lr_balance = left (-angle), negative = right (+angle)
            let pan_offset = (-self.lr_balance_percent / 100.0).clamp(-1.0, 1.0) * 38.0;
            let half_spread = ((self.width_percent / 100.0).clamp(0.06, 1.35) * 44.0).min(78.0);
            let center_deg = 270.0 + pan_offset;
            let a_start = (center_deg - half_spread).max(185.0);
            let a_end = (center_deg + half_spread).min(355.0);

            let fan_color = if self.phase_correlation < 0.0 {
                ACCENT_RED
            } else if self.phase_correlation < 0.3 {
                ACCENT_AMBER
            } else {
                ACCENT_CYAN
            };

            let fan = Path::new(|b| {
                b.move_to(origin);
                let steps = 28;
                for i in 0..=steps {
                    let deg = a_start + (a_end - a_start) * (i as f32 / steps as f32);
                    let rad = deg * PI / 180.0;
                    b.line_to(Point::new(
                        origin.x + radius * 0.90 * rad.cos(),
                        origin.y + radius * 0.90 * rad.sin(),
                    ));
                }
                b.close();
            });
            frame.fill(&fan, Color { a: 0.24, ..fan_color });
            frame.stroke(
                &build_arc_path(origin, radius * 0.90, a_start, a_end - a_start, 28),
                Stroke {
                    style: canvas::Style::Solid(fan_color),
                    width: 2.2,
                    line_cap: canvas::LineCap::Round,
                    ..Default::default()
                },
            );

            // Center balance vector ray
            let c_rad = center_deg * PI / 180.0;
            frame.stroke(
                &Path::line(
                    origin,
                    Point::new(
                        origin.x + radius * 0.92 * c_rad.cos(),
                        origin.y + radius * 0.92 * c_rad.sin(),
                    ),
                ),
                Stroke {
                    style: canvas::Style::Solid(TEXT_PRIMARY),
                    width: 1.5,
                    line_cap: canvas::LineCap::Round,
                    ..Default::default()
                },
            );

            // L / C / R Labels
            frame.fill_text(Text {
                content: "L".to_string(),
                position: Point::new(origin.x - radius - 8.0, origin.y - 2.0),
                color: TEXT_MUTED,
                size: 10.0.into(),
                font: FONT_MONO_BOLD,
                align_x: TextAlign::Right,
                align_y: Vertical::Center,
                ..Default::default()
            });
            frame.fill_text(Text {
                content: "R".to_string(),
                position: Point::new(origin.x + radius + 8.0, origin.y - 2.0),
                color: TEXT_MUTED,
                size: 10.0.into(),
                font: FONT_MONO_BOLD,
                align_x: TextAlign::Left,
                align_y: Vertical::Center,
                ..Default::default()
            });
            frame.fill_text(Text {
                content: format!("WIDTH {:.0}%", self.width_percent),
                position: Point::new(origin.x, 11.0),
                color: fan_color,
                size: 9.5.into(),
                font: FONT_MONO_BOLD,
                align_x: TextAlign::Center,
                align_y: Vertical::Center,
                ..Default::default()
            });
        });

        vec![geom]
    }
}

// ==================== 3. READINESS SCORE CIRCULAR GAUGE ====================

pub struct ReadinessGauge<'a> {
    pub score: u8,
    pub grade: String,
    pub cache: &'a Cache,
}

impl<'a, Message> canvas::Program<Message> for ReadinessGauge<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            let center = Point::new(bounds.width / 2.0, bounds.height / 2.0 + 4.0);
            let radius = (bounds.width.min(bounds.height) * 0.40).max(24.0);

            let start_deg = 145.0_f32;
            let sweep_deg = 250.0_f32;

            // Outer calibration ticks (10 ticks)
            for i in 0..=10 {
                let deg = start_deg + sweep_deg * (i as f32 / 10.0);
                let rad = deg * PI / 180.0;
                let major = i % 5 == 0;
                let r1 = radius + 8.0;
                let r2 = radius + if major { 13.5 } else { 11.0 };
                frame.stroke(
                    &Path::line(
                        Point::new(center.x + r1 * rad.cos(), center.y + r1 * rad.sin()),
                        Point::new(center.x + r2 * rad.cos(), center.y + r2 * rad.sin()),
                    ),
                    Stroke {
                        style: canvas::Style::Solid(if major { BORDER_STRONG } else { BORDER_SUBTLE }),
                        width: if major { 1.6 } else { 1.1 },
                        line_cap: canvas::LineCap::Round,
                        ..Default::default()
                    },
                );
            }

            // Background track arc
            let track_path = build_arc_path(center, radius, start_deg, sweep_deg, 56);
            frame.stroke(
                &track_path,
                Stroke {
                    style: canvas::Style::Solid(BG_CARD_ALT),
                    width: 12.0,
                    line_cap: canvas::LineCap::Round,
                    ..Default::default()
                },
            );
            frame.stroke(
                &track_path,
                Stroke {
                    style: canvas::Style::Solid(BORDER_SUBTLE),
                    width: 9.5,
                    line_cap: canvas::LineCap::Round,
                    ..Default::default()
                },
            );

            let frac = (self.score as f32 / 100.0).clamp(0.0, 1.0);
            let accent = if self.score >= 85 {
                ACCENT_GREEN
            } else if self.score >= 70 {
                ACCENT_BLUE
            } else if self.score >= 50 {
                ACCENT_AMBER
            } else {
                ACCENT_RED
            };

            if frac > 0.01 {
                let value_path = build_arc_path(center, radius, start_deg, sweep_deg * frac, 56);
                frame.stroke(
                    &value_path,
                    Stroke {
                        style: canvas::Style::Solid(Color { a: 0.22, ..accent }),
                        width: 16.0,
                        line_cap: canvas::LineCap::Round,
                        ..Default::default()
                    },
                );
                frame.stroke(
                    &value_path,
                    Stroke {
                        style: canvas::Style::Solid(accent),
                        width: 9.5,
                        line_cap: canvas::LineCap::Round,
                        ..Default::default()
                    },
                );
            }

            frame.fill_text(Text {
                content: format!("{}", self.score),
                position: Point::new(center.x, center.y - 8.0),
                color: TEXT_PRIMARY,
                size: 36.0.into(),
                font: FONT_MONO_BOLD,
                align_x: TextAlign::Center,
                align_y: Vertical::Center,
                ..Default::default()
            });

            frame.fill_text(Text {
                content: "MASTER SCORE".to_string(),
                position: Point::new(center.x, center.y + 16.0),
                color: TEXT_MUTED,
                size: 8.5.into(),
                font: FONT_SEMIBOLD,
                align_x: TextAlign::Center,
                align_y: Vertical::Center,
                ..Default::default()
            });

            frame.fill_text(Text {
                content: self.grade.to_uppercase(),
                position: Point::new(center.x, center.y + radius * 0.88),
                color: accent,
                size: 10.5.into(),
                font: FONT_BOLD,
                align_x: TextAlign::Center,
                align_y: Vertical::Center,
                ..Default::default()
            });
        });

        vec![geom]
    }
}

fn build_arc_path(center: Point, radius: f32, start_deg: f32, sweep_deg: f32, steps: usize) -> Path {
    Path::new(|builder| {
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let rad = (start_deg + sweep_deg * t) * PI / 180.0;
            let pt = Point::new(center.x + radius * rad.cos(), center.y + radius * rad.sin());
            if i == 0 {
                builder.move_to(pt);
            } else {
                builder.line_to(pt);
            }
        }
    })
}

// ==================== 4. INTERACTIVE WAVEFORM ENVELOPE CANVAS ====================

pub struct WaveformCanvas<'a> {
    pub buckets: &'a [WaveformBucket],
    pub duration_seconds: f64,
    pub cache: &'a Cache,
}

impl<'a, Message> canvas::Program<Message> for WaveformCanvas<'a> {
    type State = bool;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        hover_redraw_update(state, event, bounds, cursor)
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let pad_l = 6.0;
        let pad_r = 6.0;
        let pad_t = 6.0;
        let pad_b = 20.0;
        let w = (bounds.width - pad_l - pad_r).max(10.0);
        let h = (bounds.height - pad_t - pad_b).max(10.0);
        let mid_y = pad_t + h * 0.5;
        let half_h = h * 0.47;

        let background = self.cache.draw(renderer, bounds.size(), |frame| {

        frame.fill_rectangle(Point::new(pad_l, pad_t), Size::new(w, h), BG_CARD_ALT);
        frame.stroke(
            &Path::rectangle(Point::new(pad_l, pad_t), Size::new(w, h)),
            Stroke {
                style: canvas::Style::Solid(BORDER_SUBTLE),
                width: 1.0,
                ..Default::default()
            },
        );

        for &frac in &[0.25_f32, 0.5, 0.75] {
            let x = pad_l + w * frac;
            frame.stroke(
                &Path::line(Point::new(x, pad_t), Point::new(x, pad_t + h)),
                Stroke {
                    style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.045)),
                    width: 1.0,
                    ..Default::default()
                },
            );
            let y = pad_t + h * frac;
            frame.stroke(
                &Path::line(Point::new(pad_l, y), Point::new(pad_l + w, y)),
                Stroke {
                    style: canvas::Style::Solid(Color::from_rgba(
                        1.0,
                        1.0,
                        1.0,
                        if frac == 0.5 { 0.14 } else { 0.05 },
                    )),
                    width: 1.0,
                    ..Default::default()
                },
            );
        }

        if !self.buckets.is_empty() {
            let cols = (w as usize / 3).clamp(48, 220);
            let col_w = w / cols as f32;
            let n = self.buckets.len();

            let mut peak_builder = canvas::path::Builder::new();
            let mut rms_builder = canvas::path::Builder::new();
            let mut hot_builder = canvas::path::Builder::new();

            for c in 0..cols {
                let start = c * n / cols;
                let end = ((c + 1) * n / cols).max(start + 1).min(n);

                let mut peak_pos = 0.0_f32;
                let mut peak_neg = 0.0_f32;
                let mut rms_max = 0.0_f32;
                for b in &self.buckets[start..end] {
                    peak_pos = peak_pos.max(b.max.clamp(0.0, 1.0));
                    peak_neg = peak_neg.max((-b.min).clamp(0.0, 1.0));
                    rms_max = rms_max.max(b.rms.clamp(0.0, 1.0));
                }

                let x = pad_l + c as f32 * col_w;
                let bar_w = (col_w - 0.8).max(1.5);
                let top_y = mid_y - peak_pos * half_h;
                let bot_y = mid_y + peak_neg * half_h;
                let peak_h = (bot_y - top_y).max(1.5);
                let is_hot = peak_pos >= 0.99 || peak_neg >= 0.99;

                peak_builder.rectangle(Point::new(x, top_y), Size::new(bar_w, peak_h));

                let rms_h = (rms_max * half_h * 1.95).max(1.5);
                rms_builder.rectangle(Point::new(x, mid_y - rms_h * 0.5), Size::new(bar_w, rms_h));

                if is_hot {
                    hot_builder.rectangle(Point::new(x, top_y), Size::new(bar_w, 2.5));
                    hot_builder.rectangle(
                        Point::new(x, (bot_y - 2.5).max(mid_y)),
                        Size::new(bar_w, 2.5),
                    );
                }
            }

            frame.fill(&peak_builder.build(), Color::from_rgba(0.345, 0.651, 1.0, 0.36));
            frame.fill(&rms_builder.build(), Color::from_rgba(0.216, 0.812, 0.769, 0.88));
            frame.fill(&hot_builder.build(), ACCENT_RED);
        }

        let dur = self.duration_seconds.max(0.0);
            for (frac, label) in [
                (0.0_f32, "00:00".to_string()),
                (0.25_f32, format_mmss(dur * 0.25)),
                (0.5_f32, format_mmss(dur * 0.5)),
                (0.75_f32, format_mmss(dur * 0.75)),
                (1.0_f32, format_mmss(dur)),
            ] {
                let align = if frac == 0.0 {
                    TextAlign::Left
                } else if frac == 1.0 {
                    TextAlign::Right
                } else {
                    TextAlign::Center
                };
                frame.fill_text(Text {
                    content: label,
                    position: Point::new(pad_l + w * frac, pad_t + h + 11.0),
                    color: TEXT_MUTED,
                    size: 9.5.into(),
                    font: FONT_MONO,
                    align_x: align,
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }
        });

        // Dynamic playhead crosshair overlay (uncached lightweight pass)
        if !self.buckets.is_empty() {
            if let Some(pos) = cursor.position_in(bounds) {
                if pos.x >= pad_l && pos.x <= pad_l + w && pos.y >= pad_t && pos.y <= pad_t + h {
                    let n = self.buckets.len();
                    let frac = ((pos.x - pad_l) / w).clamp(0.0, 1.0);
                    let idx = ((frac * (n.saturating_sub(1)) as f32).round() as usize).min(n - 1);
                    let b = &self.buckets[idx];
                    let peak_lin = b.max.abs().max(b.min.abs());
                    let peak_db = if peak_lin > 1e-6 {
                        20.0 * peak_lin.log10()
                    } else {
                        -120.0
                    };
                    let rms_db = if b.rms > 1e-6 {
                        20.0 * b.rms.log10()
                    } else {
                        -120.0
                    };
                    let t_str = format_mmss(self.duration_seconds * frac as f64);

                    let mut overlay = Frame::new(renderer, bounds.size());
                    overlay.stroke(
                        &Path::line(Point::new(pos.x, pad_t), Point::new(pos.x, pad_t + h)),
                        Stroke {
                            style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.65)),
                            width: 1.2,
                            ..Default::default()
                        },
                    );

                    let hud_text = format!("{}  •  PK {:+.1} dB  •  RMS {:+.1} dB", t_str, peak_db, rms_db);
                    let hud_w = 216.0;
                    let hud_h = 20.0;
                    let hud_x = if pos.x + hud_w + 10.0 <= pad_l + w {
                        pos.x + 8.0
                    } else {
                        (pos.x - hud_w - 8.0).max(pad_l + 4.0)
                    };
                    let hud_y = pad_t + 6.0;

                    overlay.fill_rectangle(
                        Point::new(hud_x, hud_y),
                        Size::new(hud_w, hud_h),
                        Color::from_rgba(0.05, 0.07, 0.10, 0.94),
                    );
                    overlay.stroke(
                        &Path::rectangle(Point::new(hud_x, hud_y), Size::new(hud_w, hud_h)),
                        Stroke {
                            style: canvas::Style::Solid(ACCENT_CYAN),
                            width: 1.0,
                            ..Default::default()
                        },
                    );
                    overlay.fill_text(Text {
                        content: hud_text,
                        position: Point::new(hud_x + hud_w * 0.5, hud_y + hud_h * 0.5),
                        color: TEXT_PRIMARY,
                        size: 9.5.into(),
                        font: FONT_MONO_BOLD,
                        align_x: TextAlign::Center,
                        align_y: Vertical::Center,
                        ..Default::default()
                    });

                    return vec![background, overlay.into_geometry()];
                }
            }
        }

        vec![background]
    }
}

// ==================== 5. FABFILTER-STYLE INTERACTIVE 31-BAND SPECTRUM ====================

pub struct SpectrumCanvas<'a> {
    pub bands: &'a [BandEnergy],
    pub cutoff_hz: Option<f32>,
    pub cache: &'a Cache,
}

impl<'a, Message> canvas::Program<Message> for SpectrumCanvas<'a> {
    type State = bool;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        hover_redraw_update(state, event, bounds, cursor)
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let pad_l = 40.0;
        let pad_r = 12.0;
        let pad_t = 22.0;
        let pad_b = 26.0;
        let w = (bounds.width - pad_l - pad_r).max(20.0);
        let h = (bounds.height - pad_t - pad_b).max(20.0);
        let db_min = -78.0_f32;
        let db_max = 0.0_f32;
        let n = self.bands.len().max(1);
        let slot_w = w / n as f32;
        let bar_w = (slot_w * 0.64).max(3.0);
        let bar_off = (slot_w - bar_w) * 0.5;

        // Precompute envelope points for smooth FabFilter-style area & curve
        let mut pts: Vec<Point> = Vec::with_capacity(self.bands.len());
        for (i, band) in self.bands.iter().enumerate() {
            let db = band.energy_db.clamp(db_min, db_max);
            let norm = (db - db_min) / (db_max - db_min);
            let y = pad_t + h - (norm * h).max(2.0);
            let cx = pad_l + (i as f32 + 0.5) * slot_w;
            pts.push(Point::new(cx, y));
        }

        let background = self.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill_rectangle(Point::new(pad_l, pad_t), Size::new(w, h), BG_CARD_ALT);
            frame.stroke(
                &Path::rectangle(Point::new(pad_l, pad_t), Size::new(w, h)),
                Stroke {
                    style: canvas::Style::Solid(BORDER_SUBTLE),
                    width: 1.0,
                    ..Default::default()
                },
            );

            for &(frac, lbl) in &[
                (0.08_f32, "SUB"),
                (0.25_f32, "BASS"),
                (0.50_f32, "MIDS"),
                (0.74_f32, "PRESENCE"),
                (0.92_f32, "AIR"),
            ] {
                frame.fill_text(Text {
                    content: lbl.to_string(),
                    position: Point::new(pad_l + w * frac, 10.0),
                    color: TEXT_MUTED,
                    size: 9.0.into(),
                    font: FONT_SEMIBOLD,
                    align_x: TextAlign::Center,
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }

            for &db in &[0.0_f32, -12.0, -24.0, -36.0, -48.0, -60.0, -72.0] {
                let t = (db_max - db) / (db_max - db_min);
                let y = pad_t + t * h;
                frame.stroke(
                    &Path::line(Point::new(pad_l, y), Point::new(pad_l + w, y)),
                    Stroke {
                        style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.065)),
                        width: 1.0,
                        ..Default::default()
                    },
                );
                frame.fill_text(Text {
                    content: format!("{:.0}", db),
                    position: Point::new(pad_l - 7.0, y),
                    color: TEXT_MUTED,
                    size: 9.5.into(),
                    font: FONT_MONO,
                    align_x: TextAlign::Right,
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }

            // Translucent filled spectral area beneath envelope curve
            if pts.len() >= 2 {
                let area = Path::new(|b| {
                    b.move_to(Point::new(pts[0].x, pad_t + h));
                    b.line_to(pts[0]);
                    for i in 0..pts.len() - 1 {
                        let mid = Point::new(
                            (pts[i].x + pts[i + 1].x) * 0.5,
                            (pts[i].y + pts[i + 1].y) * 0.5,
                        );
                        b.quadratic_curve_to(pts[i], mid);
                    }
                    if let Some(&last) = pts.last() {
                        b.line_to(last);
                        b.line_to(Point::new(last.x, pad_t + h));
                    }
                    b.close();
                });
                frame.fill(&area, Color::from_rgba(0.345, 0.651, 1.0, 0.11));
            }

            // Draw segmented studio bars
            for (i, band) in self.bands.iter().enumerate() {
                let db = band.energy_db.clamp(db_min, db_max);
                let norm = (db - db_min) / (db_max - db_min);
                let bar_h = (norm * h).max(2.0);
                let x = pad_l + i as f32 * slot_w + bar_off;
                let y = pad_t + h - bar_h;
                let base_color = band_color(band.center_hz);

                frame.fill_rectangle(
                    Point::new(x, y),
                    Size::new(bar_w, bar_h),
                    Color { a: 0.52, ..base_color },
                );
                frame.fill_rectangle(
                    Point::new(x, y),
                    Size::new(bar_w, 2.5_f32.min(bar_h)),
                    base_color,
                );

                if i % 3 == 0 || i == n - 1 {
                    frame.fill_text(Text {
                        content: format_hz_short(band.center_hz),
                        position: Point::new(x + bar_w * 0.5, pad_t + h + 13.0),
                        color: TEXT_MUTED,
                        size: 9.5.into(),
                        font: FONT_MONO,
                        align_x: TextAlign::Center,
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                }
            }

            // Smooth illuminated spectral envelope curve on top
            if pts.len() >= 2 {
                let curve = Path::new(|b| {
                    b.move_to(pts[0]);
                    for i in 0..pts.len() - 1 {
                        let mid = Point::new(
                            (pts[i].x + pts[i + 1].x) * 0.5,
                            (pts[i].y + pts[i + 1].y) * 0.5,
                        );
                        b.quadratic_curve_to(pts[i], mid);
                    }
                    if let Some(&last) = pts.last() {
                        b.line_to(last);
                    }
                });
                frame.stroke(
                    &curve,
                    Stroke {
                        style: canvas::Style::Solid(Color::from_rgba(0.75, 0.92, 1.0, 0.85)),
                        width: 1.8,
                        line_cap: canvas::LineCap::Round,
                        line_join: canvas::LineJoin::Round,
                        ..Default::default()
                    },
                );
            }

            // Optional brickwall cutoff callout line
            if let Some(cutoff) = self.cutoff_hz {
                if let Some((idx, _)) = self
                    .bands
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        (a.center_hz - cutoff)
                            .abs()
                            .partial_cmp(&(b.center_hz - cutoff).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                {
                    let cx = pad_l + (idx as f32 + 0.5) * slot_w;
                    let right_w = (pad_l + w - cx).max(0.0);
                    if right_w > 2.0 {
                        frame.fill_rectangle(
                            Point::new(cx, pad_t),
                            Size::new(right_w, h),
                            Color::from_rgba(0.973, 0.318, 0.286, 0.10),
                        );
                    }
                    frame.stroke(
                        &Path::line(Point::new(cx, pad_t), Point::new(cx, pad_t + h)),
                        Stroke {
                            style: canvas::Style::Solid(ACCENT_RED),
                            width: 2.0,
                            ..Default::default()
                        },
                    );
                    let pill_w = 110.0;
                    let pill_x = (cx - pill_w - 6.0).max(pad_l + 6.0);
                    frame.fill_rectangle(
                        Point::new(pill_x, pad_t + 6.0),
                        Size::new(pill_w, 19.0),
                        Color::from_rgba(0.20, 0.08, 0.10, 0.95),
                    );
                    frame.stroke(
                        &Path::rectangle(Point::new(pill_x, pad_t + 6.0), Size::new(pill_w, 19.0)),
                        Stroke {
                            style: canvas::Style::Solid(ACCENT_RED),
                            width: 1.0,
                            ..Default::default()
                        },
                    );
                    frame.fill_text(Text {
                        content: format!("CUTOFF {:.1} kHz", cutoff / 1000.0),
                        position: Point::new(pill_x + pill_w * 0.5, pad_t + 15.5),
                        color: ACCENT_RED,
                        size: 9.5.into(),
                        font: FONT_MONO_BOLD,
                        align_x: TextAlign::Center,
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                }
            }
        });

        // Interactive Hover Crosshair + Floating Studio HUD Pill (DYNAMIC OVERLAY)
        let hovered_idx = cursor.position_in(bounds).and_then(|pos| {
            if pos.x >= pad_l && pos.x <= pad_l + w && pos.y >= pad_t && pos.y <= pad_t + h {
                let idx = ((pos.x - pad_l) / slot_w).floor() as usize;
                Some(idx.min(self.bands.len().saturating_sub(1)))
            } else {
                None
            }
        });

        if let Some(i) = hovered_idx {
            if let (Some(band), Some(&pt)) = (self.bands.get(i), pts.get(i)) {
                let mut overlay = Frame::new(renderer, bounds.size());
                overlay.stroke(
                    &Path::line(Point::new(pt.x, pad_t), Point::new(pt.x, pad_t + h)),
                    Stroke {
                        style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.35)),
                        width: 1.0,
                        ..Default::default()
                    },
                );
                overlay.stroke(
                    &Path::line(Point::new(pad_l, pt.y), Point::new(pad_l + w, pt.y)),
                    Stroke {
                        style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.22)),
                        width: 1.0,
                        ..Default::default()
                    },
                );
                overlay.fill(&Path::circle(pt, 4.2), TEXT_PRIMARY);

                let hz_lbl = if band.center_hz >= 1000.0 {
                    format!("{:.2} kHz", band.center_hz / 1000.0)
                } else {
                    format!("{:.0} Hz", band.center_hz)
                };
                let reg = band_region_name(band.center_hz);
                let hud_text = format!("{}  •  {:.1} dB  •  {}", hz_lbl, band.energy_db, reg);
                let hud_w = 186.0;
                let hud_h = 21.0;
                let hud_x = if pt.x + hud_w + 12.0 <= pad_l + w {
                    pt.x + 10.0
                } else {
                    (pt.x - hud_w - 10.0).max(pad_l + 6.0)
                };
                let hud_y = (pt.y - 26.0).max(pad_t + 6.0);
                let col = band_color(band.center_hz);

                overlay.fill_rectangle(
                    Point::new(hud_x, hud_y),
                    Size::new(hud_w, hud_h),
                    Color::from_rgba(0.05, 0.07, 0.10, 0.96),
                );
                overlay.stroke(
                    &Path::rectangle(Point::new(hud_x, hud_y), Size::new(hud_w, hud_h)),
                    Stroke {
                        style: canvas::Style::Solid(col),
                        width: 1.1,
                        ..Default::default()
                    },
                );
                overlay.fill_text(Text {
                    content: hud_text,
                    position: Point::new(hud_x + hud_w * 0.5, hud_y + hud_h * 0.5),
                    color: TEXT_PRIMARY,
                    size: 9.5.into(),
                    font: FONT_MONO_BOLD,
                    align_x: TextAlign::Center,
                    align_y: Vertical::Center,
                    ..Default::default()
                });

                return vec![background, overlay.into_geometry()];
            }
        }

        vec![background]
    }
}

fn band_color(hz: f32) -> Color {
    if hz < 60.0 {
        ACCENT_RED
    } else if hz < 250.0 {
        ACCENT_ORANGE
    } else if hz < 2000.0 {
        ACCENT_AMBER
    } else if hz < 6000.0 {
        ACCENT_CYAN
    } else {
        ACCENT_BLUE
    }
}

fn band_region_name(hz: f32) -> &'static str {
    if hz < 60.0 {
        "SUB"
    } else if hz < 250.0 {
        "BASS"
    } else if hz < 2000.0 {
        "MIDS"
    } else if hz < 6000.0 {
        "PRES"
    } else {
        "AIR"
    }
}

fn format_hz_short(hz: f32) -> String {
    if hz >= 1000.0 {
        let khz = hz / 1000.0;
        if (khz - khz.round()).abs() < 0.05 {
            format!("{:.0}k", khz)
        } else {
            format!("{:.1}k", khz)
        }
    } else {
        format!("{:.0}", hz)
    }
}

// ==================== 6. INTERACTIVE 12-TONE CHROMA PROFILE ====================

pub struct ChromaCanvas<'a> {
    pub chroma: &'a [f32],
    pub estimated_key: &'a str,
    pub cache: &'a Cache,
}

const CHROMA_NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

impl<'a, Message> canvas::Program<Message> for ChromaCanvas<'a> {
    type State = bool;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        hover_redraw_update(state, event, bounds, cursor)
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let pad_x = 10.0;
        let pad_t = 12.0;
        let pad_b = 24.0;
        let w = (bounds.width - pad_x * 2.0).max(20.0);
        let h = (bounds.height - pad_t - pad_b).max(20.0);
        let slot_w = w / 12.0;
        let bar_w = (slot_w * 0.58).max(4.0);
        let bar_off = (slot_w - bar_w) * 0.5;

        let tonic = self.estimated_key.split_whitespace().next().unwrap_or("");
        let max_val = self.chroma.iter().copied().fold(0.001_f32, f32::max);
        let sum_val: f32 = self.chroma.iter().copied().sum::<f32>().max(0.001);

        let background = self.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill_rectangle(Point::new(pad_x, pad_t), Size::new(w, h), BG_CARD_ALT);
            frame.stroke(
                &Path::rectangle(Point::new(pad_x, pad_t), Size::new(w, h)),
                Stroke {
                    style: canvas::Style::Solid(BORDER_SUBTLE),
                    width: 1.0,
                    ..Default::default()
                },
            );

            for &frac in &[0.33_f32, 0.66] {
                let y = pad_t + h * frac;
                frame.stroke(
                    &Path::line(Point::new(pad_x, y), Point::new(pad_x + w, y)),
                    Stroke {
                        style: canvas::Style::Solid(Color::from_rgba(1.0, 1.0, 1.0, 0.05)),
                        width: 1.0,
                        ..Default::default()
                    },
                );
            }

            for i in 0..12 {
                let val = self.chroma.get(i).copied().unwrap_or(0.0);
                let norm = (val / max_val).clamp(0.0, 1.0);
                let bar_h = (norm * (h - 14.0)).max(3.0);
                let x = pad_x + i as f32 * slot_w + bar_off;
                let y = pad_t + h - bar_h;

                let is_tonic = CHROMA_NOTES[i] == tonic;
                let base = if is_tonic { ACCENT_RED } else { ACCENT_PURPLE };

                frame.fill_rectangle(
                    Point::new(x, y),
                    Size::new(bar_w, bar_h),
                    Color {
                        a: if is_tonic { 0.82 } else { 0.58 },
                        ..base
                    },
                );
                frame.fill_rectangle(
                    Point::new(x, y),
                    Size::new(bar_w, 2.5_f32.min(bar_h)),
                    base,
                );

                if is_tonic {
                    frame.fill_text(Text {
                        content: "●".to_string(),
                        position: Point::new(x + bar_w * 0.5, (y - 6.0).max(pad_t + 5.0)),
                        color: ACCENT_RED,
                        size: 7.5.into(),
                        align_x: TextAlign::Center,
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                }

                frame.fill_text(Text {
                    content: CHROMA_NOTES[i].to_string(),
                    position: Point::new(x + bar_w * 0.5, pad_t + h + 12.0),
                    color: if is_tonic { ACCENT_RED } else { TEXT_MUTED },
                    size: 10.5.into(),
                    font: if is_tonic { FONT_MONO_BOLD } else { FONT_MONO },
                    align_x: TextAlign::Center,
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }
        });

        let hovered = cursor.position_in(bounds).and_then(|pos| {
            if pos.x >= pad_x && pos.x <= pad_x + w && pos.y >= pad_t && pos.y <= pad_t + h {
                Some((((pos.x - pad_x) / slot_w).floor() as usize).min(11))
            } else {
                None
            }
        });

        if let Some(i) = hovered {
            let val = self.chroma.get(i).copied().unwrap_or(0.0);
            let share = (val / sum_val) * 100.0;
            let hud_w = 104.0;
            let hud_h = 19.0;
            let hud_x = pad_x + w - hud_w - 6.0;
            let hud_y = pad_t + 5.0;

            let mut overlay = Frame::new(renderer, bounds.size());
            overlay.fill_rectangle(
                Point::new(hud_x, hud_y),
                Size::new(hud_w, hud_h),
                Color::from_rgba(0.05, 0.07, 0.10, 0.94),
            );
            overlay.stroke(
                &Path::rectangle(Point::new(hud_x, hud_y), Size::new(hud_w, hud_h)),
                Stroke {
                    style: canvas::Style::Solid(ACCENT_PURPLE),
                    width: 1.0,
                    ..Default::default()
                },
            );
            overlay.fill_text(Text {
                content: format!("{}  •  {:.1}%", CHROMA_NOTES[i], share),
                position: Point::new(hud_x + hud_w * 0.5, hud_y + hud_h * 0.5),
                color: TEXT_PRIMARY,
                size: 9.5.into(),
                font: FONT_MONO_BOLD,
                align_x: TextAlign::Center,
                align_y: Vertical::Center,
                ..Default::default()
            });

            return vec![background, overlay.into_geometry()];
        }

        vec![background]
    }
}

// ==================== 7. STEREO PHASE CORRELATION METER ====================

pub struct PhaseMeterCanvas<'a> {
    pub correlation: f32,
    pub cache: &'a Cache,
}

impl<'a, Message> canvas::Program<Message> for PhaseMeterCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            let pad_x = 8.0;
            let bar_y = 8.0;
            let bar_h = 11.0;
            let w = (bounds.width - pad_x * 2.0).max(20.0);

            let zero_x = pad_x + w * 0.5;
            let good_x = pad_x + w * 0.65;

            frame.fill_rectangle(
                Point::new(pad_x, bar_y),
                Size::new(w * 0.5, bar_h),
                Color::from_rgba(0.973, 0.318, 0.286, 0.30),
            );
            frame.fill_rectangle(
                Point::new(zero_x, bar_y),
                Size::new(w * 0.15, bar_h),
                Color::from_rgba(0.929, 0.725, 0.247, 0.32),
            );
            frame.fill_rectangle(
                Point::new(good_x, bar_y),
                Size::new(w * 0.35, bar_h),
                Color::from_rgba(0.247, 0.745, 0.329, 0.36),
            );
            frame.stroke(
                &Path::rectangle(Point::new(pad_x, bar_y), Size::new(w, bar_h)),
                Stroke {
                    style: canvas::Style::Solid(BORDER_SUBTLE),
                    width: 1.0,
                    ..Default::default()
                },
            );

            frame.stroke(
                &Path::line(Point::new(zero_x, bar_y - 2.0), Point::new(zero_x, bar_y + bar_h + 2.0)),
                Stroke {
                    style: canvas::Style::Solid(BORDER_STRONG),
                    width: 1.5,
                    ..Default::default()
                },
            );

            let corr = self.correlation.clamp(-1.0, 1.0);
            let nx = pad_x + ((corr + 1.0) * 0.5) * w;
            let needle_color = if corr < 0.0 {
                ACCENT_RED
            } else if corr < 0.3 {
                ACCENT_AMBER
            } else {
                ACCENT_GREEN
            };
            frame.fill_rectangle(
                Point::new(nx - 4.5, bar_y - 3.0),
                Size::new(9.0, bar_h + 6.0),
                Color { a: 0.28, ..needle_color },
            );
            frame.fill_rectangle(
                Point::new(nx - 2.0, bar_y - 3.0),
                Size::new(4.0, bar_h + 6.0),
                needle_color,
            );

            for &(frac, lbl) in &[
                (0.0_f32, "-1.0"),
                (0.5_f32, "0.0"),
                (0.75_f32, "+0.5"),
                (1.0_f32, "+1.0"),
            ] {
                let align = if frac == 0.0 {
                    TextAlign::Left
                } else if frac == 1.0 {
                    TextAlign::Right
                } else {
                    TextAlign::Center
                };
                frame.fill_text(Text {
                    content: lbl.to_string(),
                    position: Point::new(pad_x + w * frac, bar_y + bar_h + 11.0),
                    color: TEXT_SECONDARY,
                    size: 9.0.into(),
                    font: FONT_MONO,
                    align_x: align,
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }
        });

        vec![geom]
    }
}

// ==================== 8. MICRO TARGET METER BAR ====================

pub struct MiniMeterCanvas<'a> {
    pub value_norm: f32,
    pub target_norm: Option<f32>,
    pub color: Color,
    pub cache: &'a Cache,
}

impl<'a, Message> canvas::Program<Message> for MiniMeterCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            let h = 6.0;
            let y = (bounds.height - h) * 0.5;
            let w = bounds.width.max(10.0);

            frame.fill_rectangle(Point::new(0.0, y), Size::new(w, h), BG_CARD_ALT);

            let fill_w = (w * self.value_norm.clamp(0.0, 1.0)).max(2.0);
            frame.fill_rectangle(Point::new(0.0, y), Size::new(fill_w, h), self.color);

            if let Some(t) = self.target_norm {
                let tx = w * t.clamp(0.0, 1.0);
                frame.stroke(
                    &Path::line(Point::new(tx, y - 2.0), Point::new(tx, y + h + 2.0)),
                    Stroke {
                        style: canvas::Style::Solid(TEXT_PRIMARY),
                        width: 1.5,
                        ..Default::default()
                    },
                );
            }
        });

        vec![geom]
    }
}

pub fn format_mmss(secs: f64) -> String {
    let total = secs.round().max(0.0) as u64;
    let mins = total / 60;
    let rem = total % 60;
    format!("{:02}:{:02}", mins, rem)
}
