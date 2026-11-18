use std::f32::consts::PI;
use std::fs::File;
use std::io::Write;
use layacore::{analyze_file_simple, AnalysisOptions};

fn create_test_au_file(path: &std::path::Path) -> std::io::Result<()> {
    let sample_rate = 44100u32;
    let channels = 2u32;
    let duration_secs = 1.0f32;
    let num_frames = (sample_rate as f32 * duration_secs) as usize;

    let mut file = File::create(path)?;

    // 24-byte Sun AU header
    // Magic: 0x2e736e64 (".snd")
    file.write_all(&0x2e736e64u32.to_be_bytes())?;
    // Data offset: 24 bytes
    file.write_all(&24u32.to_be_bytes())?;
    // Data size: num_frames * channels * 2 bytes (16-bit PCM)
    let data_size = (num_frames * channels as usize * 2) as u32;
    file.write_all(&data_size.to_be_bytes())?;
    // Encoding: 3 (16-bit linear PCM)
    file.write_all(&3u32.to_be_bytes())?;
    // Sample rate
    file.write_all(&sample_rate.to_be_bytes())?;
    // Channels
    file.write_all(&channels.to_be_bytes())?;

    // Generate stereo audio data
    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        // Left: 440 Hz sine at -6 dBFS (0.5 amplitude)
        let left = (0.5 * (2.0 * PI * 440.0 * t).sin() * 32767.0) as i16;
        // Right: 440 Hz sine at -6 dBFS in phase
        let right = (0.5 * (2.0 * PI * 440.0 * t).sin() * 32767.0) as i16;

        file.write_all(&left.to_be_bytes())?;
        file.write_all(&right.to_be_bytes())?;
    }

    Ok(())
}

#[test]
fn test_analyze_au_file() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_audio.au");

    create_test_au_file(&test_file).expect("Failed to create test AU file");

    let options = AnalysisOptions::default();
    let result = analyze_file_simple(&test_file, &options).expect("Analysis failed");

    // Validate metadata
    assert_eq!(result.metadata.channels, 2);
    assert_eq!(result.metadata.sample_rate, 44100);
    assert_eq!(result.metadata.format_name, "Sun/NeXT AU");

    // Validate dynamics
    assert!((result.time_domain.overall_peak_dbfs - (-6.02)).abs() < 0.2);
    assert_eq!(result.time_domain.total_clipped_samples, 0);

    // Validate stereo: identical left and right -> phase correlation = 1.0
    assert!((result.stereo.phase_correlation - 1.0).abs() < 0.05);

    // Clean up
    let _ = std::fs::remove_file(test_file);
}
