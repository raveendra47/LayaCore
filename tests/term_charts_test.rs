use std::f32::consts::PI;
use std::fs::File;
use std::io::Write;
use layacore::report::term_charts::{
    render_terminal_chroma, render_terminal_spectrum, render_terminal_waveform,
};
use layacore::{analyze_file_simple, AnalysisOptions};
use termichart_core::ThemeName;

fn create_test_au(path: &std::path::Path) -> std::io::Result<()> {
    let sample_rate = 44100u32;
    let channels = 2u32;
    let duration_secs = 2.0f32;
    let num_frames = (sample_rate as f32 * duration_secs) as usize;

    let mut file = File::create(path)?;
    file.write_all(&0x2e736e64u32.to_be_bytes())?;
    file.write_all(&24u32.to_be_bytes())?;
    let data_size = (num_frames * channels as usize * 2) as u32;
    file.write_all(&data_size.to_be_bytes())?;
    file.write_all(&3u32.to_be_bytes())?;
    file.write_all(&sample_rate.to_be_bytes())?;
    file.write_all(&channels.to_be_bytes())?;

    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        // 440 Hz (A4)
        let sample = (0.7 * (2.0 * PI * 440.0 * t).sin() * 32767.0) as i16;
        file.write_all(&sample.to_be_bytes())?;
        file.write_all(&sample.to_be_bytes())?;
    }

    Ok(())
}

#[test]
fn test_render_terminal_charts() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_term_charts.au");

    create_test_au(&test_file).expect("Failed to create test AU file");

    let options = AnalysisOptions::default();
    let analysis = analyze_file_simple(&test_file, &options).expect("Analysis failed");

    let wf = render_terminal_waveform(&analysis, 72, 10, ThemeName::Dracula);
    assert!(wf.is_some(), "Waveform chart should render");

    let chroma = render_terminal_chroma(&analysis);
    assert!(chroma.is_some(), "Chroma chart should render");

    let spec = render_terminal_spectrum(&analysis, 72, 10, ThemeName::Dracula);
    assert!(spec.is_some(), "Spectrum chart should render");

    let _ = std::fs::remove_file(test_file);
}
