use std::f32::consts::PI;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use layacore::{
    analyze_file_simple, generate_mastering_advice, AdviceSeverity, AnalysisOptions,
    LosslessVerdict,
};

fn create_wav(
    path: &Path,
    freq_hz: f32,
    amp: f32,
    invert_right: bool,
    duration_secs: f32,
) -> std::io::Result<()> {
    let sr = 44100u32;
    let num_frames = (sr as f32 * duration_secs) as usize;
    let data_bytes = (num_frames * 4) as u32;

    let mut f = File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_bytes).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_bytes.to_le_bytes())?;

    for i in 0..num_frames {
        let t = i as f32 / sr as f32;
        let l = (amp * (2.0 * PI * freq_hz * t).sin()).clamp(-1.0, 1.0);
        let r = if invert_right { -l } else { l };
        f.write_all(&((l * 32767.0) as i16).to_le_bytes())?;
        f.write_all(&((r * 32767.0) as i16).to_le_bytes())?;
    }

    Ok(())
}

#[test]
fn test_advisor_detects_clipping_and_phase_cancellation() {
    let dir = std::env::temp_dir().join("audio_analyzer_advisor_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // 1. Hot clipping & out-of-phase file -> critical issues & low score
    let bad_wav = dir.join("bad_mix.wav");
    create_wav(&bad_wav, 440.0, 1.0, true, 1.0).unwrap();

    let bad = analyze_file_simple(&bad_wav, &AnalysisOptions::default()).unwrap();
    assert!(
        bad.advisor.readiness_score < 60,
        "Expected low score for clipping + phase-inverted track, got {}",
        bad.advisor.readiness_score
    );
    assert!(
        bad.advisor
            .items
            .iter()
            .any(|i| i.category == "HEADROOM" && i.severity == AdviceSeverity::Critical),
        "Expected Critical HEADROOM advice"
    );
    assert!(
        bad.advisor
            .items
            .iter()
            .any(|i| i.category == "STEREO" && i.severity == AdviceSeverity::Critical),
        "Expected Critical STEREO phase advice"
    );

    // 2. Clean headroom, in-phase file -> high score & clean headroom advice
    let clean_wav = dir.join("clean_mix.wav");
    create_wav(&clean_wav, 440.0, 0.35, false, 1.0).unwrap();

    let clean = analyze_file_simple(&clean_wav, &AnalysisOptions::default()).unwrap();
    assert!(
        clean.advisor.readiness_score >= 85,
        "Expected high score for clean track, got {}",
        clean.advisor.readiness_score
    );
    assert!(
        clean
            .advisor
            .items
            .iter()
            .any(|i| i.category == "HEADROOM" && i.severity == AdviceSeverity::Good),
        "Expected Good HEADROOM advice"
    );

    // 3. Simulated FLAC fake lossless transcode -> Critical AUTHENTICITY advice
    let mut flac_meta = clean.metadata.clone();
    flac_meta.file_name = "upscaled.flac".into();
    flac_meta.format_name = "FLAC Container".into();
    flac_meta.codec_name = "FLAC".into();

    let mut fake_quality = clean.quality.clone();
    fake_quality.verdict = LosslessVerdict::SuspectedTranscode128k;
    fake_quality.detected_cutoff_hz = Some(16000.0);

    let flac_adv = generate_mastering_advice(
        &flac_meta,
        &clean.time_domain,
        &clean.stereo,
        &clean.lufs,
        &clean.true_peak,
        &clean.spectral,
        &fake_quality,
    );
    assert!(
        flac_adv
            .items
            .iter()
            .any(|i| i.category == "AUTHENTICITY" && i.severity == AdviceSeverity::Critical),
        "Expected Critical AUTHENTICITY advice for fake FLAC"
    );

    let _ = fs::remove_dir_all(&dir);
}
