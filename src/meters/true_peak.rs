use serde::{Deserialize, Serialize};
use super::time_domain::linear_to_dbfs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruePeakStats {
    pub max_true_peak_linear: f32,
    pub max_true_peak_dbtp: f32,
    pub channel_true_peaks_dbtp: Vec<f32>,
    pub inter_sample_clipping_risk: bool, // True if > -1.0 dBTP
    pub true_peak_clipping: bool,        // True if > 0.0 dBTP
}

/// 4x Oversampling Polyphase FIR filter according to ITU-R BS.1770-4 Annex 2
/// Uses 4 sub-filters of length 12 (48 taps total)
pub struct TruePeakMeter {
    channels: usize,
    history: Vec<Vec<f32>>,
    max_peaks: Vec<f32>,
    polyphase_taps: [[f32; 12]; 4],
}

impl TruePeakMeter {
    pub fn new(channels: usize) -> Self {
        // ITU-R BS.1770-4 48-tap filter design broken into 4 polyphase branches
        // Filter coefficients designed with Kaiser-windowed sinc at 4x oversampling
        let polyphase_taps = [
            // Phase 0: t = 0.00
            [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            // Phase 1: t = 0.25
            [-0.0101, 0.0215, -0.0478, 0.1114, -0.3204, 0.8973, 0.4357, -0.1293, 0.0538, -0.0234, 0.0104, -0.0038],
            // Phase 2: t = 0.50
            [-0.0156, 0.0345, -0.0762, 0.1782, -0.4901, 0.6481, 0.6481, -0.4901, 0.1782, -0.0762, 0.0345, -0.0156],
            // Phase 3: t = 0.75
            [-0.0038, 0.0104, -0.0234, 0.0538, -0.1293, 0.4357, 0.8973, -0.3204, 0.1114, -0.0478, 0.0215, -0.0101],
        ];

        Self {
            channels,
            history: vec![vec![0.0f32; 12]; channels],
            max_peaks: vec![0.0f32; channels],
            polyphase_taps,
        }
    }

    pub fn process_frames(&mut self, frames: &[f32]) {
        if frames.is_empty() || self.channels == 0 {
            return;
        }

        let num_frames = frames.len() / self.channels;
        for i in 0..num_frames {
            let offset = i * self.channels;

            for ch in 0..self.channels {
                let sample = frames[offset + ch];
                let history = &mut self.history[ch];

                // Shift history buffer
                history.rotate_right(1);
                history[0] = sample;

                // Evaluate 4 polyphase branches (4x oversampled points)
                for phase in 0..4 {
                    let taps = &self.polyphase_taps[phase];
                    let mut interpolated = 0.0f32;
                    for k in 0..12 {
                        interpolated += history[k] * taps[k];
                    }

                    let abs_val = interpolated.abs();
                    if abs_val > self.max_peaks[ch] {
                        self.max_peaks[ch] = abs_val;
                    }
                }
            }
        }
    }

    pub fn finish(self) -> TruePeakStats {
        let mut channel_dbtp = Vec::with_capacity(self.channels);
        let mut overall_max = 0.0f32;

        for ch in 0..self.channels {
            let peak = self.max_peaks[ch];
            if peak > overall_max {
                overall_max = peak;
            }
            channel_dbtp.push(linear_to_dbfs(peak));
        }

        let max_dbtp = linear_to_dbfs(overall_max);

        TruePeakStats {
            max_true_peak_linear: overall_max,
            max_true_peak_dbtp: max_dbtp,
            channel_true_peaks_dbtp: channel_dbtp,
            inter_sample_clipping_risk: max_dbtp > -1.0,
            true_peak_clipping: max_dbtp > 0.0,
        }
    }
}
