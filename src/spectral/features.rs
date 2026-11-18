use serde::{Deserialize, Serialize};

pub const ISO_31_BANDS: [f32; 31] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0,
    200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0,
    2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0, 8000.0, 10000.0, 12500.0, 16000.0,
    20000.0,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandEnergy {
    pub center_hz: f32,
    pub energy_db: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequencyBalance {
    pub sub_bass_db: f32,     // 20 - 60 Hz
    pub bass_db: f32,         // 60 - 250 Hz
    pub low_mids_db: f32,     // 250 - 500 Hz
    pub mids_db: f32,         // 500 - 2000 Hz
    pub high_mids_db: f32,    // 2000 - 6000 Hz
    pub brilliance_db: f32,   // 6000 - 20000 Hz
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectralStats {
    pub spectral_centroid_hz: f32,
    pub spectral_rolloff_hz: f32,
    pub spectral_flatness: f32, // 0.0 (pure tone) to 1.0 (white noise)
    pub balance: FrequencyBalance,
    pub bands_31: Vec<BandEnergy>,
}

pub struct SpectralFeatureExtractor {
    _sample_rate: u32,
    _fft_size: usize,
    num_bins: usize,
    bin_frequencies: Vec<f32>,
    band_bin_ranges: Vec<(usize, usize)>,

    // Accumulators
    frame_count: u64,
    sum_centroid: f64,
    sum_rolloff: f64,
    sum_flatness: f64,
    band_powers: Vec<f64>,
    register_powers: [f64; 6], // Sub, Bass, LowMid, Mid, HighMid, Brilliance
}

impl SpectralFeatureExtractor {
    pub fn new(sample_rate: u32, fft_size: usize) -> Self {
        let num_bins = fft_size / 2 + 1;
        let bin_width = sample_rate as f32 / fft_size as f32;
        let mut bin_frequencies = Vec::with_capacity(num_bins);
        for k in 0..num_bins {
            bin_frequencies.push(k as f32 * bin_width);
        }

        // Map ISO 31 bands to bin index ranges [start_bin, end_bin]
        let factor = 2.0f32.powf(1.0 / 6.0); // 1/6 octave ratio for half-band edges
        let mut band_bin_ranges = Vec::with_capacity(ISO_31_BANDS.len());
        for &fc in &ISO_31_BANDS {
            let f_low = (fc / factor).max(10.0);
            let f_high = (fc * factor).min(sample_rate as f32 / 2.0);

            let k_low = (f_low / bin_width).floor() as usize;
            let k_high = ((f_high / bin_width).ceil() as usize).min(num_bins - 1);
            band_bin_ranges.push((k_low.min(k_high), k_high.max(k_low)));
        }

        Self {
            _sample_rate: sample_rate,
            _fft_size: fft_size,
            num_bins,
            bin_frequencies,
            band_bin_ranges,
            frame_count: 0,
            sum_centroid: 0.0,
            sum_rolloff: 0.0,
            sum_flatness: 0.0,
            band_powers: vec![0.0; ISO_31_BANDS.len()],
            register_powers: [0.0; 6],
        }
    }

    /// Process power spectrum for one frame (|X[k]|^2)
    pub fn process_frame(&mut self, power_spectrum: &[f32], magnitude_spectrum: &[f32]) {
        assert_eq!(power_spectrum.len(), self.num_bins);
        self.frame_count += 1;

        let mut total_magnitude = 0.0f64;
        let mut weighted_freq_sum = 0.0f64;
        let mut total_power = 0.0f64;

        for k in 1..self.num_bins {
            let mag = magnitude_spectrum[k] as f64;
            let pwr = power_spectrum[k] as f64;
            let freq = self.bin_frequencies[k] as f64;

            total_magnitude += mag;
            weighted_freq_sum += freq * mag;
            total_power += pwr;

            // Register grouping
            if freq >= 20.0 && freq < 60.0 {
                self.register_powers[0] += pwr;
            } else if freq >= 60.0 && freq < 250.0 {
                self.register_powers[1] += pwr;
            } else if freq >= 250.0 && freq < 500.0 {
                self.register_powers[2] += pwr;
            } else if freq >= 500.0 && freq < 2000.0 {
                self.register_powers[3] += pwr;
            } else if freq >= 2000.0 && freq < 6000.0 {
                self.register_powers[4] += pwr;
            } else if freq >= 6000.0 && freq <= 20000.0 {
                self.register_powers[5] += pwr;
            }
        }

        // 1. Spectral Centroid
        let centroid = if total_magnitude > 1e-9 {
            weighted_freq_sum / total_magnitude
        } else {
            0.0
        };
        self.sum_centroid += centroid;

        // 2. Spectral Rolloff (85% energy)
        let rolloff_threshold = total_power * 0.85;
        let mut cum_power = 0.0f64;
        let mut rolloff_freq = 0.0f64;
        for k in 1..self.num_bins {
            cum_power += power_spectrum[k] as f64;
            if cum_power >= rolloff_threshold {
                rolloff_freq = self.bin_frequencies[k] as f64;
                break;
            }
        }
        self.sum_rolloff += rolloff_freq;

        // 3. Spectral Flatness = Geometric Mean / Arithmetic Mean
        let n = (self.num_bins - 1) as f64;
        if total_power > 1e-12 {
            let arith_mean = total_power / n;
            let mut log_sum = 0.0f64;
            for k in 1..self.num_bins {
                let p = (power_spectrum[k] as f64).max(1e-12);
                log_sum += p.ln();
            }
            let geom_mean = (log_sum / n).exp();
            let flatness = (geom_mean / arith_mean).clamp(0.0, 1.0);
            self.sum_flatness += flatness;
        }

        // 4. Accumulate 31 ISO bands
        for (i, &(k_low, k_high)) in self.band_bin_ranges.iter().enumerate() {
            let mut band_energy = 0.0f64;
            for k in k_low..=k_high {
                band_energy += power_spectrum[k] as f64;
            }
            self.band_powers[i] += band_energy;
        }
    }

    pub fn finish(self) -> SpectralStats {
        let count = self.frame_count.max(1) as f64;

        let spectral_centroid_hz = (self.sum_centroid / count) as f32;
        let spectral_rolloff_hz = (self.sum_rolloff / count) as f32;
        let spectral_flatness = (self.sum_flatness / count) as f32;

        let mut bands_31 = Vec::with_capacity(ISO_31_BANDS.len());
        let max_band_energy = self.band_powers.iter().copied().fold(1e-12f64, f64::max);

        for (i, &center_hz) in ISO_31_BANDS.iter().enumerate() {
            let avg_pwr = self.band_powers[i] / count;
            // Relative dB normalized to max band (0 dB peak)
            let rel_db = if avg_pwr > 1e-12 {
                10.0 * (avg_pwr / (max_band_energy / count)).log10()
            } else {
                -72.0
            } as f32;

            bands_31.push(BandEnergy {
                center_hz,
                energy_db: rel_db.clamp(-72.0, 0.0),
            });
        }

        let max_reg_pwr = self.register_powers.iter().copied().fold(1e-12f64, f64::max);
        let reg_db = |pwr: f64| -> f32 {
            if pwr > 1e-12 {
                (10.0 * (pwr / max_reg_pwr).log10()) as f32
            } else {
                -72.0
            }
        };

        let balance = FrequencyBalance {
            sub_bass_db: reg_db(self.register_powers[0]),
            bass_db: reg_db(self.register_powers[1]),
            low_mids_db: reg_db(self.register_powers[2]),
            mids_db: reg_db(self.register_powers[3]),
            high_mids_db: reg_db(self.register_powers[4]),
            brilliance_db: reg_db(self.register_powers[5]),
        };

        SpectralStats {
            spectral_centroid_hz,
            spectral_rolloff_hz,
            spectral_flatness,
            balance,
            bands_31,
        }
    }
}
