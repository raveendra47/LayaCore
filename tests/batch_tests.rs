use std::f32::consts::PI;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use layacore::report::{format_batch_csv, format_batch_json, print_batch_report};
use layacore::{
    discover_audio_files, scan_directory, scan_directory_simple, AlertSeverity, BatchScanOptions,
};

fn create_test_wav(path: &Path, freq_hz: f32, amplitude: f32, duration_secs: f32) -> std::io::Result<()> {
    let sample_rate = 44100u32;
    let channels = 2u16;
    let bits_per_sample = 16u16;
    let num_frames = (sample_rate as f32 * duration_secs) as usize;
    let data_bytes = (num_frames * channels as usize * (bits_per_sample as usize / 8)) as u32;

    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?; // PCM
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * channels as u32 * 2).to_le_bytes())?;
    file.write_all(&(channels * 2).to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())?;

    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        let sample_f = (amplitude * (2.0 * PI * freq_hz * t).sin()).clamp(-1.0, 1.0);
        let sample_i16 = (sample_f * 32767.0) as i16;
        file.write_all(&sample_i16.to_le_bytes())?;
        file.write_all(&sample_i16.to_le_bytes())?;
    }

    Ok(())
}

#[test]
fn test_batch_folder_discovery_and_scan() {
    let temp_root = std::env::temp_dir().join("audio_analyzer_batch_test");
    let _ = fs::remove_dir_all(&temp_root);
    let sub_dir = temp_root.join("Disc2");
    fs::create_dir_all(&sub_dir).expect("create temp dirs");

    // Track 1: Quiet track (-18 dBFS approx)
    let t1 = temp_root.join("01_quiet.wav");
    create_test_wav(&t1, 440.0, 0.12, 1.2).expect("write t1");

    // Track 2: Loud clipping track (amplitude 1.0 -> clips)
    let t2 = temp_root.join("02_loud.wav");
    create_test_wav(&t2, 880.0, 1.0, 1.2).expect("write t2");

    // Track 3: Nested subdirectory track
    let t3 = sub_dir.join("03_nested.wav");
    create_test_wav(&t3, 660.0, 0.35, 1.2).expect("write t3");

    // Non-audio file that must be ignored
    fs::write(temp_root.join("cover.txt"), b"album liner notes").expect("write txt");

    // Corrupted audio file that should be recorded as failed without crashing scan
    fs::write(temp_root.join("04_corrupted.wav"), b"NOT_A_VALID_WAV_HEADER").expect("write corrupt");

    // 1. Non-recursive discovery should find 3 wav files (01, 02, 04)
    let top_files = discover_audio_files(&temp_root, false).expect("non-recursive discovery");
    assert_eq!(top_files.len(), 3);

    // 2. Recursive discovery should find 4 wav files (01, 02, 04, Disc2/03)
    let all_files = discover_audio_files(&temp_root, true).expect("recursive discovery");
    assert_eq!(all_files.len(), 4);

    // 3. Run parallel batch scan with progress callback
    let progress_hits = AtomicUsize::new(0);
    let options = BatchScanOptions {
        recursive: true,
        max_threads: Some(2),
        ..Default::default()
    };

    let report = scan_directory(
        &temp_root,
        &options,
        Some(|done: usize, total: usize, _path: &Path| {
            assert!(done <= total);
            assert_eq!(total, 4);
            progress_hits.fetch_add(1, Ordering::Relaxed);
        }),
    )
    .expect("batch scan succeeded");

    assert_eq!(progress_hits.load(Ordering::Relaxed), 4);
    assert_eq!(report.summary.total_files_found, 4);
    assert_eq!(report.summary.succeeded, 3);
    assert_eq!(report.summary.failed, 1);
    assert!(report.summary.lufs_spread_lu > 5.0, "Expected noticeable LUFS spread between quiet and loud track");
    assert!(report.summary.clipping_tracks >= 1, "Expected loud 1.0 amplitude track to trigger clipping");

    // Verify alerts include decode error, clipping, and loudness outlier
    assert!(
        report
            .summary
            .alerts
            .iter()
            .any(|a| a.category == "DECODE ERROR" && a.severity == AlertSeverity::Error),
        "Expected DECODE ERROR alert for corrupted wav"
    );
    assert!(
        report
            .summary
            .alerts
            .iter()
            .any(|a| a.category == "CLIPPING"),
        "Expected CLIPPING alert"
    );
    assert!(
        report
            .summary
            .alerts
            .iter()
            .any(|a| a.category == "LOUDNESS OUTLIER"),
        "Expected LOUDNESS OUTLIER alert"
    );

    // 4. Verify JSON & CSV formatting
    let json_str = format_batch_json(&report, true).expect("batch json");
    assert!(json_str.contains("01_quiet.wav"));
    assert!(json_str.contains("04_corrupted.wav"));

    let csv_str = format_batch_csv(&report);
    let csv_lines: Vec<_> = csv_str.lines().collect();
    assert_eq!(csv_lines.len(), 5); // 1 header + 4 tracks
    assert!(csv_lines[0].starts_with("Track,RelativePath,Format,"));

    // 5. Verify terminal rendering in both modes does not panic
    print_batch_report(&report, false);
    print_batch_report(&report, true);

    // 6. Verify non-recursive simple scan
    let non_rec_opts = BatchScanOptions {
        recursive: false,
        ..Default::default()
    };
    let non_rec_report = scan_directory_simple(&temp_root, &non_rec_opts).expect("non-recursive scan");
    assert_eq!(non_rec_report.summary.total_files_found, 3);
    assert_eq!(non_rec_report.summary.succeeded, 2);
    assert_eq!(non_rec_report.summary.failed, 1);

    let _ = fs::remove_dir_all(&temp_root);
}
