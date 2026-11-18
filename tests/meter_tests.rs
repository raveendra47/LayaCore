use std::f32::consts::PI;
use layacore::meters::{
    LufsMeter, StereoMeter, TimeDomainMeter, TruePeakMeter, WaveformGenerator,
};

fn generate_sine(frequency: f32, sample_rate: u32, duration_secs: f32, amplitude: f32) -> Vec<f32> {
    let total_samples = (sample_rate as f32 * duration_secs) as usize;
    (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            amplitude * (2.0 * PI * frequency * t).sin()
        })
        .collect()
}

#[test]
fn test_time_domain_sine_wave() {
    let sample_rate = 48000;
    let sine = generate_sine(1000.0, sample_rate, 1.0, 0.95); // 0.95 amplitude 1 kHz sine

    let mut meter = TimeDomainMeter::new(1, sample_rate);
    meter.process_frames(&sine);
    let stats = meter.finish();

    // Peak should be ~ -0.45 dBFS (amplitude 0.95)
    assert!((stats.overall_peak_dbfs - (-0.45)).abs() < 0.05);

    // RMS of sine wave with amplitude 0.95 is 0.95/sqrt(2) = -3.45 dBFS
    assert!((stats.overall_rms_dbfs - (-3.45)).abs() < 0.1);

    // Crest factor of sine wave is sqrt(2) = 3.01 dB
    assert!((stats.dynamic_range_db - 3.01).abs() < 0.1);

    // No clipping on clean 0.95 sine
    assert_eq!(stats.total_clipped_samples, 0);
    assert_eq!(stats.clipping_events, 0);
}

#[test]
fn test_clipping_detection() {
    let sample_rate = 48000;
    // Signal with 10 consecutive full-scale samples
    let mut samples = vec![0.0f32; 100];
    for s in &mut samples[40..50] {
        *s = 1.0;
    }

    let mut meter = TimeDomainMeter::new(1, sample_rate);
    meter.process_frames(&samples);
    let stats = meter.finish();

    assert_eq!(stats.total_clipped_samples, 10);
    assert!(stats.clipping_events >= 1);
}

#[test]
fn test_stereo_phase_correlation() {
    let sample_rate = 48000;
    let sine = generate_sine(440.0, sample_rate, 0.5, 0.8);

    // 1. In-phase stereo (pure mono: L == R)
    let mut in_phase_interleaved = Vec::new();
    for &s in &sine {
        in_phase_interleaved.push(s);
        in_phase_interleaved.push(s);
    }

    let mut stereo_meter = StereoMeter::new(2);
    stereo_meter.process_frames(&in_phase_interleaved, 2);
    let stats = stereo_meter.finish();

    assert!((stats.phase_correlation - 1.0).abs() < 0.01);
    assert!(!stats.mono_compatibility_risk);
    assert!(stats.stereo_width_percent < 1.0); // ~0% width

    // 2. Out-of-phase stereo (L == -R) -> Complete mono cancellation!
    let mut out_of_phase_interleaved = Vec::new();
    for &s in &sine {
        out_of_phase_interleaved.push(s);
        out_of_phase_interleaved.push(-s);
    }

    let mut meter_out = StereoMeter::new(2);
    meter_out.process_frames(&out_of_phase_interleaved, 2);
    let stats_out = meter_out.finish();

    assert!((stats_out.phase_correlation - (-1.0)).abs() < 0.01);
    assert!(stats_out.mono_compatibility_risk);
}

#[test]
fn test_true_peak_intersample_clipping() {
    // Construct samples where consecutive peaks cause analog overshoots
    let mut meter = TruePeakMeter::new(1);
    // [0.0, 0.9, 0.9, 0.0] - continuous interpolation between two 0.9 peaks overshoots 1.0!
    let samples = vec![0.0f32, 0.85, 0.85, 0.0, 0.0, -0.85, -0.85, 0.0];
    meter.process_frames(&samples);
    let stats = meter.finish();

    // True Peak should exceed the individual sample peak
    assert!(stats.max_true_peak_linear >= 0.85);
}

#[test]
fn test_waveform_generator() {
    let sample_rate = 1000;
    let samples = vec![0.5f32; 1000];

    let mut wf_gen = WaveformGenerator::new(1, sample_rate, 10);
    wf_gen.process_frames(&samples);
    let wf = wf_gen.finish();

    assert_eq!(wf.buckets.len(), 10);
    for b in wf.buckets {
        assert!((b.max - 0.5).abs() < 0.01);
    }
}

#[test]
fn test_lufs_sine_wave() {
    let sample_rate = 48000;
    // 1 kHz sine with amplitude 1.0 at 48kHz for 2 seconds
    let sine = generate_sine(1000.0, sample_rate, 2.0, 1.0);
    let mut meter = LufsMeter::new(1, sample_rate);
    meter.process_frames(&sine);
    let stats = meter.finish();

    println!("Integrated LUFS: {}", stats.integrated_lufs);
}
