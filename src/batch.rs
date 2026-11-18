use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::AnalyzerError;
use crate::spectral::LosslessVerdict;
use crate::{analyze_file_simple, AnalysisOptions, AudioAnalysis};

/// Supported audio file extensions for batch folder / album discovery.
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "wav", "mp3", "m4a", "aac", "ogg", "opus", "aiff", "aif", "alac", "mp4", "au", "snd",
];

#[derive(Debug, Clone)]
pub struct BatchScanOptions {
    pub recursive: bool,
    pub analysis_options: AnalysisOptions,
    pub max_threads: Option<usize>,
}

impl Default for BatchScanOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            analysis_options: AnalysisOptions {
                waveform_buckets_per_second: 10,
                ..Default::default()
            },
            max_threads: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackScanResult {
    pub relative_path: String,
    pub file_name: String,
    pub analysis: Option<AudioAnalysis>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAlert {
    pub severity: AlertSeverity,
    pub track: String,
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumSummary {
    pub total_files_found: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub total_duration_seconds: f64,
    pub total_size_bytes: u64,

    // Loudness & Album Consistency
    pub average_lufs: f64,
    pub min_lufs: f64,
    pub max_lufs: f64,
    pub lufs_spread_lu: f64,
    pub loudest_track: Option<String>,
    pub quietest_track: Option<String>,

    // Dynamics & Peak Health
    pub worst_true_peak_dbtp: f64,
    pub worst_true_peak_track: Option<String>,
    pub average_lra_lu: f64,
    pub average_dynamic_range_db: f64,
    pub clipping_tracks: usize,
    pub true_peak_risk_tracks: usize,
    pub phase_issue_tracks: usize,

    // Format Consistency
    pub unique_formats: Vec<String>,
    pub unique_sample_rates: Vec<u32>,
    pub unique_bit_depths: Vec<u32>,

    // Lossless Authenticity Audit
    pub flac_tracks: usize,
    pub verified_lossless_tracks: usize,
    pub suspected_320k_tracks: usize,
    pub suspected_192k_tracks: usize,
    pub suspected_128k_tracks: usize,
    pub bandwidth_limited_tracks: usize,
    pub native_lossy_tracks: usize,

    // Flagged Mastering & Authenticity Alerts
    pub alerts: Vec<BatchAlert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchScanReport {
    pub directory: String,
    pub recursive: bool,
    pub summary: AlbumSummary,
    pub tracks: Vec<TrackScanResult>,
}

/// Returns true if the given path has a supported audio file extension.
pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            SUPPORTED_AUDIO_EXTENSIONS.iter().any(|&s| s == lower)
        })
        .unwrap_or(false)
}

/// Discover all supported audio files in `dir` (recursively if requested), sorted cleanly by path.
pub fn discover_audio_files<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
) -> Result<Vec<PathBuf>, AnalyzerError> {
    let root = dir.as_ref();
    if !root.exists() {
        return Err(AnalyzerError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Directory not found: {}", root.display()),
        )));
    }
    if !root.is_dir() {
        return Err(AnalyzerError::InvalidData(format!(
            "Path is not a directory: {}",
            root.display()
        )));
    }

    let mut files = Vec::new();
    collect_files(root, recursive, &mut files)?;

    files.sort_by(|a, b| {
        a.to_string_lossy()
            .to_lowercase()
            .cmp(&b.to_string_lossy().to_lowercase())
    });

    Ok(files)
}

fn collect_files(
    dir: &Path,
    recursive: bool,
    out: &mut Vec<PathBuf>,
) -> Result<(), AnalyzerError> {
    let entries = fs::read_dir(dir)?;
    for entry_res in entries {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                let _ = collect_files(&path, recursive, out);
            }
        } else if path.is_file() && is_supported_audio_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Scan an entire folder / album without a progress callback.
pub fn scan_directory_simple<P: AsRef<Path>>(
    dir: P,
    options: &BatchScanOptions,
) -> Result<BatchScanReport, AnalyzerError> {
    scan_directory(dir, options, None::<fn(usize, usize, &Path)>)
}

/// Scan an entire folder / album in parallel across CPU cores, aggregating album-level metrics.
pub fn scan_directory<P, F>(
    dir: P,
    options: &BatchScanOptions,
    progress_callback: Option<F>,
) -> Result<BatchScanReport, AnalyzerError>
where
    P: AsRef<Path>,
    F: Fn(usize, usize, &Path) + Sync,
{
    let root = dir.as_ref();
    let files = discover_audio_files(root, options.recursive)?;
    let total = files.len();

    if total == 0 {
        return Err(AnalyzerError::InvalidData(format!(
            "No supported audio files found in '{}'",
            root.display()
        )));
    }

    let default_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .min(8);
    let num_threads = options
        .max_threads
        .unwrap_or(default_threads)
        .max(1)
        .min(total);

    let next_idx = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<TrackScanResult>>> = Mutex::new(vec![None; total]);
    let cb_ref = progress_callback.as_ref();

    std::thread::scope(|s| {
        for _ in 0..num_threads {
            s.spawn(|| {
                loop {
                    let idx = next_idx.fetch_add(1, Ordering::Relaxed);
                    if idx >= total {
                        break;
                    }
                    let path = &files[idx];
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();
                    let file_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| rel.clone());

                    let outcome = match analyze_file_simple(path, &options.analysis_options) {
                        Ok(analysis) => TrackScanResult {
                            relative_path: rel,
                            file_name,
                            analysis: Some(analysis),
                            error: None,
                        },
                        Err(e) => TrackScanResult {
                            relative_path: rel,
                            file_name,
                            analysis: None,
                            error: Some(e.to_string()),
                        },
                    };

                    {
                        let mut guard = results.lock().unwrap();
                        guard[idx] = Some(outcome);
                    }

                    let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Some(cb) = cb_ref {
                        cb(done, total, path);
                    }
                }
            });
        }
    });

    let tracks: Vec<TrackScanResult> = results
        .into_inner()
        .unwrap()
        .into_iter()
        .flatten()
        .collect();

    let summary = compute_album_summary(&tracks);

    Ok(BatchScanReport {
        directory: root.to_string_lossy().to_string(),
        recursive: options.recursive,
        summary,
        tracks,
    })
}

fn compute_album_summary(tracks: &[TrackScanResult]) -> AlbumSummary {
    let total_files_found = tracks.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut total_duration_seconds = 0.0f64;
    let mut total_size_bytes = 0u64;

    let mut lufs_sum = 0.0f64;
    let mut lufs_count = 0usize;
    let mut min_lufs = f64::INFINITY;
    let mut max_lufs = f64::NEG_INFINITY;
    let mut loudest_track: Option<String> = None;
    let mut quietest_track: Option<String> = None;

    let mut worst_true_peak_dbtp = f64::NEG_INFINITY;
    let mut worst_true_peak_track: Option<String> = None;
    let mut lra_sum = 0.0f64;
    let mut dr_sum = 0.0f64;

    let mut clipping_tracks = 0usize;
    let mut true_peak_risk_tracks = 0usize;
    let mut phase_issue_tracks = 0usize;

    let mut formats_set = BTreeSet::new();
    let mut rates_set = BTreeSet::new();
    let mut bits_set = BTreeSet::new();

    let mut flac_tracks = 0usize;
    let mut verified_lossless_tracks = 0usize;
    let mut suspected_320k_tracks = 0usize;
    let mut suspected_192k_tracks = 0usize;
    let mut suspected_128k_tracks = 0usize;
    let mut bandwidth_limited_tracks = 0usize;
    let mut native_lossy_tracks = 0usize;

    let mut alerts = Vec::new();

    for track in tracks {
        let Some(a) = &track.analysis else {
            failed += 1;
            alerts.push(BatchAlert {
                severity: AlertSeverity::Error,
                track: track.file_name.clone(),
                category: "DECODE ERROR".into(),
                message: track
                    .error
                    .clone()
                    .unwrap_or_else(|| "Unknown decode failure".into()),
            });
            continue;
        };

        succeeded += 1;
        total_duration_seconds += a.metadata.duration_seconds.unwrap_or(0.0);
        total_size_bytes += a.metadata.file_size_bytes;

        formats_set.insert(a.metadata.format_name.clone());
        rates_set.insert(a.metadata.sample_rate);
        if let Some(bits) = a.metadata.bits_per_sample {
            bits_set.insert(bits);
        }

        // Loudness stats (ignore pure silence tracks at <= -70 LUFS for album average)
        let lufs = a.lufs.integrated_lufs as f64;
        if lufs.is_finite() && lufs > -70.0 {
            lufs_sum += lufs;
            lufs_count += 1;
            if lufs > max_lufs {
                max_lufs = lufs;
                loudest_track = Some(track.file_name.clone());
            }
            if lufs < min_lufs {
                min_lufs = lufs;
                quietest_track = Some(track.file_name.clone());
            }
        }

        if a.lufs.loudness_range_lra.is_finite() {
            lra_sum += a.lufs.loudness_range_lra as f64;
        }
        if a.time_domain.dynamic_range_db.is_finite() {
            dr_sum += a.time_domain.dynamic_range_db as f64;
        }

        // True Peak & Clipping
        let tp = a.true_peak.max_true_peak_dbtp as f64;
        if tp.is_finite() && tp > worst_true_peak_dbtp {
            worst_true_peak_dbtp = tp;
            worst_true_peak_track = Some(track.file_name.clone());
        }

        if a.true_peak.true_peak_clipping || a.time_domain.total_clipped_samples > 0 {
            clipping_tracks += 1;
            alerts.push(BatchAlert {
                severity: AlertSeverity::Error,
                track: track.file_name.clone(),
                category: "CLIPPING".into(),
                message: format!(
                    "True Peak {:.2} dBTP, {} clipped sample(s)",
                    tp, a.time_domain.total_clipped_samples
                ),
            });
        } else if a.true_peak.inter_sample_clipping_risk {
            true_peak_risk_tracks += 1;
            alerts.push(BatchAlert {
                severity: AlertSeverity::Warning,
                track: track.file_name.clone(),
                category: "TRUE PEAK RISK".into(),
                message: format!("True Peak {:.2} dBTP exceeds -1.0 dBTP streaming limit", tp),
            });
        }

        // Phase cancellation
        if a.stereo.is_stereo && a.stereo.phase_correlation < 0.0 {
            phase_issue_tracks += 1;
            alerts.push(BatchAlert {
                severity: AlertSeverity::Error,
                track: track.file_name.clone(),
                category: "PHASE ISSUE".into(),
                message: format!(
                    "Negative phase correlation ({:.2}) — mono cancellation risk",
                    a.stereo.phase_correlation
                ),
            });
        }

        // Lossless Authenticity Audit
        if a.metadata.is_flac() {
            flac_tracks += 1;
        }
        match a.quality.verdict {
            LosslessVerdict::GenuineLossless => verified_lossless_tracks += 1,
            LosslessVerdict::SuspectedTranscode320k => {
                suspected_320k_tracks += 1;
                alerts.push(BatchAlert {
                    severity: AlertSeverity::Warning,
                    track: track.file_name.clone(),
                    category: "TRANSCODE (320k)".into(),
                    message: format!(
                        "Suspected 320k MP3 cutoff shelf at ~{:.1} kHz ({:.0}% conf.)",
                        a.quality.detected_cutoff_hz.unwrap_or(20100.0) / 1000.0,
                        a.quality.confidence_pct
                    ),
                });
            }
            LosslessVerdict::SuspectedTranscode192k => {
                suspected_192k_tracks += 1;
                alerts.push(BatchAlert {
                    severity: AlertSeverity::Error,
                    track: track.file_name.clone(),
                    category: "FAKE LOSSLESS".into(),
                    message: format!(
                        "Suspected 192k MP3 transcode shelf at ~{:.1} kHz ({:.0}% conf.)",
                        a.quality.detected_cutoff_hz.unwrap_or(18500.0) / 1000.0,
                        a.quality.confidence_pct
                    ),
                });
            }
            LosslessVerdict::SuspectedTranscode128k => {
                suspected_128k_tracks += 1;
                alerts.push(BatchAlert {
                    severity: AlertSeverity::Error,
                    track: track.file_name.clone(),
                    category: "FAKE LOSSLESS".into(),
                    message: format!(
                        "Fake lossless (128k MP3 brickwall at ~{:.1} kHz, {:.0}% conf.)",
                        a.quality.detected_cutoff_hz.unwrap_or(16000.0) / 1000.0,
                        a.quality.confidence_pct
                    ),
                });
            }
            LosslessVerdict::BandwidthLimited => bandwidth_limited_tracks += 1,
            LosslessVerdict::NativeLossy => native_lossy_tracks += 1,
        }
    }

    let average_lufs = if lufs_count > 0 {
        lufs_sum / lufs_count as f64
    } else {
        -70.0
    };
    if !min_lufs.is_finite() {
        min_lufs = average_lufs;
    }
    if !max_lufs.is_finite() {
        max_lufs = average_lufs;
    }
    let lufs_spread_lu = (max_lufs - min_lufs).max(0.0);

    if !worst_true_peak_dbtp.is_finite() {
        worst_true_peak_dbtp = -96.0;
    }

    let average_lra_lu = if succeeded > 0 {
        lra_sum / succeeded as f64
    } else {
        0.0
    };
    let average_dynamic_range_db = if succeeded > 0 {
        dr_sum / succeeded as f64
    } else {
        0.0
    };

    // Flag album loudness outliers (> 3.5 LU away from album average)
    if lufs_count >= 2 {
        for track in tracks {
            if let Some(a) = &track.analysis {
                let lufs = a.lufs.integrated_lufs as f64;
                if lufs.is_finite() && lufs > -70.0 {
                    let delta = lufs - average_lufs;
                    if delta.abs() > 3.5 {
                        alerts.push(BatchAlert {
                            severity: AlertSeverity::Warning,
                            track: track.file_name.clone(),
                            category: "LOUDNESS OUTLIER".into(),
                            message: format!(
                                "{:.1} LUFS ({:+.1} LU from album avg {:.1} LUFS)",
                                lufs, delta, average_lufs
                            ),
                        });
                    }
                }
            }
        }
    }

    let unique_formats: Vec<String> = formats_set.into_iter().collect();
    let unique_sample_rates: Vec<u32> = rates_set.into_iter().collect();
    let unique_bit_depths: Vec<u32> = bits_set.into_iter().collect();

    if unique_sample_rates.len() > 1 {
        let rates_str = unique_sample_rates
            .iter()
            .map(|r| format!("{} Hz", r))
            .collect::<Vec<_>>()
            .join(", ");
        alerts.push(BatchAlert {
            severity: AlertSeverity::Info,
            track: "(Album)".into(),
            category: "MIXED SAMPLE RATES".into(),
            message: format!("Multiple sample rates detected: {}", rates_str),
        });
    }

    // Sort alerts: Error first, then Warning, then Info
    alerts.sort_by_key(|al| match al.severity {
        AlertSeverity::Error => 0,
        AlertSeverity::Warning => 1,
        AlertSeverity::Info => 2,
    });

    AlbumSummary {
        total_files_found,
        succeeded,
        failed,
        total_duration_seconds,
        total_size_bytes,
        average_lufs,
        min_lufs,
        max_lufs,
        lufs_spread_lu,
        loudest_track,
        quietest_track,
        worst_true_peak_dbtp,
        worst_true_peak_track,
        average_lra_lu,
        average_dynamic_range_db,
        clipping_tracks,
        true_peak_risk_tracks,
        phase_issue_tracks,
        unique_formats,
        unique_sample_rates,
        unique_bit_depths,
        flac_tracks,
        verified_lossless_tracks,
        suspected_320k_tracks,
        suspected_192k_tracks,
        suspected_128k_tracks,
        bandwidth_limited_tracks,
        native_lossy_tracks,
        alerts,
    }
}
