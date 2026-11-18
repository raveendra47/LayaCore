use std::path::Path;
use plotters::prelude::*;

use crate::error::AnalyzerError;
use crate::AudioAnalysis;

const DARK_BG: RGBColor = RGBColor(22, 24, 30);
const PANEL_BG: RGBColor = RGBColor(30, 33, 42);
const GRID_COLOR: RGBColor = RGBColor(50, 55, 70);
const TEXT_COLOR: RGBColor = RGBColor(200, 205, 220);
const ACCENT_CYAN: RGBColor = RGBColor(0, 210, 255);
const ACCENT_YELLOW: RGBColor = RGBColor(255, 204, 0);
const ACCENT_RED: RGBColor = RGBColor(255, 70, 70);
const ACCENT_PURPLE: RGBColor = RGBColor(160, 100, 255);
const ACCENT_GOLD: RGBColor = RGBColor(255, 215, 0);

/// Generate a high-resolution dark-mode mastering dashboard image (PNG or SVG)
pub fn generate_visual_charts(analysis: &AudioAnalysis, output_path: &Path) -> Result<(), AnalyzerError> {
    let extension = output_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "png".to_string());

    if extension == "svg" {
        let root = SVGBackend::new(output_path, (1400, 950)).into_drawing_area();
        render_dashboard(analysis, &root)?;
    } else {
        let root = BitMapBackend::new(output_path, (1400, 950)).into_drawing_area();
        render_dashboard(analysis, &root)?;
    }

    Ok(())
}

fn render_dashboard<DB: DrawingBackend>(
    analysis: &AudioAnalysis,
    root: &DrawingArea<DB, plotters::coord::Shift>,
) -> Result<(), AnalyzerError>
where
    DB::ErrorType: 'static,
{
    root.fill(&DARK_BG)
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters draw error: {:?}", e)))?;

    // Layout: Top 42% = Waveform, Bottom 58% split horizontally into Spectrum & Chroma
    let (top_area, bottom_area) = root.split_vertically(400);
    let (bottom_left, bottom_right) = bottom_area.split_horizontally(700);

    // 1. TOP PANEL: Time-Domain Waveform Envelope
    render_waveform_chart(analysis, &top_area)?;

    // 2. BOTTOM LEFT PANEL: 31-Band ISO Equalizer Spectrum
    render_spectrum_chart(analysis, &bottom_left)?;

    // 3. BOTTOM RIGHT PANEL: 12-Tone Chroma & Key Profile
    render_chroma_chart(analysis, &bottom_right)?;

    root.present()
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters present error: {:?}", e)))?;

    Ok(())
}

fn render_waveform_chart<DB: DrawingBackend>(
    analysis: &AudioAnalysis,
    area: &DrawingArea<DB, plotters::coord::Shift>,
) -> Result<(), AnalyzerError>
where
    DB::ErrorType: 'static,
{
    let buckets = &analysis.waveform.buckets;
    let duration = analysis.metadata.duration_seconds.unwrap_or(1.0) as f32;
    let title = format!(
        "Waveform Peak Envelope & RMS  •  {} (LUFS: {:.1}, True Peak: {:.2} dBTP)",
        analysis.metadata.file_name,
        analysis.lufs.integrated_lufs,
        analysis.true_peak.max_true_peak_dbtp
    );

    area.fill(&PANEL_BG)
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters fill error: {:?}", e)))?;

    let mut chart = ChartBuilder::on(area)
        .caption(title, ("sans-serif", 18).into_font().color(&TEXT_COLOR))
        .margin(15)
        .set_label_area_size(LabelAreaPosition::Left, 45)
        .set_label_area_size(LabelAreaPosition::Bottom, 35)
        .build_cartesian_2d(0.0f32..duration, -1.05f32..1.05f32)
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters chart error: {:?}", e)))?;

    chart
        .configure_mesh()
        .bold_line_style(&GRID_COLOR)
        .light_line_style(&GRID_COLOR)
        .axis_style(&TEXT_COLOR)
        .label_style(("sans-serif", 12).into_font().color(&TEXT_COLOR))
        .x_desc("Time (seconds)")
        .y_desc("Amplitude")
        .draw()
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters mesh error: {:?}", e)))?;

    if buckets.is_empty() {
        return Ok(());
    }

    let num_buckets = buckets.len() as f32;
    let time_step = duration / num_buckets;

    // Draw Min/Max Peak Area
    let mut min_series = Vec::with_capacity(buckets.len());
    let mut max_series = Vec::with_capacity(buckets.len());
    let mut rms_series = Vec::with_capacity(buckets.len());
    let mut rms_neg_series = Vec::with_capacity(buckets.len());
    let mut clip_points = Vec::new();

    for (i, b) in buckets.iter().enumerate() {
        let t = i as f32 * time_step;
        min_series.push((t, b.min));
        max_series.push((t, b.max));
        rms_series.push((t, b.rms));
        rms_neg_series.push((t, -b.rms));

        if b.max >= 0.999 || b.min <= -0.999 {
            clip_points.push((t, if b.max >= 0.999 { b.max } else { b.min }));
        }
    }

    // Positive peak envelope
    chart
        .draw_series(LineSeries::new(max_series, &ACCENT_CYAN))
        .map_err(|e| AnalyzerError::InvalidData(format!("Draw max series error: {:?}", e)))?;

    // Negative peak envelope
    chart
        .draw_series(LineSeries::new(min_series, &ACCENT_CYAN))
        .map_err(|e| AnalyzerError::InvalidData(format!("Draw min series error: {:?}", e)))?;

    // RMS Level (Yellow)
    chart
        .draw_series(LineSeries::new(rms_series, &ACCENT_YELLOW))
        .map_err(|e| AnalyzerError::InvalidData(format!("Draw RMS series error: {:?}", e)))?
        .label("RMS Power")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &ACCENT_YELLOW));

    chart
        .draw_series(LineSeries::new(rms_neg_series, &ACCENT_YELLOW))
        .map_err(|e| AnalyzerError::InvalidData(format!("Draw -RMS series error: {:?}", e)))?;

    // Red dots for clipping events
    if !clip_points.is_empty() {
        chart
            .draw_series(
                clip_points
                    .into_iter()
                    .map(|coord| Circle::new(coord, 2, ACCENT_RED.filled())),
            )
            .map_err(|e| AnalyzerError::InvalidData(format!("Draw clip points error: {:?}", e)))?
            .label("Clipping Event")
            .legend(|(x, y)| Circle::new((x + 10, y), 3, ACCENT_RED.filled()));
    }

    chart
        .configure_series_labels()
        .background_style(&PANEL_BG)
        .border_style(&GRID_COLOR)
        .label_font(("sans-serif", 11).into_font().color(&TEXT_COLOR))
        .position(SeriesLabelPosition::UpperRight)
        .draw()
        .map_err(|e| AnalyzerError::InvalidData(format!("Draw legend error: {:?}", e)))?;

    Ok(())
}

fn render_spectrum_chart<DB: DrawingBackend>(
    analysis: &AudioAnalysis,
    area: &DrawingArea<DB, plotters::coord::Shift>,
) -> Result<(), AnalyzerError>
where
    DB::ErrorType: 'static,
{
    let bands = &analysis.spectral.bands_31;
    let title = format!(
        "31-Band ISO Equalizer Spectrum  •  Centroid: {:.0} Hz, Rolloff: {:.0} Hz",
        analysis.spectral.spectral_centroid_hz, analysis.spectral.spectral_rolloff_hz
    );

    area.fill(&PANEL_BG)
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters fill error: {:?}", e)))?;

    let mut chart = ChartBuilder::on(area)
        .caption(title, ("sans-serif", 16).into_font().color(&TEXT_COLOR))
        .margin(15)
        .set_label_area_size(LabelAreaPosition::Left, 45)
        .set_label_area_size(LabelAreaPosition::Bottom, 40)
        .build_cartesian_2d(0..bands.len(), -65.0f32..3.0f32)
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters chart error: {:?}", e)))?;

    chart
        .configure_mesh()
        .bold_line_style(&GRID_COLOR)
        .light_line_style(&GRID_COLOR)
        .axis_style(&TEXT_COLOR)
        .label_style(("sans-serif", 10).into_font().color(&TEXT_COLOR))
        .x_desc("ISO Frequency Band (Hz)")
        .y_desc("Relative Level (dB)")
        .x_label_formatter(&|&idx| {
            if idx < bands.len() && (idx % 3 == 0 || idx == bands.len() - 1) {
                let hz = bands[idx].center_hz;
                if hz >= 1000.0 {
                    format!("{:.0}k", hz / 1000.0)
                } else {
                    format!("{:.0}", hz)
                }
            } else {
                "".to_string()
            }
        })
        .draw()
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters mesh error: {:?}", e)))?;

    // Render bars
    for (i, b) in bands.iter().enumerate() {
        let color = if b.center_hz < 60.0 {
            RGBColor(60, 120, 255) // Sub-bass: Blue
        } else if b.center_hz < 250.0 {
            ACCENT_CYAN // Bass: Cyan
        } else if b.center_hz < 2000.0 {
            ACCENT_PURPLE // Mids: Purple
        } else if b.center_hz < 6000.0 {
            ACCENT_YELLOW // High-Mids: Yellow
        } else {
            RGBColor(255, 120, 50) // Brilliance: Orange
        };

        let db = b.energy_db.clamp(-65.0, 0.0);
        chart
            .draw_series(std::iter::once(Rectangle::new(
                [(i, -65.0), (i + 1, db)],
                color.filled(),
            )))
            .map_err(|e| AnalyzerError::InvalidData(format!("Draw spectrum bar error: {:?}", e)))?;
    }

    Ok(())
}

fn render_chroma_chart<DB: DrawingBackend>(
    analysis: &AudioAnalysis,
    area: &DrawingArea<DB, plotters::coord::Shift>,
) -> Result<(), AnalyzerError>
where
    DB::ErrorType: 'static,
{
    let chroma = &analysis.musical.chroma_profile;
    let notes = crate::spectral::NOTE_NAMES;
    let detected_key = &analysis.musical.estimated_key;
    let confidence = (analysis.musical.key_confidence * 100.0).round();
    let bpm = analysis.musical.estimated_bpm;

    let title = format!(
        "Musical Key & Pitch Class (Chroma)  •  Key: {} ({}%), BPM: {:.0}",
        detected_key, confidence, bpm
    );

    let max_energy = chroma.iter().copied().fold(1e-6f32, f32::max);

    area.fill(&PANEL_BG)
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters fill error: {:?}", e)))?;

    let mut chart = ChartBuilder::on(area)
        .caption(title, ("sans-serif", 16).into_font().color(&TEXT_COLOR))
        .margin(15)
        .set_label_area_size(LabelAreaPosition::Left, 45)
        .set_label_area_size(LabelAreaPosition::Bottom, 40)
        .build_cartesian_2d(0usize..12usize, 0.0f32..(max_energy * 1.15))
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters chart error: {:?}", e)))?;

    chart
        .configure_mesh()
        .bold_line_style(&GRID_COLOR)
        .light_line_style(&GRID_COLOR)
        .axis_style(&TEXT_COLOR)
        .label_style(("sans-serif", 11).into_font().color(&TEXT_COLOR))
        .x_desc("Semitone Note")
        .y_desc("Normalized Energy")
        .x_label_formatter(&|&idx: &usize| {
            if idx < 12 {
                notes[idx].to_string()
            } else {
                "".to_string()
            }
        })
        .draw()
        .map_err(|e| AnalyzerError::InvalidData(format!("Plotters mesh error: {:?}", e)))?;

    // Determine tonic note from estimated_key (e.g. "D Minor" -> "D")
    let tonic_name = detected_key.split_whitespace().next().unwrap_or("");

    for i in 0..12 {
        let is_tonic = notes[i] == tonic_name;
        let color = if is_tonic {
            ACCENT_GOLD // Highlight Key tonic in Gold
        } else {
            ACCENT_CYAN
        };

        let val = chroma.get(i).copied().unwrap_or(0.0);
        chart
            .draw_series(std::iter::once(Rectangle::new(
                [(i, 0.0), (i + 1, val)],
                color.filled(),
            )))
            .map_err(|e| AnalyzerError::InvalidData(format!("Draw chroma bar error: {:?}", e)))?;
    }

    Ok(())
}
