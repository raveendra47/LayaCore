use serde::{Deserialize, Serialize};

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"
];

// Standard Krumhansl-Schmuckler Key Profiles
const KRUMHANSL_MAJOR: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88
];

const KRUMHANSL_MINOR: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicalStats {
    pub estimated_bpm: f32,
    pub bpm_confidence: f32,
    pub estimated_key: String,
    pub key_confidence: f32,
    pub chroma_profile: Vec<f32>, // 12 notes: C to B
}

pub struct RhythmAndKeyAnalyzer {
    sample_rate: u32,
    _fft_size: usize,
    hop_size: usize,
    prev_magnitude: Vec<f32>,
    onset_curve: Vec<f32>,
    chroma_accum: [f64; 12],
    bin_pitch_classes: Vec<Option<usize>>,
}

impl RhythmAndKeyAnalyzer {
    pub fn new(sample_rate: u32, fft_size: usize, hop_size: usize) -> Self {
        let num_bins = fft_size / 2 + 1;
        let bin_width = sample_rate as f64 / fft_size as f64;

        // Precompute pitch class for each FFT bin between 55 Hz (A1) and 3520 Hz (A7)
        let mut bin_pitch_classes = vec![None; num_bins];
        for k in 1..num_bins {
            let freq = k as f64 * bin_width;
            if freq >= 55.0 && freq <= 3520.0 {
                // MIDI note: p = 69 + 12 * log2(f / 440)
                let midi_pitch = 69.0 + 12.0 * (freq / 440.0).log2();
                let note_index = (midi_pitch.round() as i64).rem_euclid(12) as usize;
                bin_pitch_classes[k] = Some(note_index);
            }
        }

        Self {
            sample_rate,
            _fft_size: fft_size,
            hop_size,
            prev_magnitude: vec![0.0f32; num_bins],
            onset_curve: Vec::with_capacity(2048),
            chroma_accum: [0.0; 12],
            bin_pitch_classes,
        }
    }

    /// Process magnitude and power spectra for this frame
    pub fn process_frame(&mut self, magnitude_spectrum: &[f32], power_spectrum: &[f32]) {
        let num_bins = magnitude_spectrum.len();

        // 1. Spectral Flux (Onset Detection)
        let mut flux = 0.0f32;
        for k in 1..num_bins {
            let diff = magnitude_spectrum[k] - self.prev_magnitude[k];
            if diff > 0.0 {
                flux += diff;
            }
            self.prev_magnitude[k] = magnitude_spectrum[k];
        }
        self.onset_curve.push(flux);

        // 2. Chroma Accumulation for Key Detection
        for k in 1..num_bins {
            if let Some(pitch_class) = self.bin_pitch_classes[k] {
                self.chroma_accum[pitch_class] += power_spectrum[k] as f64;
            }
        }
    }

    pub fn finish(self) -> MusicalStats {
        let (bpm, bpm_conf) = self.estimate_bpm();
        let (key, key_conf, norm_chroma) = self.estimate_key();

        MusicalStats {
            estimated_bpm: bpm,
            bpm_confidence: bpm_conf,
            estimated_key: key,
            key_confidence: key_conf,
            chroma_profile: norm_chroma,
        }
    }

    fn estimate_bpm(&self) -> (f32, f32) {
        if self.onset_curve.len() < 64 {
            return (0.0, 0.0);
        }

        // Normalize onset curve: subtract moving mean
        let mean_flux: f32 = self.onset_curve.iter().sum::<f32>() / self.onset_curve.len() as f32;
        let centered: Vec<f32> = self.onset_curve.iter().map(|&x| (x - mean_flux).max(0.0)).collect();

        // Smooth slightly with 3-point moving average
        let mut smoothed = centered.clone();
        for i in 1..centered.len() - 1 {
            smoothed[i] = 0.25 * centered[i - 1] + 0.5 * centered[i] + 0.25 * centered[i + 1];
        }

        let hop_time = self.hop_size as f64 / self.sample_rate as f64;
        let min_bpm = 60.0f64;
        let max_bpm = 190.0f64;

        let max_lag = ((60.0 / (min_bpm * hop_time)).ceil() as usize).min(smoothed.len() / 2);
        let min_lag = ((60.0 / (max_bpm * hop_time)).floor() as usize).max(2);

        if min_lag >= max_lag {
            return (120.0, 0.0);
        }

        // Autocorrelation over lag range [min_lag, max_lag]
        let mut best_bpm = 120.0f32;
        let mut best_score = 0.0f64;
        let mut all_scores = Vec::with_capacity(max_lag - min_lag + 1);

        for lag in min_lag..=max_lag {
            let mut sum = 0.0f64;
            let n = smoothed.len() - lag;
            for t in 0..n {
                sum += (smoothed[t] as f64) * (smoothed[t + lag] as f64);
            }
            let autocorr = sum / n as f64;

            // Apply gentle perceptual weighting towards common 120 BPM tempo
            let candidate_bpm = 60.0 / (lag as f64 * hop_time);
            let diff_from_120 = (candidate_bpm - 120.0) / 45.0;
            let tempo_prior = (-0.5 * diff_from_120 * diff_from_120).exp();
            let weighted_score = autocorr * (0.8 + 0.2 * tempo_prior);

            all_scores.push((candidate_bpm as f32, weighted_score));

            if weighted_score > best_score {
                best_score = weighted_score;
                best_bpm = candidate_bpm as f32;
            }
        }

        let avg_score = if !all_scores.is_empty() {
            all_scores.iter().map(|s| s.1).sum::<f64>() / all_scores.len() as f64
        } else {
            1.0
        };

        let confidence = if avg_score > 1e-9 {
            ((best_score / avg_score - 1.0) / 3.0).clamp(0.0, 1.0) as f32
        } else {
            0.5
        };

        (best_bpm.round(), confidence)
    }

    fn estimate_key(&self) -> (String, f32, Vec<f32>) {
        let chroma_sum: f64 = self.chroma_accum.iter().sum();
        if chroma_sum < 1e-9 {
            return ("Unknown".to_string(), 0.0, vec![0.0; 12]);
        }

        // Normalize chroma vector to unit sum
        let mut norm_chroma = Vec::with_capacity(12);
        for &val in &self.chroma_accum {
            norm_chroma.push((val / chroma_sum) as f32);
        }

        let mut best_key = "C Major".to_string();
        let mut highest_corr = -2.0f64;

        // Correlate with all 12 Major and 12 Minor keys
        for root in 0..12 {
            // Major correlation
            let maj_corr = pearson_correlation_rotated(&self.chroma_accum, &KRUMHANSL_MAJOR, root);
            if maj_corr > highest_corr {
                highest_corr = maj_corr;
                best_key = format!("{} Major", NOTE_NAMES[root]);
            }

            // Minor correlation
            let min_corr = pearson_correlation_rotated(&self.chroma_accum, &KRUMHANSL_MINOR, root);
            if min_corr > highest_corr {
                highest_corr = min_corr;
                best_key = format!("{} Minor", NOTE_NAMES[root]);
            }
        }

        let confidence = ((highest_corr.max(0.0) - 0.4) / 0.5).clamp(0.0, 1.0) as f32;
        (best_key, confidence, norm_chroma)
    }
}

/// Pearson correlation between observation vector and rotated template
fn pearson_correlation_rotated(observed: &[f64; 12], template: &[f64; 12], rotation: usize) -> f64 {
    let mean_obs = observed.iter().sum::<f64>() / 12.0;
    let mean_tmpl = template.iter().sum::<f64>() / 12.0;

    let mut num = 0.0f64;
    let mut denom_obs = 0.0f64;
    let mut denom_tmpl = 0.0f64;

    for i in 0..12 {
        let tmpl_idx = (i + 12 - rotation) % 12;
        let obs_diff = observed[i] - mean_obs;
        let tmpl_diff = template[tmpl_idx] - mean_tmpl;

        num += obs_diff * tmpl_diff;
        denom_obs += obs_diff * obs_diff;
        denom_tmpl += tmpl_diff * tmpl_diff;
    }

    let denom = (denom_obs * denom_tmpl).sqrt();
    if denom > 1e-12 {
        num / denom
    } else {
        0.0
    }
}
