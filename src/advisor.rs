use serde::{Deserialize, Serialize};

use crate::decoder::AudioMetadata;
use crate::meters::{LufsStats, StereoStats, TimeDomainStats, TruePeakStats};
use crate::spectral::{LosslessVerdict, QualityReport, SpectralStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdviceSeverity {
    Critical,
    Warning,
    Tip,
    Good,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringAdvice {
    pub severity: AdviceSeverity,
    pub category: String,
    pub summary: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringAdvisorReport {
    pub readiness_score: u8,
    pub readiness_grade: String,
    pub items: Vec<MasteringAdvice>,
}

/// Evaluates all acoustic measurements and produces plain-English, actionable
/// mastering & mixing recommendations alongside a 0–100 release-readiness score.
pub fn generate_mastering_advice(
    meta: &AudioMetadata,
    time: &TimeDomainStats,
    stereo: &StereoStats,
    lufs: &LufsStats,
    tp: &TruePeakStats,
    spectral: &SpectralStats,
    quality: &QualityReport,
) -> MasteringAdvisorReport {
    let mut items = Vec::new();
    let mut score: i32 = 100;

    // 1. HEADROOM & CLIPPING (True Peak + Sample Clips)
    if tp.true_peak_clipping || time.total_clipped_samples > 0 {
        score -= 25;
        let needed_cut = (tp.max_true_peak_dbtp - (-1.0)).max(0.5);
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Critical,
            category: "HEADROOM".into(),
            summary: "Digital / Inter-Sample Clipping".into(),
            action: format!(
                "Lower limiter ceiling by ~{:.1} dB to hit -1.0 dBTP ({:.2} dBTP, {} clipped sample(s)).",
                needed_cut, tp.max_true_peak_dbtp, time.total_clipped_samples
            ),
        });
    } else if tp.inter_sample_clipping_risk {
        score -= 10;
        let needed_cut = (tp.max_true_peak_dbtp - (-1.0)).max(0.1);
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Warning,
            category: "HEADROOM".into(),
            summary: "True Peak Above Streaming Ceiling".into(),
            action: format!(
                "Trim ceiling by {:.2} dB ({:.2} -> -1.00 dBTP) to prevent AAC/Ogg codec clipping.",
                needed_cut, tp.max_true_peak_dbtp
            ),
        });
    } else {
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Good,
            category: "HEADROOM".into(),
            summary: "Clean True Peak Headroom".into(),
            action: format!(
                "Peak at {:.2} dBTP safely meets <= -1.0 dBTP streaming target with zero clips.",
                tp.max_true_peak_dbtp
            ),
        });
    }

    // 2. LOUDNESS & STREAMING COMPLIANCE
    let int_lufs = lufs.integrated_lufs;
    if int_lufs.is_finite() && int_lufs > -65.0 {
        if int_lufs > -9.0 {
            score -= 10;
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Warning,
                category: "LOUDNESS".into(),
                summary: "Aggressively Hot Master".into(),
                action: format!(
                    "{:.1} LUFS will be attenuated by {:.1} dB on Spotify/YouTube (-14 LUFS); ease limiter 2–3 dB for punch.",
                    int_lufs,
                    (int_lufs - (-14.0)).max(0.0)
                ),
            });
        } else if int_lufs > -12.8 {
            score -= 2;
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Tip,
                category: "LOUDNESS".into(),
                summary: "Competitive Commercial Loudness".into(),
                action: format!(
                    "{:.1} LUFS (streaming turns down ~{:.1} dB). Fits modern Pop/EDM/Rock; keep ceiling <= -1.0 dBTP.",
                    int_lufs,
                    (int_lufs - (-14.0)).max(0.0)
                ),
            });
        } else if int_lufs >= -15.5 {
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Good,
                category: "LOUDNESS".into(),
                summary: "Streaming-Optimal Loudness".into(),
                action: format!(
                    "{:.1} LUFS lands right in the -14 LUFS sweet spot for Spotify, YouTube & Apple Music.",
                    int_lufs
                ),
            });
        } else {
            score -= 2;
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Tip,
                category: "LOUDNESS".into(),
                summary: "Dynamic / Quiet Master".into(),
                action: format!(
                    "{:.1} LUFS ({:.1} dB under -14 LUFS). Ideal for acoustic/classical, or add gentle gain for playlists.",
                    int_lufs,
                    (-14.0 - int_lufs).max(0.0)
                ),
            });
        }
    }

    // 3. DYNAMICS & TRANSIENT PRESERVATION
    let dr = time.dynamic_range_db;
    let lra = lufs.loudness_range_lra;
    if dr < 6.0 && int_lufs > -12.0 {
        score -= 10;
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Warning,
            category: "DYNAMICS".into(),
            summary: "Heavily Compressed / Brickwalled".into(),
            action: format!(
                "Crest factor {:.1} dB (LRA {:.1} LU) shows heavy limiting; back off bus compression for transient life.",
                dr, lra
            ),
        });
    } else if (8.0..=18.0).contains(&dr) {
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Good,
            category: "DYNAMICS".into(),
            summary: "Balanced Dynamic Contrast".into(),
            action: format!(
                "Crest factor {:.1} dB and LRA {:.1} LU preserve natural punch and groove.",
                dr, lra
            ),
        });
    }

    // 4. STEREO FIELD & MONO COMPATIBILITY
    if stereo.is_stereo {
        if stereo.phase_correlation < 0.0 {
            score -= 25;
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Critical,
                category: "STEREO".into(),
                summary: "Phase Cancellation Detected".into(),
                action: format!(
                    "Correlation {:.2} causes mono dropout. Check polarity, stereo wideners, and mono bass < 120 Hz.",
                    stereo.phase_correlation
                ),
            });
        } else if stereo.phase_correlation < 0.25 {
            score -= 10;
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Warning,
                category: "STEREO".into(),
                summary: "Very Wide Stereo / Low Correlation".into(),
                action: format!(
                    "Correlation {:.2} (width {:.0}%). Verify mono fold-down and center low frequencies (< 120 Hz).",
                    stereo.phase_correlation, stereo.stereo_width_percent
                ),
            });
        } else if stereo.lr_balance_percent.abs() > 8.0 {
            score -= 8;
            let side = if stereo.lr_balance_percent > 0.0 {
                "Left"
            } else {
                "Right"
            };
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Warning,
                category: "STEREO".into(),
                summary: "Asymmetric L/R Pan Balance".into(),
                action: format!(
                    "Mix leans {:.1}% toward {} channel. Inspect stereo bus balance and panned low-mid elements.",
                    stereo.lr_balance_percent.abs(),
                    side
                ),
            });
        } else {
            items.push(MasteringAdvice {
                severity: AdviceSeverity::Good,
                category: "STEREO".into(),
                summary: "Healthy Stereo Translation".into(),
                action: format!(
                    "Phase correlation {:.2} with balanced L/R image translates cleanly to mono & club systems.",
                    stereo.phase_correlation
                ),
            });
        }
    }

    // 5. TONAL BALANCE & SPECTRAL EQ GUIDANCE
    let bal = &spectral.balance;
    if bal.sub_bass_db > bal.mids_db + 9.0 || bal.bass_db > bal.mids_db + 10.0 {
        score -= 3;
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Tip,
            category: "TONAL EQ".into(),
            summary: "Heavy Low-End Weight".into(),
            action: format!(
                "Sub/Bass ({:+.1}/{:+.1} dB) dominates mids ({:+.1} dB). Try a 25–30 Hz HPF or -1.5 dB shelf < 120 Hz.",
                bal.sub_bass_db, bal.bass_db, bal.mids_db
            ),
        });
    }
    if bal.low_mids_db > bal.mids_db + 5.0 && bal.low_mids_db > bal.high_mids_db + 6.0 {
        score -= 3;
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Tip,
            category: "TONAL EQ".into(),
            summary: "Elevated Low-Mids (250–500 Hz)".into(),
            action: format!(
                "Low-mid energy ({:+.1} dB) may sound muddy. A gentle -1.5 to -2.5 dB dip around 300–400 Hz adds clarity.",
                bal.low_mids_db
            ),
        });
    }
    if spectral.spectral_centroid_hz > 3600.0 || bal.high_mids_db > bal.mids_db + 4.5 {
        score -= 3;
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Tip,
            category: "TONAL EQ".into(),
            summary: "Bright / Forward Upper Mids".into(),
            action: format!(
                "Centroid at {:.0} Hz. Check 2.5–5 kHz for fatigue or vocal sibilance with a dynamic EQ / de-esser.",
                spectral.spectral_centroid_hz
            ),
        });
    } else if spectral.spectral_centroid_hz > 0.0
        && spectral.spectral_centroid_hz < 1200.0
        && bal.brilliance_db < bal.mids_db - 18.0
    {
        score -= 2;
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Tip,
            category: "TONAL EQ".into(),
            summary: "Dark / Warm Top-End".into(),
            action: format!(
                "Centroid {:.0} Hz with recessed air ({:+.1} dB). Add a subtle +1.5 dB high shelf > 10 kHz if extra air is wanted.",
                spectral.spectral_centroid_hz, bal.brilliance_db
            ),
        });
    }

    // 6. SILENCE / TRIMMING
    if time.silence_percentage > 12.0 {
        score -= 3;
        items.push(MasteringAdvice {
            severity: AdviceSeverity::Tip,
            category: "TRIM".into(),
            summary: "Extended Silence Detected".into(),
            action: format!(
                "{:.1}% ({:.1}s) is below -60 dBFS. Trim excess leading/trailing dead air before release.",
                time.silence_percentage, time.silence_duration_seconds
            ),
        });
    }

    // 7. FLAC LOSSLESS AUTHENTICITY
    if meta.is_flac() {
        match quality.verdict {
            LosslessVerdict::SuspectedTranscode128k | LosslessVerdict::SuspectedTranscode192k => {
                score -= 25;
                items.push(MasteringAdvice {
                    severity: AdviceSeverity::Critical,
                    category: "AUTHENTICITY".into(),
                    summary: "Fake Lossless Transcode Source".into(),
                    action: format!(
                        "Brickwall cutoff at ~{:.1} kHz indicates lossy MP3 origin. Replace with a genuine lossless master.",
                        quality.detected_cutoff_hz.unwrap_or(16000.0) / 1000.0
                    ),
                });
            }
            LosslessVerdict::SuspectedTranscode320k => {
                score -= 10;
                items.push(MasteringAdvice {
                    severity: AdviceSeverity::Warning,
                    category: "AUTHENTICITY".into(),
                    summary: "Suspected 320k MP3 Upscale".into(),
                    action: format!(
                        "Cutoff shelf near ~{:.1} kHz matches 320 kbps MP3 encoding. Verify against original studio master.",
                        quality.detected_cutoff_hz.unwrap_or(20100.0) / 1000.0
                    ),
                });
            }
            _ => {}
        }
    }

    // Sort: Critical -> Warning -> Tip -> Good
    items.sort_by_key(|it| match it.severity {
        AdviceSeverity::Critical => 0,
        AdviceSeverity::Warning => 1,
        AdviceSeverity::Tip => 2,
        AdviceSeverity::Good => 3,
    });

    let readiness_score = score.clamp(0, 100) as u8;
    let readiness_grade = if readiness_score >= 90 {
        "RELEASE READY".to_string()
    } else if readiness_score >= 75 {
        "GOOD (MINOR TWEAKS)".to_string()
    } else if readiness_score >= 55 {
        "NEEDS MASTERING WORK".to_string()
    } else {
        "CRITICAL ISSUES FOUND".to_string()
    };

    MasteringAdvisorReport {
        readiness_score,
        readiness_grade,
        items,
    }
}
