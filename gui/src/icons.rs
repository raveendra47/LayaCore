use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry, Path, Stroke};
use iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Home,
    Mastering,
    Spectrum,
    Forensics,
    Batch,
    OpenFile,
    ScanFolder,
    Copy,
    ExportImage,
    ExportData,
    Critical,
    Warning,
    Tip,
    Pass,
    Settings,
    DropZone,
    FolderOpen,
    Reset,
}

pub fn icon<'a, Message: 'a>(kind: IconKind, size: f32, color: Color) -> Element<'a, Message> {
    Canvas::new(VectorIcon { kind, color })
        .width(size)
        .height(size)
        .into()
}

struct VectorIcon {
    kind: IconKind,
    color: Color,
}

impl<Message> canvas::Program<Message> for VectorIcon {
    type State = canvas::Cache;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = state.draw(renderer, bounds.size(), |frame| {
            let s = bounds.width.min(bounds.height);
            let stroke = Stroke {
                style: canvas::Style::Solid(self.color),
                width: (s * 0.105).max(1.3),
                line_cap: canvas::LineCap::Round,
                line_join: canvas::LineJoin::Round,
                ..Default::default()
            };
            let p = |x: f32, y: f32| Point::new(x * s, y * s);

            match self.kind {
            IconKind::Home => {
                // Roof
                let roof = Path::new(|b| {
                    b.move_to(p(0.14, 0.48));
                    b.line_to(p(0.50, 0.16));
                    b.line_to(p(0.86, 0.48));
                });
                frame.stroke(&roof, stroke);
                // Walls
                let body = Path::new(|b| {
                    b.move_to(p(0.24, 0.44));
                    b.line_to(p(0.24, 0.84));
                    b.line_to(p(0.76, 0.84));
                    b.line_to(p(0.76, 0.44));
                });
                frame.stroke(&body, stroke);
                // Door
                let door = Path::new(|b| {
                    b.move_to(p(0.42, 0.84));
                    b.line_to(p(0.42, 0.58));
                    b.line_to(p(0.58, 0.58));
                    b.line_to(p(0.58, 0.84));
                });
                frame.stroke(&door, stroke);
            }
            IconKind::Mastering => {
                // 3 vertical studio faders with knobs
                for &(x, knob_y) in &[(0.22_f32, 0.36_f32), (0.50, 0.64), (0.78, 0.42)] {
                    frame.stroke(&Path::line(p(x, 0.16), p(x, 0.84)), stroke);
                    frame.fill(&Path::circle(p(x, knob_y), s * 0.11), self.color);
                }
            }
            IconKind::Spectrum => {
                // 5 vertical spectrum bars
                for &(x, h) in &[
                    (0.14_f32, 0.36_f32),
                    (0.32, 0.66),
                    (0.50, 0.82),
                    (0.68, 0.54),
                    (0.86, 0.40),
                ] {
                    frame.stroke(&Path::line(p(x, 0.84), p(x, 0.84 - h * 0.72)), stroke);
                }
            }
            IconKind::Forensics => {
                // Shield outline + inner check
                let shield = Path::new(|b| {
                    b.move_to(p(0.50, 0.12));
                    b.line_to(p(0.84, 0.26));
                    b.line_to(p(0.80, 0.58));
                    b.line_to(p(0.50, 0.88));
                    b.line_to(p(0.20, 0.58));
                    b.line_to(p(0.16, 0.26));
                    b.close();
                });
                frame.stroke(&shield, stroke);
                let check = Path::new(|b| {
                    b.move_to(p(0.34, 0.50));
                    b.line_to(p(0.46, 0.62));
                    b.line_to(p(0.67, 0.38));
                });
                frame.stroke(&check, stroke);
            }
            IconKind::Batch => {
                // Stacked album layers
                let top = Path::new(|b| {
                    b.move_to(p(0.50, 0.16));
                    b.line_to(p(0.86, 0.36));
                    b.line_to(p(0.50, 0.56));
                    b.line_to(p(0.14, 0.36));
                    b.close();
                });
                frame.stroke(&top, stroke);
                let mid = Path::new(|b| {
                    b.move_to(p(0.14, 0.54));
                    b.line_to(p(0.50, 0.72));
                    b.line_to(p(0.86, 0.54));
                });
                frame.stroke(&mid, stroke);
                let bot = Path::new(|b| {
                    b.move_to(p(0.14, 0.70));
                    b.line_to(p(0.50, 0.88));
                    b.line_to(p(0.86, 0.70));
                });
                frame.stroke(&bot, stroke);
            }
            IconKind::OpenFile => {
                // Audio wave inside plus/file frame
                frame.stroke(&Path::line(p(0.50, 0.22), p(0.50, 0.78)), stroke);
                frame.stroke(&Path::line(p(0.22, 0.50), p(0.78, 0.50)), stroke);
            }
            IconKind::ScanFolder => {
                let folder = Path::new(|b| {
                    b.move_to(p(0.14, 0.24));
                    b.line_to(p(0.42, 0.24));
                    b.line_to(p(0.50, 0.34));
                    b.line_to(p(0.86, 0.34));
                    b.line_to(p(0.86, 0.80));
                    b.line_to(p(0.14, 0.80));
                    b.close();
                });
                frame.stroke(&folder, stroke);
            }
            IconKind::Copy => {
                frame.stroke(
                    &Path::rectangle(p(0.36, 0.32), Size::new(0.48 * s, 0.52 * s)),
                    stroke,
                );
                let back = Path::new(|b| {
                    b.move_to(p(0.26, 0.66));
                    b.line_to(p(0.18, 0.66));
                    b.line_to(p(0.18, 0.16));
                    b.line_to(p(0.64, 0.16));
                    b.line_to(p(0.64, 0.24));
                });
                frame.stroke(&back, stroke);
            }
            IconKind::ExportImage => {
                frame.stroke(
                    &Path::rectangle(p(0.14, 0.20), Size::new(0.72 * s, 0.60 * s)),
                    stroke,
                );
                let mt = Path::new(|b| {
                    b.move_to(p(0.18, 0.72));
                    b.line_to(p(0.42, 0.48));
                    b.line_to(p(0.58, 0.62));
                    b.line_to(p(0.72, 0.44));
                    b.line_to(p(0.84, 0.68));
                });
                frame.stroke(&mt, stroke);
                frame.fill(&Path::circle(p(0.34, 0.36), s * 0.06), self.color);
            }
            IconKind::ExportData => {
                // Download arrow into tray
                frame.stroke(&Path::line(p(0.50, 0.18), p(0.50, 0.64)), stroke);
                let tip = Path::new(|b| {
                    b.move_to(p(0.32, 0.48));
                    b.line_to(p(0.50, 0.66));
                    b.line_to(p(0.68, 0.48));
                });
                frame.stroke(&tip, stroke);
                let tray = Path::new(|b| {
                    b.move_to(p(0.18, 0.68));
                    b.line_to(p(0.18, 0.84));
                    b.line_to(p(0.82, 0.84));
                    b.line_to(p(0.82, 0.68));
                });
                frame.stroke(&tray, stroke);
            }
            IconKind::Critical => {
                frame.stroke(&Path::circle(p(0.50, 0.50), s * 0.38), stroke);
                frame.stroke(&Path::line(p(0.36, 0.36), p(0.64, 0.64)), stroke);
                frame.stroke(&Path::line(p(0.64, 0.36), p(0.36, 0.64)), stroke);
            }
            IconKind::Warning => {
                let tri = Path::new(|b| {
                    b.move_to(p(0.50, 0.16));
                    b.line_to(p(0.86, 0.82));
                    b.line_to(p(0.14, 0.82));
                    b.close();
                });
                frame.stroke(&tri, stroke);
                frame.stroke(&Path::line(p(0.50, 0.40), p(0.50, 0.58)), stroke);
                frame.fill(&Path::circle(p(0.50, 0.70), s * 0.055), self.color);
            }
            IconKind::Tip => {
                frame.stroke(&Path::circle(p(0.50, 0.50), s * 0.38), stroke);
                frame.stroke(&Path::line(p(0.50, 0.46), p(0.50, 0.70)), stroke);
                frame.fill(&Path::circle(p(0.50, 0.32), s * 0.055), self.color);
            }
            IconKind::Pass => {
                frame.stroke(&Path::circle(p(0.50, 0.50), s * 0.38), stroke);
                let chk = Path::new(|b| {
                    b.move_to(p(0.33, 0.51));
                    b.line_to(p(0.45, 0.63));
                    b.line_to(p(0.68, 0.38));
                });
                frame.stroke(&chk, stroke);
            }
            IconKind::Settings => {
                // Gear wheel: hub circle
                frame.stroke(&Path::circle(p(0.50, 0.50), s * 0.22), stroke);
                // 6 radial gear teeth
                for i in 0..6 {
                    let angle = (i as f32) * (std::f32::consts::PI / 3.0);
                    let cos_a = angle.cos();
                    let sin_a = angle.sin();
                    let r1 = s * 0.22;
                    let r2 = s * 0.38;
                    frame.stroke(
                        &Path::line(
                            Point::new(s * 0.50 + r1 * cos_a, s * 0.50 + r1 * sin_a),
                            Point::new(s * 0.50 + r2 * cos_a, s * 0.50 + r2 * sin_a),
                        ),
                        stroke,
                    );
                }
                // Center axle hole
                frame.fill(&Path::circle(p(0.50, 0.50), s * 0.07), self.color);
            }
            IconKind::DropZone => {
                // Dashed or stylish drop target: outer rounded bracket + center waveform & down arrow
                let top_left = Path::new(|b| {
                    b.move_to(p(0.32, 0.18));
                    b.line_to(p(0.20, 0.18));
                    b.line_to(p(0.20, 0.32));
                });
                frame.stroke(&top_left, stroke);

                let top_right = Path::new(|b| {
                    b.move_to(p(0.68, 0.18));
                    b.line_to(p(0.80, 0.18));
                    b.line_to(p(0.80, 0.32));
                });
                frame.stroke(&top_right, stroke);

                let bot_left = Path::new(|b| {
                    b.move_to(p(0.20, 0.68));
                    b.line_to(p(0.20, 0.82));
                    b.line_to(p(0.32, 0.82));
                });
                frame.stroke(&bot_left, stroke);

                let bot_right = Path::new(|b| {
                    b.move_to(p(0.80, 0.68));
                    b.line_to(p(0.80, 0.82));
                    b.line_to(p(0.68, 0.82));
                });
                frame.stroke(&bot_right, stroke);

                // Central downward arrow into an audio wave bar
                frame.stroke(&Path::line(p(0.50, 0.28), p(0.50, 0.62)), stroke);
                let arrow = Path::new(|b| {
                    b.move_to(p(0.38, 0.50));
                    b.line_to(p(0.50, 0.62));
                    b.line_to(p(0.62, 0.50));
                });
                frame.stroke(&arrow, stroke);

                // Horizontal audio pulse lines below arrow
                frame.stroke(&Path::line(p(0.30, 0.72), p(0.70, 0.72)), stroke);
            }
            IconKind::FolderOpen => {
                let folder = Path::new(|b| {
                    b.move_to(p(0.14, 0.28));
                    b.line_to(p(0.38, 0.28));
                    b.line_to(p(0.46, 0.38));
                    b.line_to(p(0.86, 0.38));
                    b.line_to(p(0.86, 0.50));
                    b.line_to(p(0.14, 0.50));
                    b.close();
                });
                frame.stroke(&folder, stroke);
                let flap = Path::new(|b| {
                    b.move_to(p(0.12, 0.82));
                    b.line_to(p(0.24, 0.48));
                    b.line_to(p(0.88, 0.48));
                    b.line_to(p(0.76, 0.82));
                    b.close();
                });
                frame.stroke(&flap, stroke);
            }
            IconKind::Reset => {
                // Circular refresh/undo arrow
                frame.stroke(&Path::circle(p(0.50, 0.50), s * 0.32), stroke);
                let arrow = Path::new(|b| {
                    b.move_to(p(0.50, 0.10));
                    b.line_to(p(0.50, 0.26));
                    b.line_to(p(0.66, 0.26));
                });
                frame.stroke(&arrow, stroke);
            }
        }
        });

        vec![geom]
    }
}
