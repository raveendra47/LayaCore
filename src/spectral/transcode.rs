use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LosslessVerdict {
    /// Full frequency spectrum with energy extending up to near-Nyquist (e.g. >21.5 kHz for 44.1k).
    GenuineLossless,
    /// Brickwall cutoff shelf around 20.0 - 20.6 kHz, typical of MP3 320 kbps or AAC 256 kbps upsampled to lossless.
    SuspectedTranscode320k,
    /// Brickwall cutoff shelf around 18.0 - 19.2 kHz, typical of MP3 192 kbps upsampled to lossless.
    SuspectedTranscode192k,
    /// Sharp brickwall cutoff shelf around 15.5 - 16.5 kHz, typical of low-quality MP3 128 kbps upsampled to lossless.
    SuspectedTranscode128k,
    /// File container or codec is natively lossy (MP3, AAC, OGG, Opus).
    NativeLossy,
    /// Mellow or bandwidth-limited acoustic recording without a steep digital brickwall cliff.
    BandwidthLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub verdict: LosslessVerdict,
    pub verdict_description: String,
    pub detected_cutoff_hz: Option<f32>,
    pub bandwidth_utilized_pct: f32,
    pub shelf_drop_db: f32,
    pub high_freq_energy_dbfs: f32,
    pub nyquist_hz: f32,
    pub confidence_pct: f32,
}

pub struct TranscodeDetector {
    sample_rate: u32,
    fft_size: usize,
    num_bins: usize,
    bin_width: f32,
    bin_peak_powers: Vec<f32>,
    bin_sum_powers: Vec<f64>,
    frame_count: u64,
}

impl TranscodeDetector {
    pub fn new(sample_rate: u32, fft_size: usize) -> Self {
        let num_bins = fft_size / 2 + 1;
        let bin_width = sample_rate as f32 / fft_size as f32;

        Self {
            sample_rate,
            fft_size,
            num_bins,
            bin_width,
            bin_peak_powers: vec![0.0f32; num_bins],
            bin_sum_powers: vec![0.0f64; num_bins],
            frame_count: 0,
        }
    }

    pub fn process_frame(&mut self, pwr_spectrum: &[f32]) {
        self.frame_count += 1;
        let n = self.num_bins.min(pwr_spectrum.len());
        for k in 0..n {
            let p = pwr_spectrum[k];
            if p > self.bin_peak_powers[k] {
                self.bin_peak_powers[k] = p;
            }
            self.bin_sum_powers[k] += p as f64;
        }
    }

    pub fn finish(&self, is_native_lossy: bool) -> QualityReport {
        let nyquist = (self.sample_rate as f32) / 2.0;

        if self.frame_count == 0 {
            return QualityReport {
                verdict: LosslessVerdict::BandwidthLimited,
                verdict_description: "Insufficient audio frames for spectral analysis".into(),
                detected_cutoff_hz: None,
                bandwidth_utilized_pct: 0.0,
                shelf_drop_db: 0.0,
                high_freq_energy_dbfs: -100.0,
                nyquist_hz: nyquist,
                confidence_pct: 0.0,
            };
        }

        // Normalization factor for power to dBFS
        let norm_factor = (self.fft_size as f32 * self.fft_size as f32) * 0.25;

        // Helper to get highest peak bin in dBFS for window [f_start, f_end]
        let band_max_db = |f_start: f32, f_end: f32| -> f32 {
            let k_start = ((f_start / self.bin_width).round() as usize).clamp(0, self.num_bins - 1);
            let k_end = ((f_end / self.bin_width).round() as usize).clamp(k_start + 1, self.num_bins);

            let max_pwr: f32 = self.bin_peak_powers[k_start..k_end]
                .iter()
                .copied()
                .fold(0.0f32, f32::max);
            let norm_p = (max_pwr / norm_factor).max(1e-12);
            10.0 * norm_p.log10()
        };

        // Reference baseline: highest peak energy between 14 kHz and 16 kHz
        let baseline_15k = band_max_db(14000.0f32.min(nyquist * 0.7), 16000.0f32.min(nyquist * 0.8));
        // Energy in top band: near Nyquist (e.g. 21.0 kHz to Nyquist for 44.1k)
        let nyquist_shelf_start = (nyquist - 1200.0).max(14000.0);
        let top_band_db = band_max_db(nyquist_shelf_start, nyquist);

        // Scan for the sharpest cliff/shelf between 14.5 kHz and (nyquist - 400 Hz)
        let mut max_cliff_drop = 0.0f32;
        let mut cliff_freq = 0.0f32;

        let scan_start = 14500.0f32;
        let scan_end = nyquist - 400.0;
        let step = 100.0f32;

        let mut f = scan_start;
        while f <= scan_end {
            // Energy in 500 Hz window below f vs 500 Hz window above f
            let db_below = band_max_db(f - 500.0, f);
            let db_above = band_max_db(f, f + 500.0);
            let drop = db_below - db_above;

            // Also check peak energy from (f + 250) to Nyquist
            let tail_db = band_max_db(f + 250.0, nyquist);

            // A lossy brickwall shelf has a steep drop (> 18 dB within 500 Hz) AND tail drops significantly below baseline
            if drop >= 16.0 && (baseline_15k - tail_db) >= 18.0 && tail_db < -60.0 {
                if drop > max_cliff_drop {
                    max_cliff_drop = drop;
                    cliff_freq = f;
                }
            }

            f += step;
        }

        // Determine verdict
        if is_native_lossy {
            let cutoff_opt = if cliff_freq > 0.0 { Some(cliff_freq) } else { None };
            let bw_pct = cutoff_opt.map(|c| (c / nyquist) * 100.0).unwrap_or(100.0);
            return QualityReport {
                verdict: LosslessVerdict::NativeLossy,
                verdict_description: "Native Lossy Codec (Encoded in lossy container format)".into(),
                detected_cutoff_hz: cutoff_opt,
                bandwidth_utilized_pct: bw_pct,
                shelf_drop_db: max_cliff_drop,
                high_freq_energy_dbfs: top_band_db,
                nyquist_hz: nyquist,
                confidence_pct: 99.0,
            };
        }

        // If sharp cliff was detected in a supposedly lossless file:
        if max_cliff_drop >= 18.0 && cliff_freq > 0.0 {
            let bw_pct = (cliff_freq / nyquist) * 100.0;
            let conf = (70.0 + (max_cliff_drop * 1.2)).clamp(75.0, 99.0);

            if cliff_freq <= 16500.0 {
                return QualityReport {
                    verdict: LosslessVerdict::SuspectedTranscode128k,
                    verdict_description: format!(
                        "FAKE LOSSLESS: Sharp brickwall cutoff at {:.1} kHz (Consistent with 128 kbps MP3 transcode)",
                        cliff_freq / 1000.0
                    ),
                    detected_cutoff_hz: Some(cliff_freq),
                    bandwidth_utilized_pct: bw_pct,
                    shelf_drop_db: max_cliff_drop,
                    high_freq_energy_dbfs: top_band_db,
                    nyquist_hz: nyquist,
                    confidence_pct: conf,
                };
            } else if cliff_freq <= 19200.0 {
                return QualityReport {
                    verdict: LosslessVerdict::SuspectedTranscode192k,
                    verdict_description: format!(
                        "SUSPECTED TRANSCODE: Lowpass shelf at {:.1} kHz (Consistent with 192 kbps MP3 transcode)",
                        cliff_freq / 1000.0
                    ),
                    detected_cutoff_hz: Some(cliff_freq),
                    bandwidth_utilized_pct: bw_pct,
                    shelf_drop_db: max_cliff_drop,
                    high_freq_energy_dbfs: top_band_db,
                    nyquist_hz: nyquist,
                    confidence_pct: conf,
                };
            } else if cliff_freq <= 20700.0 {
                return QualityReport {
                    verdict: LosslessVerdict::SuspectedTranscode320k,
                    verdict_description: format!(
                        "SUSPECTED TRANSCODE: High-frequency cutoff at {:.1} kHz (Consistent with 320 kbps MP3 / 256k AAC transcode)",
                        cliff_freq / 1000.0
                    ),
                    detected_cutoff_hz: Some(cliff_freq),
                    bandwidth_utilized_pct: bw_pct,
                    shelf_drop_db: max_cliff_drop,
                    high_freq_energy_dbfs: top_band_db,
                    nyquist_hz: nyquist,
                    confidence_pct: conf,
                };
            }
        }

        // Check for genuine lossless: energy extends continuously into top octave (> 21 kHz)
        let top_tail_db = band_max_db(nyquist - 1500.0, nyquist);
        if top_tail_db > -68.0 || (baseline_15k - top_tail_db).abs() < 28.0 {
            QualityReport {
                verdict: LosslessVerdict::GenuineLossless,
                verdict_description: format!(
                    "GENUINE LOSSLESS: Full continuous frequency spectrum up to Nyquist ({:.1} kHz)",
                    nyquist / 1000.0
                ),
                detected_cutoff_hz: None,
                bandwidth_utilized_pct: 100.0,
                shelf_drop_db: 0.0,
                high_freq_energy_dbfs: top_tail_db,
                nyquist_hz: nyquist,
                confidence_pct: 95.0,
            }
        } else {
            // Soft or bandwidth-limited acoustic track without a hard digital brickwall
            let bw_pct = ((nyquist - 2000.0) / nyquist) * 100.0;
            QualityReport {
                verdict: LosslessVerdict::BandwidthLimited,
                verdict_description: "Bandwidth Limited Master (Gentle acoustic roll-off, no sharp MP3 brickwall detected)".into(),
                detected_cutoff_hz: None,
                bandwidth_utilized_pct: bw_pct,
                shelf_drop_db: max_cliff_drop,
                high_freq_energy_dbfs: top_tail_db,
                nyquist_hz: nyquist,
                confidence_pct: 80.0,
            }
        }
    }
}
