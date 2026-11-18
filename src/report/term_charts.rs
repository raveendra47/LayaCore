use colored::Colorize;
use termichart_charts::LineChart;
use termichart_core::{ChartConfig, ChartType, Color, Point, Range, Style, Theme, ThemeName, Viewport};
use termichart_render::{painter, render_to_string, AxisRenderer, CellGrid, Layout};

use crate::spectral::NOTE_NAMES;
use crate::AudioAnalysis;

/// Renders a multi-row terminal line chart of the audio waveform's RMS envelope with clean mm:ss timestamps.
pub fn render_terminal_waveform(analysis: &AudioAnalysis, width: u16, height: u16, theme_name: ThemeName) -> Option<String> {
    let buckets = &analysis.waveform.buckets;
    if buckets.len() < 2 {
        return None;
    }

    let duration = analysis.metadata.duration_seconds.unwrap_or(0.0).max(0.1);
    let theme = Theme::from_name(theme_name);

    let config = ChartConfig {
        chart_type: ChartType::Line,
        width: Some(width),
        height: Some(height),
        theme: theme.clone(),
        show_axis: true,
        ..Default::default()
    };

    let layout = Layout::compute(width, height, &config);
    let mut grid = CellGrid::new(width, height);

    // Title
    let title = format!("WAVEFORM ENVELOPE (RMS Dynamics  •  0:00 – {})", format_mmss(duration));
    painter::draw_text(
        &mut grid,
        2,
        0,
        &title,
        Style {
            fg: theme.text_color(),
            bold: true,
            ..Default::default()
        },
    );

    let target_points = (layout.chart_area.width as usize).clamp(20, 100);
    let step = (buckets.len() as f64) / (target_points as f64);
    let mut points = Vec::with_capacity(target_points);
    let mut max_rms = 0.0f64;

    for i in 0..target_points {
        let start_idx = (i as f64 * step).floor() as usize;
        let end_idx = (((i + 1) as f64 * step).ceil() as usize).min(buckets.len());

        let mut bucket_max = 0.0f32;
        for b in &buckets[start_idx..end_idx.max(start_idx + 1)] {
            if b.rms > bucket_max {
                bucket_max = b.rms;
            }
        }

        let time_sec = (i as f64 / (target_points - 1).max(1) as f64) * duration;
        let rms_val = bucket_max as f64;
        if rms_val > max_rms {
            max_rms = rms_val;
        }
        points.push(Point { x: time_sec, y: rms_val });
    }

    let viewport = Viewport {
        x_range: Range { min: 0.0, max: duration },
        y_range: Range { min: 0.0, max: max_rms.max(0.1) },
        width: layout.chart_area.width,
        height: layout.chart_area.height,
    };

    // Render line chart & Y-axis
    let chart = LineChart::new(points);
    chart.render(&mut grid, &viewport, &config, layout.chart_area.x, layout.chart_area.y).ok()?;
    AxisRenderer.draw_y_axis(&mut grid, &viewport, &layout.y_axis_area, &theme);

    // Render clean X-axis with time ticks
    let axis_style = Style {
        fg: theme.axis_color(),
        dim: true,
        ..Default::default()
    };
    let label_style = Style {
        fg: theme.label_color(),
        ..Default::default()
    };

    let x_start = layout.chart_area.x;
    let x_end = layout.chart_area.right();
    let y_line = layout.chart_area.bottom();

    grid.set(x_start.saturating_sub(1), y_line, '\u{2514}', axis_style); // └
    for col in x_start..x_end {
        grid.set(col, y_line, '\u{2500}', axis_style); // ─
    }

    // 5 clean evenly spaced timestamp ticks
    let num_ticks = 5usize;
    for i in 0..num_ticks {
        let frac = i as f64 / (num_ticks - 1) as f64;
        let sec = frac * duration;
        let col = x_start + (frac * (layout.chart_area.width.saturating_sub(1)) as f64).round() as u16;

        grid.set(col, y_line, '\u{252C}', axis_style); // ┬

        let label = format_mmss(sec);
        let label_x = if i == 0 {
            col
        } else if i == num_ticks - 1 {
            col.saturating_sub(label.len() as u16 - 1)
        } else {
            col.saturating_sub((label.len() / 2) as u16)
        };

        painter::draw_text(&mut grid, label_x, y_line + 1, &label, label_style);
    }

    Some(render_to_string(&grid))
}

/// Helper to format seconds as m:ss (e.g. 94.2s -> 1:34)
fn format_mmss(seconds: f64) -> String {
    let total_secs = seconds.round() as u64;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{}:{:02}", mins, secs)
}

/// Renders a 12-semitone pitch class (chroma) energy profile as an ANSI bar visualizer,
/// highlighting the detected key's tonic and dominant 5th.
pub fn render_terminal_chroma(analysis: &AudioAnalysis) -> Option<String> {
    let chroma = &analysis.musical.chroma_profile;
    if chroma.len() < 12 {
        return None;
    }

    let max_energy = chroma.iter().copied().fold(1e-6f32, f32::max);
    let key_str = &analysis.musical.estimated_key;

    // Extract tonic note name (e.g. "D" from "D Minor" or "C#" from "C# Major")
    let tonic = key_str.split_whitespace().next().unwrap_or("C");
    let tonic_idx = NOTE_NAMES.iter().position(|&n| n == tonic).unwrap_or(0);
    let dominant_idx = (tonic_idx + 7) % 12; // Musical dominant 5th is 7 semitones up
    let dominant = NOTE_NAMES[dominant_idx];

    let width: u16 = 72;
    let height: u16 = 10;
    let mut grid = CellGrid::new(width, height);
    let theme = Theme::from_name(ThemeName::Dracula);

    // Title
    let title = format!(
        "CHROMA PROFILE • {} (Tonic: {}, 5th: {}, {:.0}% conf)",
        key_str,
        tonic,
        dominant,
        analysis.musical.key_confidence * 100.0
    );
    painter::draw_text(
        &mut grid,
        2,
        0,
        &title,
        Style {
            fg: theme.text_color(),
            bold: true,
            ..Default::default()
        },
    );

    let bar_width = 3usize;
    let gap = 2usize;
    let start_x = 4usize;
    let chart_height = (height - 3) as usize;

    for (i, (&name, &energy)) in NOTE_NAMES.iter().zip(chroma.iter()).enumerate() {
        let norm_energy = (energy / max_energy) as f64;
        let x = start_x + i * (bar_width + gap);
        let bar_rows = ((norm_energy * chart_height as f64).round() as usize).min(chart_height);

        let is_tonic = name == tonic;
        let is_dom = name == dominant;

        let fg_color = if is_tonic {
            Color::Rgb(80, 250, 123) // Dracula green for tonic
        } else if is_dom {
            Color::Rgb(139, 233, 253) // Dracula cyan for dominant
        } else {
            Color::Rgb(98, 114, 164) // Dracula purple for others
        };

        let style = Style {
            fg: fg_color,
            bold: is_tonic || is_dom,
            ..Default::default()
        };

        // Draw bar from bottom up
        for r in 0..bar_rows {
            let row = (height - 3) as usize - r;
            for col in 0..bar_width {
                grid.set((x + col) as u16, row as u16, '\u{2588}', style);
            }
        }

        // Draw note label at bottom row
        let note_label = format!("{:^3}", name);
        painter::draw_text(&mut grid, x as u16, (height - 2) as u16, &note_label, style);

        // Draw energy percentage at bottom-most row
        let pct = format!("{:>2}%", (norm_energy * 100.0).round() as u32);
        let pct_style = Style {
            fg: if is_tonic || is_dom { fg_color } else { Color::Rgb(120, 120, 120) },
            dim: !is_tonic && !is_dom,
            ..Default::default()
        };
        painter::draw_text(&mut grid, x as u16, (height - 1) as u16, &pct, pct_style);
    }

    Some(render_to_string(&grid))
}

/// Renders a 31-band ISO 1/3-octave equalizer frequency curve with explicit frequency landmark ticks.
pub fn render_terminal_spectrum(analysis: &AudioAnalysis, width: u16, height: u16, theme_name: ThemeName) -> Option<String> {
    let bands = &analysis.spectral.bands_31;
    if bands.len() < 2 {
        return None;
    }

    let theme = Theme::from_name(theme_name);

    let config = ChartConfig {
        chart_type: ChartType::Line,
        width: Some(width),
        height: Some(height),
        theme: theme.clone(),
        show_axis: true,
        ..Default::default()
    };

    let layout = Layout::compute(width, height, &config);
    let mut grid = CellGrid::new(width, height);

    // Title
    painter::draw_text(
        &mut grid,
        2,
        0,
        "31-BAND ISO FREQUENCY SPECTRUM (20 Hz – 20 kHz)",
        Style {
            fg: theme.text_color(),
            bold: true,
            ..Default::default()
        },
    );

    let mut points = Vec::with_capacity(bands.len());
    let mut min_db = 0.0f64;
    let mut max_db = -100.0f64;

    for (i, band) in bands.iter().enumerate() {
        let db = band.energy_db as f64;
        if db < min_db { min_db = db; }
        if db > max_db { max_db = db; }
        points.push(Point { x: i as f64, y: db });
    }

    let viewport = Viewport {
        x_range: Range { min: 0.0, max: (bands.len() - 1) as f64 },
        y_range: Range { min: min_db.min(-30.0), max: max_db.max(0.0) },
        width: layout.chart_area.width,
        height: layout.chart_area.height,
    };

    // Render line chart & Y-axis
    let chart = LineChart::new(points);
    chart.render(&mut grid, &viewport, &config, layout.chart_area.x, layout.chart_area.y).ok()?;
    AxisRenderer.draw_y_axis(&mut grid, &viewport, &layout.y_axis_area, &theme);

    // Render clean X-axis with key frequency landmark ticks
    let axis_style = Style {
        fg: theme.axis_color(),
        dim: true,
        ..Default::default()
    };
    let label_style = Style {
        fg: theme.label_color(),
        ..Default::default()
    };

    let x_start = layout.chart_area.x;
    let x_end = layout.chart_area.right();
    let y_line = layout.chart_area.bottom();

    grid.set(x_start.saturating_sub(1), y_line, '\u{2514}', axis_style);
    for col in x_start..x_end {
        grid.set(col, y_line, '\u{2500}', axis_style);
    }

    // 6 widely-spaced landmark frequencies (band_idx, label)
    // Band 0=20Hz, 7=100Hz, 14=500Hz, 20=2kHz, 26=8kHz, 30=20kHz
    let landmarks = [
        (0usize, "20Hz"),
        (7usize, "100Hz"),
        (14usize, "500Hz"),
        (20usize, "2kHz"),
        (26usize, "8kHz"),
        ((bands.len() - 1), "20kHz"),
    ];

    for (band_idx, label) in landmarks {
        let frac = band_idx as f64 / (bands.len() - 1) as f64;
        let col = x_start + (frac * (layout.chart_area.width.saturating_sub(1)) as f64).round() as u16;

        grid.set(col, y_line, '\u{252C}', axis_style);

        let label_x = if band_idx == 0 {
            col
        } else if band_idx == bands.len() - 1 {
            col.saturating_sub(label.len() as u16 - 1)
        } else {
            col.saturating_sub((label.len() / 2) as u16)
        };

        painter::draw_text(&mut grid, label_x, y_line + 1, label, label_style);
    }

    let chart_str = render_to_string(&grid);

    let footer = format!(
        "   {}\n   {}",
        "20Hz        100Hz         500Hz        2kHz        8kHz     20kHz".dimmed(),
        "[ Sub-Bass ] [    Bass    ] [  Low-Mid  ] [   Mids   ] [ High-Mid / Air ]".cyan()
    );

    Some(format!("{}\n{}", chart_str, footer))
}
