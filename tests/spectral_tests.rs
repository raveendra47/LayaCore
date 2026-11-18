use std::f32::consts::PI;
use layacore::spectral::{LosslessVerdict, SpectralMeter};

fn generate_sine(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let total_samples = (sample_rate as f32 * duration_secs) as usize;
    (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * PI * frequency * t).sin()
        })
        .collect()
}

#[test]
fn test_spectral_centroid_accuracy() {
    let sample_rate = 44100;
    // 1000 Hz pure sine wave
    let sine = generate_sine(1000.0, sample_rate, 1.0);

    let mut meter = SpectralMeter::new(1, sample_rate);
    meter.process_frames(&sine);
    let (spectral, _, _) = meter.finish(false);

    // Centroid should be within 100 Hz of 1000 Hz
    assert!(
        (spectral.spectral_centroid_hz - 1000.0).abs() < 100.0,
        "Expected ~1000 Hz centroid, got {:.1} Hz",
        spectral.spectral_centroid_hz
    );

    // Pure sine wave must have very low spectral flatness (< 0.05)
    assert!(
        spectral.spectral_flatness < 0.05,
        "Expected low flatness for sine, got {:.3}",
        spectral.spectral_flatness
    );
}

#[test]
fn test_key_detection_single_note() {
    let sample_rate = 44100;
    // 440 Hz is A4
    let sine = generate_sine(440.0, sample_rate, 2.0);

    let mut meter = SpectralMeter::new(1, sample_rate);
    meter.process_frames(&sine);
    let (_, musical, _) = meter.finish(false);

    // Estimated key should identify A (Major or Minor)
    assert!(
        musical.estimated_key.starts_with('A'),
        "Expected key starting with A for 440 Hz tone, got {}",
        musical.estimated_key
    );
}

#[test]
fn test_transcode_detection_128k_shelf() {
    let sample_rate = 44100;
    let duration_secs = 2.0;
    let total_samples = (sample_rate as f32 * duration_secs) as usize;

    // Synthesize audio with strong harmonics up to 15.8 kHz, but zero energy above 16.0 kHz (128 kbps MP3 transcode simulation)
    let mut signal = vec![0.0f32; total_samples];
    let freqs = [500.0, 1000.0, 4000.0, 8000.0, 12000.0, 15000.0, 15800.0];
    for &f in &freqs {
        for (i, sample) in signal.iter_mut().enumerate() {
            let t = i as f32 / sample_rate as f32;
            *sample += (2.0 * PI * f * t).sin() * 0.1;
        }
    }

    let mut meter = SpectralMeter::new(1, sample_rate);
    meter.process_frames(&signal);
    let (_, _, quality) = meter.finish(false);

    assert!(
        quality.detected_cutoff_hz.is_some(),
        "Expected cutoff frequency detection"
    );
    let cutoff = quality.detected_cutoff_hz.unwrap();
    assert!(
        (cutoff - 16000.0).abs() < 1000.0,
        "Expected cutoff near 16 kHz, got {:.1} Hz",
        cutoff
    );
    assert_eq!(
        quality.verdict,
        LosslessVerdict::SuspectedTranscode128k,
        "Expected 128k transcode verdict"
    );
}

#[test]
fn test_transcode_detection_320k_shelf() {
    let sample_rate = 44100;
    let duration_secs = 2.0;
    let total_samples = (sample_rate as f32 * duration_secs) as usize;

    // Harmonics up to 20.0 kHz, silence above 20.3 kHz (320 kbps MP3 transcode simulation)
    let mut signal = vec![0.0f32; total_samples];
    let freqs = [1000.0, 5000.0, 10000.0, 15000.0, 18000.0, 19900.0];
    for &f in &freqs {
        for (i, sample) in signal.iter_mut().enumerate() {
            let t = i as f32 / sample_rate as f32;
            *sample += (2.0 * PI * f * t).sin() * 0.1;
        }
    }

    let mut meter = SpectralMeter::new(1, sample_rate);
    meter.process_frames(&signal);
    let (_, _, quality) = meter.finish(false);

    assert!(
        quality.detected_cutoff_hz.is_some(),
        "Expected cutoff frequency detection near 20 kHz"
    );
    assert_eq!(
        quality.verdict,
        LosslessVerdict::SuspectedTranscode320k,
        "Expected 320k transcode verdict"
    );
}

#[test]
fn test_transcode_detection_genuine_lossless() {
    let sample_rate = 44100;
    let duration_secs = 2.0;
    let total_samples = (sample_rate as f32 * duration_secs) as usize;

    // Full bandwidth audio with continuous harmonics extending into 21.5 kHz
    let mut signal = vec![0.0f32; total_samples];
    let freqs = [1000.0, 5000.0, 10000.0, 15000.0, 18000.0, 20000.0, 21500.0];
    for &f in &freqs {
        for (i, sample) in signal.iter_mut().enumerate() {
            let t = i as f32 / sample_rate as f32;
            *sample += (2.0 * PI * f * t).sin() * 0.1;
        }
    }

    let mut meter = SpectralMeter::new(1, sample_rate);
    meter.process_frames(&signal);
    let (_, _, quality) = meter.finish(false);

    assert_eq!(
        quality.verdict,
        LosslessVerdict::GenuineLossless,
        "Expected GenuineLossless verdict for full-bandwidth signal"
    );
    assert_eq!(quality.bandwidth_utilized_pct, 100.0);
}
