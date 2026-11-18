use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, ContentArrangement, Table};

use termichart_core::ThemeName;

use crate::advisor::AdviceSeverity;
use crate::batch::{AlertSeverity, BatchScanReport};
use crate::meters::waveform::render_ascii_waveform;
use crate::report::term_charts::{
    render_terminal_chroma, render_terminal_spectrum, render_terminal_waveform,
};
use crate::spectral::LosslessVerdict;
use crate::AudioAnalysis;

pub fn print_terminal_report(analysis: &AudioAnalysis) {
    let meta = &analysis.metadata;
    let time = &analysis.time_domain;
    let stereo = &analysis.stereo;
    let lufs = &analysis.lufs;
    let tp = &analysis.true_peak;

    println!("\n{}", "=".repeat(68).bright_cyan());
    println!(
        " {}  {} {}",
        "LAYACORE".bright_white().bold(),
        "•".bright_cyan(),
        meta.file_name.bright_yellow().bold()
    );
    println!("{}\n", "=".repeat(68).bright_cyan());

    // 1. FILE & STREAM METADATA TABLE
    let mut file_table = Table::new();
    file_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    file_table.set_header(vec![
        Cell::new("Property").fg(Color::Cyan),
        Cell::new("Value").fg(Color::Cyan),
    ]);

    let duration_str = meta
        .duration_seconds
        .map(|d| {
            let mins = (d / 60.0).floor() as u64;
            let secs = d % 60.0;
            format!("{}m {:05.2}s ({:.2}s)", mins, secs, d)
        })
        .unwrap_or_else(|| "Unknown".into());

    let bitrate_str = meta
        .bitrate_kbps
        .map(|b| format!("{:.1} kbps", b))
        .unwrap_or_else(|| "N/A".into());

    let size_mb = (meta.file_size_bytes as f64) / (1024.0 * 1024.0);

    let channels_str = match meta.channels {
        1 => "1 (Mono)".to_string(),
        2 => "2 (Stereo)".to_string(),
        c => format!("{} Channels", c),
    };
    file_table.add_row(vec!["Format / Container", &meta.format_name]);
    file_table.add_row(vec!["Audio Codec", &meta.codec_name]);
    file_table.add_row(vec!["Channels", &channels_str]);
    file_table.add_row(vec!["Sample Rate", &format!("{} Hz", meta.sample_rate)]);
    file_table.add_row(vec![
        "Bit Depth",
        &meta
            .bits_per_sample
            .map(|b| format!("{}-bit", b))
            .unwrap_or_else(|| "N/A".into()),
    ]);
    file_table.add_row(vec!["Duration", &duration_str]);
    file_table.add_row(vec!["File Size", &format!("{:.2} MB", size_mb)]);
    file_table.add_row(vec!["Bitrate", &bitrate_str]);

    if let Some(artist) = &meta.artist {
        file_table.add_row(vec!["Artist", artist]);
    }
    if let Some(title) = &meta.title {
        file_table.add_row(vec!["Title", title]);
    }
    if let Some(album) = &meta.album {
        file_table.add_row(vec!["Album", album]);
    }

    let mut sec = 1;
    println!("{}", format!("{}. FILE INFORMATION", sec).bright_cyan().bold());
    println!("{}\n", file_table);

    // 2. FAKE LOSSLESS & LOSSY TRANSCODE DETECTOR (Only included for FLAC audio files)
    if meta.is_flac() {
        sec += 1;
        println!(
            "{}",
            format!("{}. FAKE LOSSLESS & LOSSY TRANSCODE DETECTOR", sec)
                .bright_cyan()
                .bold()
        );

        let qual = &analysis.quality;

        // Hero Card Verdict
        let (icon, title, desc, border_col) = match qual.verdict {
            LosslessVerdict::GenuineLossless => (
                "✔",
                "VERIFIED GENUINE LOSSLESS",
                format!(
                    "Continuous harmonic frequency spectrum extends up to Nyquist limit ({:.1} kHz).\n\
                     No lossy brickwall lowpass filters detected. Audio displays natural acoustic decay.",
                    qual.nyquist_hz / 1000.0
                ),
                Color::Green,
            ),
            LosslessVerdict::SuspectedTranscode128k => (
                "✖",
                "FAKE LOSSLESS DETECTED — 128 kbps MP3 TRANSCODE",
                format!(
                    "Sharp brickwall lowpass cutoff cliff detected at ~{:.1} kHz ({:.1} dB drop into noise floor).\n\
                     This file is stored in a lossless container ({}) but originated from ~128k lossy audio.\n\
                     Frequencies above 16.0 kHz were permanently lost during initial lossy compression.",
                    qual.detected_cutoff_hz.unwrap_or(16000.0) / 1000.0,
                    qual.shelf_drop_db,
                    meta.format_name
                ),
                Color::Red,
            ),
            LosslessVerdict::SuspectedTranscode192k => (
                "✖",
                "SUSPECTED TRANSCODE — 192 kbps MP3 CUTOFF",
                format!(
                    "Steep lowpass cutoff shelf detected at ~{:.1} kHz ({:.1} dB drop into noise floor).\n\
                     High frequencies drop off unnaturally above ~18.5 kHz, characteristic of 192k lossy encoding.",
                    qual.detected_cutoff_hz.unwrap_or(18500.0) / 1000.0,
                    qual.shelf_drop_db
                ),
                Color::Red,
            ),
            LosslessVerdict::SuspectedTranscode320k => (
                "⚠",
                "SUSPECTED TRANSCODE — 320 kbps MP3 CUTOFF",
                format!(
                    "Lowpass brickwall cutoff shelf detected at ~{:.1} kHz ({:.1} dB drop into noise floor).\n\
                     Standard 320 kbps MP3 encoders hard-filter frequency content at ~20.1 - 20.5 kHz.\n\
                     Audio appears to be upscaled from a 320 kbps MP3 master.",
                    qual.detected_cutoff_hz.unwrap_or(20100.0) / 1000.0,
                    qual.shelf_drop_db
                ),
                Color::Yellow,
            ),
            LosslessVerdict::NativeLossy => (
                "ℹ",
                "NATIVE LOSSY CONTAINER",
                format!(
                    "File format is {} (native lossy compression).\n\
                     Transcode verification is intended for lossless containers (FLAC, WAV, ALAC, AIFF).",
                    meta.format_name
                ),
                Color::Cyan,
            ),
            LosslessVerdict::BandwidthLimited => (
                "ℹ",
                "ACOUSTIC BANDWIDTH LIMITED (Natural Roll-off)",
                "High-frequency spectrum rolls off naturally without artificial encoder brickwall filter shelves\n\
                 (typical of acoustic instruments, vintage recordings, or warm mastering).".to_string(),
                Color::Cyan,
            ),
        };

        let card_content = format!("  {}  {}\n\n  {}", icon, title, desc.replace('\n', "\n  "));
        let mut hero_table = Table::new();
        hero_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
        hero_table.add_row(vec![Cell::new(card_content).fg(border_col)]);
        println!("{}\n", hero_table);

        // Authenticity Metrics Table
        let mut qual_table = Table::new();
        qual_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
        qual_table.set_header(vec![
            Cell::new("Authenticity Check").fg(Color::Cyan),
            Cell::new("Measurement / Result").fg(Color::Cyan),
            Cell::new("Confidence").fg(Color::Cyan),
            Cell::new("Verdict / Status").fg(Color::Cyan),
        ]);

        let (status_text, status_color) = match qual.verdict {
            LosslessVerdict::GenuineLossless => ("VERIFIED LOSSLESS", Color::Green),
            LosslessVerdict::SuspectedTranscode320k => ("SUSPECTED 320k", Color::Yellow),
            LosslessVerdict::SuspectedTranscode192k => ("SUSPECTED 192k", Color::Red),
            LosslessVerdict::SuspectedTranscode128k => ("FAKE LOSSLESS (128k)", Color::Red),
            LosslessVerdict::NativeLossy => ("LOSSY CONTAINER", Color::Cyan),
            LosslessVerdict::BandwidthLimited => ("BANDWIDTH LIMITED", Color::DarkCyan),
        };

        let conf_str = format!("{:.0}%", qual.confidence_pct);
        let source_measurement = match qual.verdict {
            LosslessVerdict::GenuineLossless => {
                format!("Continuous to {:.1} kHz Nyquist", qual.nyquist_hz / 1000.0)
            }
            LosslessVerdict::SuspectedTranscode320k => format!(
                "Cutoff shelf at ~{:.1} kHz",
                qual.detected_cutoff_hz.unwrap_or(20100.0) / 1000.0
            ),
            LosslessVerdict::SuspectedTranscode192k => format!(
                "Cutoff shelf at ~{:.1} kHz",
                qual.detected_cutoff_hz.unwrap_or(18500.0) / 1000.0
            ),
            LosslessVerdict::SuspectedTranscode128k => format!(
                "Brickwall cutoff at ~{:.1} kHz",
                qual.detected_cutoff_hz.unwrap_or(16000.0) / 1000.0
            ),
            LosslessVerdict::NativeLossy => format!("Lossy format ({})", meta.format_name),
            LosslessVerdict::BandwidthLimited => "Natural acoustic roll-off".to_string(),
        };
        qual_table.add_row(vec![
            Cell::new("Source Authenticity"),
            Cell::new(source_measurement),
            Cell::new(&conf_str),
            Cell::new(status_text).fg(status_color),
        ]);

        let cutoff_str = match qual.detected_cutoff_hz {
            Some(f) => format!("{:.1} kHz (Brickwall shelf)", f / 1000.0),
            None => format!("None (Full to {:.1} kHz Nyquist)", qual.nyquist_hz / 1000.0),
        };
        let cutoff_status = match qual.verdict {
            LosslessVerdict::GenuineLossless => Cell::new("FULL BANDWIDTH").fg(Color::Green),
            LosslessVerdict::BandwidthLimited => Cell::new("NATURAL ROLLOFF").fg(Color::Green),
            _ => Cell::new("CUTOFF DETECTED").fg(status_color),
        };
        qual_table.add_row(vec![
            Cell::new("Spectral Cutoff Shelf"),
            Cell::new(cutoff_str),
            Cell::new(&conf_str),
            cutoff_status,
        ]);

        qual_table.add_row(vec![
            Cell::new("Nyquist Utilization"),
            Cell::new(format!(
                "{:.1}% ({:.1} kHz of {:.1} kHz)",
                qual.bandwidth_utilized_pct,
                (qual.bandwidth_utilized_pct / 100.0 * qual.nyquist_hz) / 1000.0,
                qual.nyquist_hz / 1000.0
            )),
            Cell::new("N/A"),
            Cell::new(if qual.bandwidth_utilized_pct >= 95.0 {
                "FULL"
            } else {
                "TRUNCATED"
            })
            .fg(if qual.bandwidth_utilized_pct >= 95.0 {
                Color::Green
            } else {
                Color::Yellow
            }),
        ]);

        qual_table.add_row(vec![
            Cell::new("Shelf Cliff Drop"),
            Cell::new(format!("{:.1} dB drop into noise floor", qual.shelf_drop_db)),
            Cell::new(&conf_str),
            if qual.shelf_drop_db >= 16.0 {
                Cell::new("STEEP CLIFF (>= 16 dB)").fg(Color::Red)
            } else {
                Cell::new("GRADUAL (< 16 dB)").fg(Color::Green)
            },
        ]);

        println!("{}\n", qual_table);

        // If a transcode was detected on this FLAC file, also show the Reference Guide
        if matches!(
            qual.verdict,
            LosslessVerdict::SuspectedTranscode128k
                | LosslessVerdict::SuspectedTranscode192k
                | LosslessVerdict::SuspectedTranscode320k
        ) {
            let mut ref_table = Table::new();
            ref_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
            ref_table.set_header(vec![
                Cell::new("Format / Bitrate").fg(Color::Cyan),
                Cell::new("Cutoff Shelf").fg(Color::Cyan),
                Cell::new("Spectral Characteristics").fg(Color::Cyan),
            ]);
            ref_table.add_row(vec![
                Cell::new("MP3 128 kbps"),
                Cell::new("~16.0 kHz"),
                Cell::new("Hard brickwall; near-silence above 16 kHz"),
            ]);
            ref_table.add_row(vec![
                Cell::new("MP3 192 kbps"),
                Cell::new("~18.5 - 19.0 kHz"),
                Cell::new("Steep shelf; sharp cutoff"),
            ]);
            ref_table.add_row(vec![
                Cell::new("MP3 320 kbps"),
                Cell::new("~20.1 - 20.5 kHz"),
                Cell::new("Hard brickwall cutoff below 20.5 kHz"),
            ]);
            ref_table.add_row(vec![
                Cell::new("Genuine Lossless"),
                Cell::new("Full Nyquist (22k+)"),
                Cell::new("Continuous harmonic acoustic decay"),
            ]);
            println!("{}\n", ref_table);
        }
    }

    // LOUDNESS & EBU R128 TABLE
    let mut lufs_table = Table::new();
    lufs_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    lufs_table.set_header(vec![
        Cell::new("Metric").fg(Color::Cyan),
        Cell::new("Value").fg(Color::Cyan),
        Cell::new("Target / Reference").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
    ]);

    let int_lufs_cell = format!("{:.1} LUFS", lufs.integrated_lufs);
    let int_status = if lufs.integrated_lufs > -12.0 {
        Cell::new("VERY LOUD").fg(Color::Red)
    } else if lufs.integrated_lufs > -14.5 {
        Cell::new("STREAMING OPTIMAL").fg(Color::Green)
    } else {
        Cell::new("DYNAMIC / QUIET").fg(Color::Yellow)
    };
    lufs_table.add_row(vec![
        Cell::new("Integrated Loudness (LUFS)"),
        Cell::new(int_lufs_cell),
        Cell::new("-14.0 LUFS (Spotify / YouTube)"),
        int_status,
    ]);

    lufs_table.add_row(vec![
        Cell::new("Short-Term Max"),
        Cell::new(format!("{:.1} LUFS", lufs.short_term_max_lufs)),
        Cell::new("3.0s sliding window"),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    lufs_table.add_row(vec![
        Cell::new("Momentary Max"),
        Cell::new(format!("{:.1} LUFS", lufs.momentary_max_lufs)),
        Cell::new("400ms window"),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    lufs_table.add_row(vec![
        Cell::new("Loudness Range (LRA)"),
        Cell::new(format!("{:.1} LU", lufs.loudness_range_lra)),
        Cell::new("Dynamics: 4-6 LU (Pop/EDM), >10 (Jazz/Classical)"),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    let spotify_diff = lufs.streaming_compliance.spotify_diff_lu;
    let spotify_note = if spotify_diff.abs() < 1.0 {
        "Match (±1 LU)".to_string()
    } else if spotify_diff > 0.0 {
        format!("Will be attenuated by {:.1} dB", spotify_diff)
    } else {
        format!("{:.1} dB under target", spotify_diff.abs())
    };
    lufs_table.add_row(vec![
        Cell::new("Spotify Target Offset"),
        Cell::new(format!("{:+.1} LU", spotify_diff)),
        Cell::new("-14.0 LUFS"),
        Cell::new(spotify_note).fg(if spotify_diff.abs() <= 1.0 {
            Color::Green
        } else {
            Color::Yellow
        }),
    ]);

    sec += 1;
    println!(
        "{}",
        format!(
            "{}. LOUDNESS & BROADCAST COMPLIANCE (ITU-R BS.1770 / EBU R128)",
            sec
        )
        .bright_cyan()
        .bold()
    );
    println!("{}\n", lufs_table);

    // DYNAMICS, PEAK & CLIPPING TABLE
    let mut dyn_table = Table::new();
    dyn_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    dyn_table.set_header(vec![
        Cell::new("Metric").fg(Color::Cyan),
        Cell::new("Value").fg(Color::Cyan),
        Cell::new("Threshold / Limit").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
    ]);

    // True Peak
    let tp_status = if tp.true_peak_clipping {
        Cell::new("CLIPPING (> 0 dBTP)").fg(Color::Red)
    } else if tp.inter_sample_clipping_risk {
        Cell::new("RISK (> -1.0 dBTP)").fg(Color::Yellow)
    } else {
        Cell::new("SAFE (<= -1.0 dBTP)").fg(Color::Green)
    };
    dyn_table.add_row(vec![
        Cell::new("True Peak (4x Oversampled)"),
        Cell::new(format!("{:.2} dBTP", tp.max_true_peak_dbtp)),
        Cell::new("<= -1.0 dBTP (Streaming Safe)"),
        tp_status,
    ]);

    // Sample Peak
    let sp_status = if time.overall_peak_dbfs >= -0.05 {
        Cell::new("FULL SCALE (0 dBFS)").fg(Color::Yellow)
    } else {
        Cell::new("OK").fg(Color::Green)
    };
    dyn_table.add_row(vec![
        Cell::new("Sample Peak (Digital)"),
        Cell::new(format!("{:.2} dBFS", time.overall_peak_dbfs)),
        Cell::new("< 0.0 dBFS"),
        sp_status,
    ]);

    dyn_table.add_row(vec![
        Cell::new("Overall RMS Level"),
        Cell::new(format!("{:.2} dBFS", time.overall_rms_dbfs)),
        Cell::new("Average Signal Energy"),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    dyn_table.add_row(vec![
        Cell::new("Crest Factor / Dynamic Range"),
        Cell::new(format!("{:.2} dB", time.dynamic_range_db)),
        Cell::new("Peak to RMS difference"),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    let clip_status = if time.total_clipped_samples > 0 {
        Cell::new(format!("{} CLIPS", time.total_clipped_samples)).fg(Color::Red)
    } else {
        Cell::new("ZERO CLIPPING").fg(Color::Green)
    };
    dyn_table.add_row(vec![
        Cell::new("Digital Clipping"),
        Cell::new(format!(
            "{} samples ({} events)",
            time.total_clipped_samples, time.clipping_events
        )),
        Cell::new("0 samples"),
        clip_status,
    ]);

    dyn_table.add_row(vec![
        Cell::new("Silence (< -60 dBFS)"),
        Cell::new(format!(
            "{:.2}s ({:.1}%)",
            time.silence_duration_seconds, time.silence_percentage
        )),
        Cell::new("Gaps & intros"),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    sec += 1;
    println!(
        "{}",
        format!("{}. DYNAMICS, PEAK & DEFECT DETECTION", sec)
            .bright_cyan()
            .bold()
    );
    println!("{}\n", dyn_table);

    // STEREO & PHASE TABLE
    if stereo.is_stereo {
        let mut stereo_table = Table::new();
        stereo_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
        stereo_table.set_header(vec![
            Cell::new("Metric").fg(Color::Cyan),
            Cell::new("Value").fg(Color::Cyan),
            Cell::new("Interpretation").fg(Color::Cyan),
            Cell::new("Status").fg(Color::Cyan),
        ]);

        let phase_status = if stereo.phase_correlation < 0.0 {
            Cell::new("OUT OF PHASE (Mono Cancellation!)").fg(Color::Red)
        } else if stereo.phase_correlation < 0.3 {
            Cell::new("WIDE STEREO").fg(Color::Yellow)
        } else {
            Cell::new("HEALTHY CORRELATION").fg(Color::Green)
        };
        stereo_table.add_row(vec![
            Cell::new("Phase Correlation"),
            Cell::new(format!("{:.2}", stereo.phase_correlation)),
            Cell::new("-1.0 (Out of Phase) to +1.0 (Mono)"),
            phase_status,
        ]);

        stereo_table.add_row(vec![
            Cell::new("Stereo Width"),
            Cell::new(format!("{:.1}%", stereo.stereo_width_percent)),
            Cell::new("Side energy ratio (0%=Mono, 100%=Balanced)"),
            Cell::new("INFO").fg(Color::Cyan),
        ]);

        stereo_table.add_row(vec![
            Cell::new("Mid/Side Ratio"),
            Cell::new(format!("{:.2} dB", stereo.mid_side_ratio_db)),
            Cell::new("Side RMS minus Mid RMS"),
            Cell::new("INFO").fg(Color::Cyan),
        ]);

        let bal_str = if stereo.lr_balance_percent.abs() < 2.0 {
            "Centered (Balanced)".to_string()
        } else if stereo.lr_balance_percent > 0.0 {
            format!("{:.1}% Left Bias", stereo.lr_balance_percent)
        } else {
            format!("{:.1}% Right Bias", stereo.lr_balance_percent.abs())
        };
        stereo_table.add_row(vec![
            Cell::new("L/R Balance"),
            Cell::new(bal_str),
            Cell::new("Pan center"),
            Cell::new("INFO").fg(Color::Cyan),
        ]);

        sec += 1;
        println!(
            "{}",
            format!("{}. STEREO FIELD & MONO COMPATIBILITY", sec)
                .bright_cyan()
                .bold()
        );
        println!("{}\n", stereo_table);
    }

    // FREQUENCY SPECTRUM & TONAL BALANCE
    let spec = &analysis.spectral;
    let mut spec_table = Table::new();
    spec_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    spec_table.set_header(vec![
        Cell::new("Metric").fg(Color::Cyan),
        Cell::new("Value").fg(Color::Cyan),
        Cell::new("Interpretation / Description").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
    ]);

    let brightness_desc = if spec.spectral_centroid_hz < 1800.0 {
        "Warm / Dark Tone"
    } else if spec.spectral_centroid_hz < 3200.0 {
        "Balanced / Natural Tone"
    } else {
        "Bright / Airy Tone"
    };
    spec_table.add_row(vec![
        Cell::new("Spectral Centroid"),
        Cell::new(format!("{:.0} Hz", spec.spectral_centroid_hz)),
        Cell::new("Acoustic brightness (center of gravity)"),
        Cell::new(brightness_desc).fg(Color::Green),
    ]);

    spec_table.add_row(vec![
        Cell::new("Spectral Rolloff (85%)"),
        Cell::new(format!("{:.0} Hz", spec.spectral_rolloff_hz)),
        Cell::new("Frequency below which 85% of power resides"),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    let flatness_desc = if spec.spectral_flatness < 0.15 {
        "Highly Harmonic / Tonal (Musical)"
    } else if spec.spectral_flatness < 0.40 {
        "Mixed Tonal & Percussive"
    } else {
        "Noisy / Heavily Distorted / Sibilant"
    };
    spec_table.add_row(vec![
        Cell::new("Spectral Flatness"),
        Cell::new(format!("{:.3}", spec.spectral_flatness)),
        Cell::new("0.0 = pure sine / harmonic, 1.0 = white noise"),
        Cell::new(flatness_desc).fg(Color::Green),
    ]);

    sec += 1;
    println!(
        "{}",
        format!("{}. FREQUENCY SPECTRUM & TONAL BALANCE", sec)
            .bright_cyan()
            .bold()
    );
    println!("{}\n", spec_table);

    if let Some(spec_chart) = render_terminal_spectrum(analysis, 72, 10, ThemeName::Dracula) {
        println!("{}\n", spec_chart);
    }

    // REGISTER BALANCE (Sub, Bass, Low-Mid, Mid, High-Mid, Air)
    let bal = &spec.balance;
    let mut reg_table = Table::new();
    reg_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    reg_table.set_header(vec![
        Cell::new("Acoustic Register").fg(Color::Cyan),
        Cell::new("Frequency Range").fg(Color::Cyan),
        Cell::new("Relative Level").fg(Color::Cyan),
        Cell::new("Energy Distribution").fg(Color::Cyan),
    ]);

    let registers = [
        ("Sub-Bass", "20 Hz - 60 Hz", bal.sub_bass_db),
        ("Bass", "60 Hz - 250 Hz", bal.bass_db),
        ("Low-Mids", "250 Hz - 500 Hz", bal.low_mids_db),
        ("Mids", "500 Hz - 2 kHz", bal.mids_db),
        ("High-Mids", "2 kHz - 6 kHz", bal.high_mids_db),
        ("Brilliance / Air", "6 kHz - 20 kHz", bal.brilliance_db),
    ];

    for (name, range, db) in registers {
        let bar_len = (((db + 36.0) / 36.0).clamp(0.0, 1.0) * 24.0).round() as usize;
        let bar_str = "█".repeat(bar_len);
        reg_table.add_row(vec![
            Cell::new(name),
            Cell::new(range),
            Cell::new(format!("{:+.1} dB", db)),
            Cell::new(bar_str).fg(Color::Magenta),
        ]);
    }
    sec += 1;
    println!(
        "{}",
        format!("{}. REGISTER ENERGY DISTRIBUTION", sec)
            .bright_cyan()
            .bold()
    );
    println!("{}\n", reg_table);

    // MUSICAL & RHYTHMIC METRICS (MIR)
    let mus = &analysis.musical;
    let mut mus_table = Table::new();
    mus_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    mus_table.set_header(vec![
        Cell::new("Musical Property").fg(Color::Cyan),
        Cell::new("Estimated Value").fg(Color::Cyan),
        Cell::new("Confidence").fg(Color::Cyan),
        Cell::new("Algorithm / Details").fg(Color::Cyan),
    ]);

    let bpm_str = if mus.estimated_bpm > 0.0 {
        format!("{:.0} BPM", mus.estimated_bpm)
    } else {
        "N/A".to_string()
    };
    let bpm_conf_str = format!("{:.0}%", mus.bpm_confidence * 100.0);
    mus_table.add_row(vec![
        Cell::new("Tempo (BPM)"),
        Cell::new(bpm_str).fg(Color::Yellow),
        Cell::new(bpm_conf_str),
        Cell::new("Spectral Flux Onset Novelty + Autocorrelation"),
    ]);

    let key_conf_str = format!("{:.0}%", mus.key_confidence * 100.0);
    mus_table.add_row(vec![
        Cell::new("Musical Key & Scale"),
        Cell::new(&mus.estimated_key).fg(Color::Yellow),
        Cell::new(key_conf_str),
        Cell::new("12-Tone Chroma & Krumhansl-Schmuckler Profiles"),
    ]);

    sec += 1;
    println!(
        "{}",
        format!("{}. MUSICAL & RHYTHMIC METRICS (MIR)", sec)
            .bright_cyan()
            .bold()
    );
    println!("{}\n", mus_table);

    // Chroma pitch class profile visualizer via TermiChart
    if let Some(chroma_chart) = render_terminal_chroma(analysis) {
        println!("{}\n", chroma_chart);
    }

    // VISUAL WAVEFORM GRAPH
    if !analysis.waveform.buckets.is_empty() {
        sec += 1;
        println!(
            "{}",
            format!("{}. WAVEFORM ENVELOPE (TIME DOMAIN)", sec)
                .bright_cyan()
                .bold()
        );
        if let Some(wf_chart) = render_terminal_waveform(analysis, 72, 10, ThemeName::Dracula) {
            println!("{}\n", wf_chart);
        } else {
            let ascii_wave = render_ascii_waveform(&analysis.waveform.buckets, 64);
            println!("┌{}┐", "─".repeat(64));
            println!("│{}│", ascii_wave.bright_magenta());
            println!("└{}┘", "─".repeat(64));
            println!(
                "0:00 {} {:.0}s\n",
                " ".repeat(53),
                meta.duration_seconds.unwrap_or(0.0)
            );
        }
    }

    // AUTOMATED MASTERING & MIXING ADVISOR
    let adv = &analysis.advisor;
    sec += 1;
    let grade_colored = if adv.readiness_score >= 90 {
        format!(
            "Score: {}/100  •  {}",
            adv.readiness_score, adv.readiness_grade
        )
        .bright_green()
        .bold()
    } else if adv.readiness_score >= 75 {
        format!(
            "Score: {}/100  •  {}",
            adv.readiness_score, adv.readiness_grade
        )
        .bright_yellow()
        .bold()
    } else {
        format!(
            "Score: {}/100  •  {}",
            adv.readiness_score, adv.readiness_grade
        )
        .bright_red()
        .bold()
    };

    println!(
        "{}  [{}]",
        format!("{}. AUTOMATED MASTERING & MIXING ADVISOR", sec)
            .bright_cyan()
            .bold(),
        grade_colored
    );

    let mut adv_table = Table::new();
    adv_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(96);
    adv_table.set_header(vec![
        Cell::new("Level").fg(Color::Cyan),
        Cell::new("Area").fg(Color::Cyan),
        Cell::new("Finding & Recommended Action").fg(Color::Cyan),
    ]);

    for item in &adv.items {
        let (badge, col) = match item.severity {
            AdviceSeverity::Critical => ("✖ CRIT", Color::Red),
            AdviceSeverity::Warning => ("⚠ WARN", Color::Yellow),
            AdviceSeverity::Tip => ("ℹ TIP", Color::Cyan),
            AdviceSeverity::Good => ("✔ GOOD", Color::Green),
        };
        adv_table.add_row(vec![
            Cell::new(badge).fg(col),
            Cell::new(&item.category).fg(col),
            Cell::new(format!("{} — {}", item.summary, item.action)),
        ]);
    }

    println!("{}\n", adv_table);
}

/// Dedicated, focused terminal report for Fake Lossless & Lossy Transcode Verification.
pub fn print_transcode_report(analysis: &AudioAnalysis) {
    let meta = &analysis.metadata;
    let qual = &analysis.quality;

    println!("\n{}", "=".repeat(72).bright_cyan());
    println!(
        " {}  {} {}",
        "LOSSLESS AUTHENTICITY & TRANSCODE VERIFIER".bright_white().bold(),
        "•".bright_cyan(),
        meta.file_name.bright_yellow().bold()
    );
    println!("{}\n", "=".repeat(72).bright_cyan());

    // 1. Verdict Hero Box
    let (icon, title, desc, border_col) = match qual.verdict {
        LosslessVerdict::GenuineLossless => (
            "✔",
            "VERIFIED GENUINE LOSSLESS",
            format!(
                "Continuous harmonic frequency spectrum extends up to Nyquist limit ({:.1} kHz).\n\
                 No lossy brickwall lowpass filters detected. Audio displays natural acoustic decay.",
                qual.nyquist_hz / 1000.0
            ),
            Color::Green,
        ),
        LosslessVerdict::SuspectedTranscode128k => (
            "✖",
            "FAKE LOSSLESS DETECTED — 128 kbps MP3 TRANSCODE",
            format!(
                "Sharp brickwall lowpass cutoff cliff detected at ~{:.1} kHz ({:.1} dB drop into noise floor).\n\
                 This file is stored in a lossless container ({}) but originated from ~128k lossy audio.\n\
                 Frequencies above 16.0 kHz were permanently lost during initial lossy compression.",
                qual.detected_cutoff_hz.unwrap_or(16000.0) / 1000.0,
                qual.shelf_drop_db,
                meta.format_name
            ),
            Color::Red,
        ),
        LosslessVerdict::SuspectedTranscode192k => (
            "✖",
            "SUSPECTED TRANSCODE — 192 kbps MP3 CUTOFF",
            format!(
                "Steep lowpass cutoff shelf detected at ~{:.1} kHz ({:.1} dB drop into noise floor).\n\
                 High frequencies drop off unnaturally above ~18.5 kHz, characteristic of 192k lossy encoding.",
                qual.detected_cutoff_hz.unwrap_or(18500.0) / 1000.0,
                qual.shelf_drop_db
            ),
            Color::Red,
        ),
        LosslessVerdict::SuspectedTranscode320k => (
            "⚠",
            "SUSPECTED TRANSCODE — 320 kbps MP3 CUTOFF",
            format!(
                "Lowpass brickwall cutoff shelf detected at ~{:.1} kHz ({:.1} dB drop into noise floor).\n\
                 Standard 320 kbps MP3 encoders hard-filter frequency content at ~20.1 - 20.5 kHz.\n\
                 Audio appears to be upscaled from a 320 kbps MP3 master.",
                qual.detected_cutoff_hz.unwrap_or(20100.0) / 1000.0,
                qual.shelf_drop_db
            ),
            Color::Yellow,
        ),
        LosslessVerdict::NativeLossy => (
            "ℹ",
            "NATIVE LOSSY CONTAINER",
            format!(
                "File format is {} (native lossy compression).\n\
                 Transcode verification is intended for lossless containers (FLAC, WAV, ALAC, AIFF).",
                meta.format_name
            ),
            Color::Cyan,
        ),
        LosslessVerdict::BandwidthLimited => (
            "ℹ",
            "ACOUSTIC BANDWIDTH LIMITED (Natural Roll-off)",
            "High-frequency spectrum rolls off naturally without artificial encoder brickwall filter shelves\n\
             (typical of acoustic instruments, vintage recordings, or warm mastering).".to_string(),
            Color::Cyan,
        ),
    };

    let card_content = format!("  {}  {}\n\n  {}", icon, title, desc.replace('\n', "\n  "));
    let mut hero_table = Table::new();
    hero_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    hero_table.add_row(vec![Cell::new(card_content).fg(border_col)]);
    println!("{}\n", hero_table);

    // 2. File & Stream Summary Table
    let mut file_table = Table::new();
    file_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    file_table.set_header(vec![
        Cell::new("Stream Property").fg(Color::Cyan),
        Cell::new("Value").fg(Color::Cyan),
    ]);

    let size_mb = (meta.file_size_bytes as f64) / (1024.0 * 1024.0);
    let channels_str = match meta.channels {
        1 => "1 (Mono)".to_string(),
        2 => "2 (Stereo)".to_string(),
        c => format!("{} Channels", c),
    };
    let duration_str = meta
        .duration_seconds
        .map(|d| {
            let mins = (d / 60.0).floor() as u64;
            let secs = d % 60.0;
            format!("{}m {:05.2}s ({:.2}s)", mins, secs, d)
        })
        .unwrap_or_else(|| "Unknown".into());

    file_table.add_row(vec!["Container / Format", &meta.format_name]);
    file_table.add_row(vec!["Audio Codec", &meta.codec_name]);
    file_table.add_row(vec!["Channels", &channels_str]);
    file_table.add_row(vec![
        "Sample Rate",
        &format!("{} Hz (Nyquist Limit: {:.1} kHz)", meta.sample_rate, qual.nyquist_hz / 1000.0),
    ]);
    file_table.add_row(vec![
        "Bit Depth",
        &meta.bits_per_sample.map(|b| format!("{}-bit", b)).unwrap_or_else(|| "N/A".into()),
    ]);
    file_table.add_row(vec![
        "Bitrate",
        &meta.bitrate_kbps.map(|b| format!("{:.1} kbps", b)).unwrap_or_else(|| "N/A".into()),
    ]);
    file_table.add_row(vec!["File Size", &format!("{:.2} MB", size_mb)]);
    file_table.add_row(vec!["Duration", &duration_str]);

    println!("{}", "1. FILE & CONTAINER SUMMARY".bright_cyan().bold());
    println!("{}\n", file_table);

    // 3. Lossless Authenticity Metrics Table
    let mut qual_table = Table::new();
    qual_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    qual_table.set_header(vec![
        Cell::new("Authenticity Check").fg(Color::Cyan),
        Cell::new("Measurement / Result").fg(Color::Cyan),
        Cell::new("Confidence").fg(Color::Cyan),
        Cell::new("Verdict / Status").fg(Color::Cyan),
    ]);

    let (status_text, status_color) = match qual.verdict {
        LosslessVerdict::GenuineLossless => ("VERIFIED LOSSLESS", Color::Green),
        LosslessVerdict::SuspectedTranscode320k => ("SUSPECTED 320k", Color::Yellow),
        LosslessVerdict::SuspectedTranscode192k => ("SUSPECTED 192k", Color::Red),
        LosslessVerdict::SuspectedTranscode128k => ("FAKE LOSSLESS (128k)", Color::Red),
        LosslessVerdict::NativeLossy => ("LOSSY CONTAINER", Color::Cyan),
        LosslessVerdict::BandwidthLimited => ("BANDWIDTH LIMITED", Color::DarkCyan),
    };

    let conf_str = format!("{:.0}%", qual.confidence_pct);
    let source_measurement = match qual.verdict {
        LosslessVerdict::GenuineLossless => format!("Continuous to {:.1} kHz Nyquist", qual.nyquist_hz / 1000.0),
        LosslessVerdict::SuspectedTranscode320k => format!("Cutoff shelf at ~{:.1} kHz", qual.detected_cutoff_hz.unwrap_or(20100.0) / 1000.0),
        LosslessVerdict::SuspectedTranscode192k => format!("Cutoff shelf at ~{:.1} kHz", qual.detected_cutoff_hz.unwrap_or(18500.0) / 1000.0),
        LosslessVerdict::SuspectedTranscode128k => format!("Brickwall cutoff at ~{:.1} kHz", qual.detected_cutoff_hz.unwrap_or(16000.0) / 1000.0),
        LosslessVerdict::NativeLossy => format!("Lossy format ({})", meta.format_name),
        LosslessVerdict::BandwidthLimited => "Natural acoustic roll-off".to_string(),
    };
    qual_table.add_row(vec![
        Cell::new("Source Authenticity"),
        Cell::new(source_measurement),
        Cell::new(&conf_str),
        Cell::new(status_text).fg(status_color),
    ]);

    let cutoff_str = match qual.detected_cutoff_hz {
        Some(f) => format!("{:.1} kHz (Brickwall shelf)", f / 1000.0),
        None => format!("None (Full to {:.1} kHz Nyquist)", qual.nyquist_hz / 1000.0),
    };
    let cutoff_status = match qual.verdict {
        LosslessVerdict::GenuineLossless => Cell::new("FULL BANDWIDTH").fg(Color::Green),
        LosslessVerdict::BandwidthLimited => Cell::new("NATURAL ROLLOFF").fg(Color::Green),
        _ => Cell::new("CUTOFF DETECTED").fg(status_color),
    };
    qual_table.add_row(vec![
        Cell::new("Spectral Cutoff Shelf"),
        Cell::new(cutoff_str),
        Cell::new(&conf_str),
        cutoff_status,
    ]);

    qual_table.add_row(vec![
        Cell::new("Nyquist Utilization"),
        Cell::new(format!(
            "{:.1}% ({:.1} kHz of {:.1} kHz)",
            qual.bandwidth_utilized_pct,
            (qual.bandwidth_utilized_pct / 100.0 * qual.nyquist_hz) / 1000.0,
            qual.nyquist_hz / 1000.0
        )),
        Cell::new("N/A"),
        Cell::new(if qual.bandwidth_utilized_pct >= 95.0 { "FULL" } else { "TRUNCATED" })
            .fg(if qual.bandwidth_utilized_pct >= 95.0 { Color::Green } else { Color::Yellow }),
    ]);

    qual_table.add_row(vec![
        Cell::new("Shelf Cliff Drop"),
        Cell::new(format!("{:.1} dB drop into noise floor", qual.shelf_drop_db)),
        Cell::new(&conf_str),
        if qual.shelf_drop_db >= 16.0 {
            Cell::new("STEEP CLIFF (>= 16 dB)").fg(Color::Red)
        } else {
            Cell::new("GRADUAL (< 16 dB)").fg(Color::Green)
        },
    ]);

    println!("{}", "2. LOSSLESS AUTHENTICITY METRICS".bright_cyan().bold());
    println!("{}\n", qual_table);

    // 4. Frequency Spectrum (31-Band ISO)
    println!("{}", "3. FREQUENCY SPECTRUM (20 Hz – 20 kHz)".bright_cyan().bold());
    if let Some(spec_chart) = render_terminal_spectrum(analysis, 72, 10, ThemeName::Dracula) {
        println!("{}\n", spec_chart);
    }

    // 5. Common Encoder Reference Guide Table
    let mut ref_table = Table::new();
    ref_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    ref_table.set_header(vec![
        Cell::new("Format / Bitrate").fg(Color::Cyan),
        Cell::new("Cutoff Shelf").fg(Color::Cyan),
        Cell::new("Spectral Characteristics").fg(Color::Cyan),
    ]);
    ref_table.add_row(vec![
        Cell::new("MP3 128 kbps"),
        Cell::new("~16.0 kHz"),
        Cell::new("Hard brickwall; near-silence above 16 kHz"),
    ]);
    ref_table.add_row(vec![
        Cell::new("MP3 192 kbps"),
        Cell::new("~18.5 - 19.0 kHz"),
        Cell::new("Steep shelf; sharp cutoff"),
    ]);
    ref_table.add_row(vec![
        Cell::new("MP3 320 kbps"),
        Cell::new("~20.1 - 20.5 kHz"),
        Cell::new("Hard brickwall cutoff below 20.5 kHz"),
    ]);
    ref_table.add_row(vec![
        Cell::new("AAC 256 kbps"),
        Cell::new("~19.5 - 20.0 kHz"),
        Cell::new("Soft psychoacoustic roll-off"),
    ]);
    ref_table.add_row(vec![
        Cell::new("Genuine Lossless"),
        Cell::new("Full Nyquist (22k+)"),
        Cell::new("Continuous harmonic acoustic decay"),
    ]);

    println!("{}", "4. COMMON ENCODER CUTOFF REFERENCE GUIDE".bright_cyan().bold());
    println!("{}\n", ref_table);
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else if max_chars <= 1 {
        "…".to_string()
    } else {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

fn short_format_label(format_name: &str, codec_name: &str) -> String {
    let f = format_name.to_lowercase();
    let c = codec_name.to_lowercase();
    if f.contains("flac") || c.contains("flac") {
        "FLAC".into()
    } else if f.contains("wav") || f.contains("riff") {
        "WAV".into()
    } else if f.contains("mp3") || c.contains("mp3") {
        "MP3".into()
    } else if c.contains("alac") {
        "ALAC".into()
    } else if f.contains("aac") || c.contains("aac") {
        "AAC".into()
    } else if c.contains("opus") {
        "OPUS".into()
    } else if f.contains("ogg") || c.contains("vorbis") {
        "OGG".into()
    } else if f.contains("aiff") {
        "AIFF".into()
    } else if f.contains("au") || f.contains("snd") {
        "AU".into()
    } else if f.contains("mp4") || f.contains("m4a") {
        "M4A".into()
    } else {
        truncate_label(format_name, 6).to_uppercase()
    }
}

fn format_duration_hms(total_secs: f64) -> String {
    let secs_total = total_secs.round().max(0.0) as u64;
    let hours = secs_total / 3600;
    let mins = (secs_total % 3600) / 60;
    let secs = secs_total % 60;
    if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, mins, secs)
    } else {
        format!("{}m {:02}s", mins, secs)
    }
}

/// Prints a comprehensive Batch Folder / Album Mastering & Authenticity report.
pub fn print_batch_report(report: &BatchScanReport, transcode_only: bool) {
    let sum = &report.summary;
    let dir_name = std::path::Path::new(&report.directory)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| report.directory.clone());

    let banner_title = if transcode_only {
        "BATCH LOSSLESS & TRANSCODE AUDIT"
    } else {
        "BATCH FOLDER / ALBUM MASTERING SCAN"
    };

    println!("\n{}", "=".repeat(76).bright_cyan());
    println!(
        " {}  {} {}",
        banner_title.bright_white().bold(),
        "•".bright_cyan(),
        dir_name.bright_yellow().bold()
    );
    println!("{}\n", "=".repeat(76).bright_cyan());

    // 1. ALBUM / FOLDER SUMMARY TABLE
    let mut sum_table = Table::new();
    sum_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
    sum_table.set_header(vec![
        Cell::new("Album / Folder Metric").fg(Color::Cyan),
        Cell::new("Value / Measurement").fg(Color::Cyan),
        Cell::new("Status / Notes").fg(Color::Cyan),
    ]);

    let mode_label = if report.recursive {
        "Recursive"
    } else {
        "Top-level only"
    };
    sum_table.add_row(vec![
        Cell::new("Target Directory"),
        Cell::new(truncate_label(&report.directory, 42)),
        Cell::new(mode_label).fg(Color::Cyan),
    ]);

    let track_status = if sum.failed == 0 {
        Cell::new("ALL DECODED").fg(Color::Green)
    } else {
        Cell::new(format!("{} FAILED", sum.failed)).fg(Color::Red)
    };
    sum_table.add_row(vec![
        Cell::new("Tracks Scanned"),
        Cell::new(format!(
            "{} total ({} ok, {} failed)",
            sum.total_files_found, sum.succeeded, sum.failed
        )),
        track_status,
    ]);

    let total_mb = sum.total_size_bytes as f64 / (1024.0 * 1024.0);
    sum_table.add_row(vec![
        Cell::new("Total Runtime & Size"),
        Cell::new(format!(
            "{}  ({:.2} MB)",
            format_duration_hms(sum.total_duration_seconds),
            total_mb
        )),
        Cell::new("INFO").fg(Color::Cyan),
    ]);

    let rates_str = if sum.unique_sample_rates.is_empty() {
        "N/A".to_string()
    } else {
        sum.unique_sample_rates
            .iter()
            .map(|r| format!("{:.1}k", *r as f64 / 1000.0))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let bits_str = if sum.unique_bit_depths.is_empty() {
        "N/A".to_string()
    } else {
        sum.unique_bit_depths
            .iter()
            .map(|b| format!("{}-bit", b))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let fmt_status = if sum.unique_sample_rates.len() <= 1 {
        Cell::new("CONSISTENT").fg(Color::Green)
    } else {
        Cell::new("MIXED RATES").fg(Color::Yellow)
    };
    sum_table.add_row(vec![
        Cell::new("Sample Rates & Bit Depth"),
        Cell::new(format!("{} Hz  |  {}", rates_str, bits_str)),
        fmt_status,
    ]);

    if !transcode_only && sum.succeeded > 0 {
        let spread_status = if sum.lufs_spread_lu <= 2.5 {
            Cell::new("CONSISTENT ALBUM").fg(Color::Green)
        } else if sum.lufs_spread_lu <= 4.5 {
            Cell::new("MODERATE SPREAD").fg(Color::Yellow)
        } else {
            Cell::new("UNEVEN LOUDNESS").fg(Color::Red)
        };
        sum_table.add_row(vec![
            Cell::new("Album Loudness (LUFS)"),
            Cell::new(format!(
                "Avg {:.1}  |  Range {:.1}..{:.1} ({:.1} LU)",
                sum.average_lufs, sum.min_lufs, sum.max_lufs, sum.lufs_spread_lu
            )),
            spread_status,
        ]);

        let peak_status = if sum.clipping_tracks > 0 {
            Cell::new(format!("{} CLIPPING", sum.clipping_tracks)).fg(Color::Red)
        } else if sum.true_peak_risk_tracks > 0 {
            Cell::new(format!("{} TP RISK", sum.true_peak_risk_tracks)).fg(Color::Yellow)
        } else {
            Cell::new("ALL SAFE").fg(Color::Green)
        };
        sum_table.add_row(vec![
            Cell::new("Worst True Peak"),
            Cell::new(format!(
                "{:.2} dBTP  ({})",
                sum.worst_true_peak_dbtp,
                sum.worst_true_peak_track
                    .as_deref()
                    .map(|s| truncate_label(s, 22))
                    .unwrap_or_else(|| "N/A".into())
            )),
            peak_status,
        ]);

        sum_table.add_row(vec![
            Cell::new("Average Dynamics"),
            Cell::new(format!(
                "LRA {:.1} LU  |  Crest {:.1} dB",
                sum.average_lra_lu, sum.average_dynamic_range_db
            )),
            Cell::new("INFO").fg(Color::Cyan),
        ]);
    }

    // Lossless Authenticity Summary (always shown in transcode_only mode, or whenever FLAC/lossless tracks exist)
    let fake_count = sum.suspected_128k_tracks + sum.suspected_192k_tracks;
    if transcode_only || sum.flac_tracks > 0 || fake_count > 0 || sum.suspected_320k_tracks > 0 {
        let mut parts = Vec::new();
        if sum.verified_lossless_tracks > 0 {
            parts.push(format!("{} Genuine", sum.verified_lossless_tracks));
        }
        if sum.suspected_320k_tracks > 0 {
            parts.push(format!("{} 320k?", sum.suspected_320k_tracks));
        }
        if fake_count > 0 {
            parts.push(format!("{} Fake(128/192k)", fake_count));
        }
        if sum.bandwidth_limited_tracks > 0 {
            parts.push(format!("{} BW-Lim", sum.bandwidth_limited_tracks));
        }
        if sum.native_lossy_tracks > 0 {
            parts.push(format!("{} Lossy", sum.native_lossy_tracks));
        }
        let auth_val = if parts.is_empty() {
            "N/A".to_string()
        } else {
            parts.join(" | ")
        };
        let auth_status = if fake_count > 0 {
            Cell::new("FAKE LOSSLESS FOUND").fg(Color::Red)
        } else if sum.suspected_320k_tracks > 0 {
            Cell::new("320k SUSPECTED").fg(Color::Yellow)
        } else if sum.verified_lossless_tracks > 0 {
            Cell::new("VERIFIED LOSSLESS").fg(Color::Green)
        } else {
            Cell::new("INFO").fg(Color::Cyan)
        };
        sum_table.add_row(vec![
            Cell::new("Lossless Authenticity"),
            Cell::new(auth_val),
            auth_status,
        ]);
    }

    println!("{}", "1. ALBUM / FOLDER SUMMARY".bright_cyan().bold());
    println!("{}\n", sum_table);

    // 2. TRACK-BY-TRACK MATRIX
    let mut track_table = Table::new();
    track_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);

    if transcode_only {
        track_table.set_header(vec![
            Cell::new("#").fg(Color::Cyan),
            Cell::new("Track").fg(Color::Cyan),
            Cell::new("Fmt").fg(Color::Cyan),
            Cell::new("Rate/Bit").fg(Color::Cyan),
            Cell::new("Cutoff").fg(Color::Cyan),
            Cell::new("Drop").fg(Color::Cyan),
            Cell::new("Conf").fg(Color::Cyan),
            Cell::new("Verdict").fg(Color::Cyan),
        ]);

        for (i, t) in report.tracks.iter().enumerate() {
            let idx_str = format!("{:02}", i + 1);
            let name_str = truncate_label(&t.file_name, 26);

            let Some(a) = &t.analysis else {
                track_table.add_row(vec![
                    Cell::new(idx_str),
                    Cell::new(name_str),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("ERROR").fg(Color::Red),
                ]);
                continue;
            };

            let fmt_str = short_format_label(&a.metadata.format_name, &a.metadata.codec_name);
            let rate_bit = match a.metadata.bits_per_sample {
                Some(b) => format!("{:.1}k/{}b", a.metadata.sample_rate as f64 / 1000.0, b),
                None => format!("{:.1}k", a.metadata.sample_rate as f64 / 1000.0),
            };
            let cutoff_str = match a.quality.detected_cutoff_hz {
                Some(f) => format!("{:.1}kHz", f / 1000.0),
                None => "Full".to_string(),
            };
            let drop_str = format!("{:.1}dB", a.quality.shelf_drop_db);
            let conf_str = format!("{:.0}%", a.quality.confidence_pct);

            let (verdict_str, verdict_col) = match a.quality.verdict {
                LosslessVerdict::GenuineLossless => ("✔ LOSSLESS", Color::Green),
                LosslessVerdict::SuspectedTranscode320k => ("⚠ 320k?", Color::Yellow),
                LosslessVerdict::SuspectedTranscode192k => ("✖ 192k", Color::Red),
                LosslessVerdict::SuspectedTranscode128k => ("✖ FAKE 128k", Color::Red),
                LosslessVerdict::BandwidthLimited => ("ℹ BW-LIM", Color::DarkCyan),
                LosslessVerdict::NativeLossy => ("LOSSY", Color::Cyan),
            };

            track_table.add_row(vec![
                Cell::new(idx_str),
                Cell::new(name_str),
                Cell::new(fmt_str),
                Cell::new(rate_bit),
                Cell::new(cutoff_str),
                Cell::new(drop_str),
                Cell::new(conf_str),
                Cell::new(verdict_str).fg(verdict_col),
            ]);
        }

        println!(
            "{}",
            "2. TRACK-BY-TRACK LOSSLESS AUTHENTICITY MATRIX"
                .bright_cyan()
                .bold()
        );
        println!("{}\n", track_table);
    } else {
        track_table.set_header(vec![
            Cell::new("#").fg(Color::Cyan),
            Cell::new("Track").fg(Color::Cyan),
            Cell::new("Fmt").fg(Color::Cyan),
            Cell::new("LUFS").fg(Color::Cyan),
            Cell::new("Peak").fg(Color::Cyan),
            Cell::new("DR").fg(Color::Cyan),
            Cell::new("Key / BPM").fg(Color::Cyan),
            Cell::new("Status").fg(Color::Cyan),
        ]);

        for (i, t) in report.tracks.iter().enumerate() {
            let idx_str = format!("{:02}", i + 1);
            let name_str = truncate_label(&t.file_name, 26);

            let Some(a) = &t.analysis else {
                track_table.add_row(vec![
                    Cell::new(idx_str),
                    Cell::new(name_str),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("-"),
                    Cell::new("✖ ERROR").fg(Color::Red),
                ]);
                continue;
            };

            let fmt_str = short_format_label(&a.metadata.format_name, &a.metadata.codec_name);
            let lufs_val = a.lufs.integrated_lufs;
            let lufs_str = format!("{:.1}", lufs_val);
            let lufs_col = if lufs_val > -12.0 {
                Color::Red
            } else if lufs_val > -14.5 {
                Color::Green
            } else {
                Color::Yellow
            };

            let tp_val = a.true_peak.max_true_peak_dbtp;
            let tp_str = format!("{:.1}", tp_val);
            let tp_col = if a.true_peak.true_peak_clipping || a.time_domain.total_clipped_samples > 0
            {
                Color::Red
            } else if a.true_peak.inter_sample_clipping_risk {
                Color::Yellow
            } else {
                Color::Green
            };

            let dr_str = format!("{:.1}", a.time_domain.dynamic_range_db);

            let key_short = a
                .musical
                .estimated_key
                .replace(" Major", " Maj")
                .replace(" Minor", " Min");
            let key_bpm = if a.musical.estimated_bpm > 0.0 {
                format!("{} / {:.0}", key_short, a.musical.estimated_bpm)
            } else {
                key_short
            };

            let is_clip =
                a.true_peak.true_peak_clipping || a.time_domain.total_clipped_samples > 0;
            let is_risk = a.true_peak.inter_sample_clipping_risk;

            let (status_str, status_col) = if a.metadata.is_flac() {
                match a.quality.verdict {
                    LosslessVerdict::SuspectedTranscode128k => ("✖ FAKE 128k", Color::Red),
                    LosslessVerdict::SuspectedTranscode192k => ("✖ 192k?", Color::Red),
                    LosslessVerdict::SuspectedTranscode320k => ("⚠ 320k?", Color::Yellow),
                    LosslessVerdict::GenuineLossless => {
                        if is_clip {
                            ("✖ CLIP | OK", Color::Red)
                        } else if is_risk {
                            ("⚠ TP | LOSSLESS", Color::Yellow)
                        } else {
                            ("✔ LOSSLESS", Color::Green)
                        }
                    }
                    LosslessVerdict::BandwidthLimited => {
                        if is_clip {
                            ("✖ CLIP | BW-LIM", Color::Red)
                        } else {
                            ("ℹ BW-LIM", Color::DarkCyan)
                        }
                    }
                    LosslessVerdict::NativeLossy => ("LOSSY", Color::Cyan),
                }
            } else if is_clip {
                ("✖ CLIPPING", Color::Red)
            } else if a.stereo.is_stereo && a.stereo.phase_correlation < 0.0 {
                ("✖ PHASE", Color::Red)
            } else if is_risk {
                ("⚠ TP RISK", Color::Yellow)
            } else {
                ("✔ OK", Color::Green)
            };

            track_table.add_row(vec![
                Cell::new(idx_str),
                Cell::new(name_str),
                Cell::new(fmt_str),
                Cell::new(lufs_str).fg(lufs_col),
                Cell::new(tp_str).fg(tp_col),
                Cell::new(dr_str),
                Cell::new(truncate_label(&key_bpm, 14)),
                Cell::new(status_str).fg(status_col),
            ]);
        }

        println!(
            "{}",
            "2. TRACK-BY-TRACK MASTERING & AUTHENTICITY MATRIX"
                .bright_cyan()
                .bold()
        );
        println!("{}\n", track_table);
    }

    // 3. FLAGGED ISSUES & MASTERING ALERTS
    let visible_alerts: Vec<_> = sum
        .alerts
        .iter()
        .filter(|al| {
            if transcode_only {
                al.category.contains("FAKE")
                    || al.category.contains("TRANSCODE")
                    || al.category.contains("DECODE")
            } else {
                true
            }
        })
        .collect();

    println!(
        "{}",
        "3. FLAGGED ISSUES & MASTERING ALERTS".bright_cyan().bold()
    );
    if visible_alerts.is_empty() {
        let mut ok_box = Table::new();
        ok_box.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
        ok_box.add_row(vec![
            Cell::new("  ✔  All scanned tracks passed mastering & authenticity checks with no alerts.")
                .fg(Color::Green),
        ]);
        println!("{}\n", ok_box);
    } else {
        let mut alert_table = Table::new();
        alert_table.load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS);
        alert_table.set_header(vec![
            Cell::new("Level").fg(Color::Cyan),
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Track").fg(Color::Cyan),
            Cell::new("Details").fg(Color::Cyan),
        ]);

        for al in visible_alerts {
            let (lvl, col) = match al.severity {
                AlertSeverity::Error => ("✖ ERR", Color::Red),
                AlertSeverity::Warning => ("⚠ WARN", Color::Yellow),
                AlertSeverity::Info => ("ℹ INFO", Color::Cyan),
            };
            alert_table.add_row(vec![
                Cell::new(lvl).fg(col),
                Cell::new(&al.category).fg(col),
                Cell::new(truncate_label(&al.track, 24)),
                Cell::new(&al.message),
            ]);
        }
        println!("{}\n", alert_table);
    }
}
