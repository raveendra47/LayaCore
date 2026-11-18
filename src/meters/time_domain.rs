use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTimeStats {
    pub channel_index: usize,
    pub peak_linear: f32,
    pub peak_dbfs: f32,
    pub rms_linear: f32,
    pub rms_dbfs: f32,
    pub crest_factor_linear: f32,
    pub crest_factor_db: f32,
    pub dc_offset: f32,
    pub clipped_samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeDomainStats {
    pub channels: Vec<ChannelTimeStats>,
    pub overall_peak_dbfs: f32,
    pub overall_rms_dbfs: f32,
    pub dynamic_range_db: f32,
    pub total_clipped_samples: u64,
    pub clipping_events: u64,
    pub silence_duration_seconds: f64,
    pub silence_percentage: f32,
    pub total_samples_processed: u64,
}

pub struct TimeDomainMeter {
    channels: usize,
    sample_rate: u32,
    sum_squares: Vec<f64>,
    sum_samples: Vec<f64>,
    peak_samples: Vec<f32>,
    clipped_sample_counts: Vec<u64>,
    consecutive_clips: Vec<usize>,
    clipping_events: u64,
    total_samples_per_channel: u64,
    
    // Silence tracking (window of 100ms)
    silence_window_samples: usize,
    silence_window_accum: Vec<f64>,
    silence_window_count: usize,
    silent_windows: u64,
    total_windows: u64,
}

impl TimeDomainMeter {
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        let silence_window_samples = (sample_rate as f64 * 0.1).max(1.0) as usize; // 100ms
        Self {
            channels,
            sample_rate,
            sum_squares: vec![0.0; channels],
            sum_samples: vec![0.0; channels],
            peak_samples: vec![0.0; channels],
            clipped_sample_counts: vec![0; channels],
            consecutive_clips: vec![0; channels],
            clipping_events: 0,
            total_samples_per_channel: 0,
            silence_window_samples,
            silence_window_accum: vec![0.0; channels],
            silence_window_count: 0,
            silent_windows: 0,
            total_windows: 0,
        }
    }

    /// Process interleaved frames [L0, R0, L1, R1, ...]
    pub fn process_frames(&mut self, frames: &[f32]) {
        if frames.is_empty() || self.channels == 0 {
            return;
        }

        let num_frames = frames.len() / self.channels;
        self.total_samples_per_channel += num_frames as u64;

        for frame_idx in 0..num_frames {
            let offset = frame_idx * self.channels;

            for ch in 0..self.channels {
                let s = frames[offset + ch];
                let s_abs = s.abs();

                self.sum_squares[ch] += (s as f64) * (s as f64);
                self.sum_samples[ch] += s as f64;

                if s_abs > self.peak_samples[ch] {
                    self.peak_samples[ch] = s_abs;
                }

                // Digital clipping check (>= 0.9999)
                if s_abs >= 0.9999 {
                    self.clipped_sample_counts[ch] += 1;
                    self.consecutive_clips[ch] += 1;
                    if self.consecutive_clips[ch] == 3 {
                        self.clipping_events += 1;
                    }
                } else {
                    self.consecutive_clips[ch] = 0;
                }

                self.silence_window_accum[ch] += (s as f64) * (s as f64);
            }

            self.silence_window_count += 1;
            if self.silence_window_count >= self.silence_window_samples {
                self.total_windows += 1;
                // Check if RMS in this 100ms window was < -60 dBFS (amplitude < 0.001)
                let window_rms_sq: f64 = self.silence_window_accum.iter().sum::<f64>()
                    / (self.channels * self.silence_window_samples) as f64;
                if window_rms_sq < 1e-6 {
                    self.silent_windows += 1;
                }

                for item in &mut self.silence_window_accum {
                    *item = 0.0;
                }
                self.silence_window_count = 0;
            }
        }
    }

    pub fn finish(self) -> TimeDomainStats {
        let n = self.total_samples_per_channel.max(1) as f64;
        let mut channel_stats = Vec::with_capacity(self.channels);

        let mut max_overall_peak = 0.0f32;
        let mut total_overall_energy = 0.0f64;
        let mut total_clipped = 0u64;

        for ch in 0..self.channels {
            let peak_lin = self.peak_samples[ch];
            if peak_lin > max_overall_peak {
                max_overall_peak = peak_lin;
            }

            let rms_lin = (self.sum_squares[ch] / n).sqrt() as f32;
            total_overall_energy += self.sum_squares[ch];

            let peak_dbfs = linear_to_dbfs(peak_lin);
            let rms_dbfs = linear_to_dbfs(rms_lin);

            let crest_factor_lin = if rms_lin > 1e-7 {
                peak_lin / rms_lin
            } else {
                1.0
            };
            let crest_factor_db = peak_dbfs - rms_dbfs;

            let dc_offset = (self.sum_samples[ch] / n) as f32;
            let clips = self.clipped_sample_counts[ch];
            total_clipped += clips;

            channel_stats.push(ChannelTimeStats {
                channel_index: ch,
                peak_linear: peak_lin,
                peak_dbfs,
                rms_linear: rms_lin,
                rms_dbfs,
                crest_factor_linear: crest_factor_lin,
                crest_factor_db,
                dc_offset,
                clipped_samples: clips,
            });
        }

        let overall_rms_lin = ((total_overall_energy / (n * self.channels.max(1) as f64)).sqrt()) as f32;
        let overall_peak_dbfs = linear_to_dbfs(max_overall_peak);
        let overall_rms_dbfs = linear_to_dbfs(overall_rms_lin);
        let dynamic_range_db = (overall_peak_dbfs - overall_rms_dbfs).max(0.0);

        let silence_fraction = if self.total_windows > 0 {
            self.silent_windows as f64 / self.total_windows as f64
        } else {
            0.0
        };

        let total_duration_secs = n / self.sample_rate as f64;
        let silence_duration_seconds = total_duration_secs * silence_fraction;
        let silence_percentage = (silence_fraction * 100.0) as f32;

        TimeDomainStats {
            channels: channel_stats,
            overall_peak_dbfs,
            overall_rms_dbfs,
            dynamic_range_db,
            total_clipped_samples: total_clipped,
            clipping_events: self.clipping_events,
            silence_duration_seconds,
            silence_percentage,
            total_samples_processed: self.total_samples_per_channel * self.channels as u64,
        }
    }
}

pub fn linear_to_dbfs(linear: f32) -> f32 {
    if linear <= 1e-7 {
        -144.0
    } else {
        20.0 * linear.log10()
    }
}
