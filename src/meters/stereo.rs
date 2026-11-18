use serde::{Deserialize, Serialize};
use super::time_domain::linear_to_dbfs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StereoStats {
    pub is_stereo: bool,
    pub phase_correlation: f32,
    pub mono_compatibility_risk: bool,
    pub mid_rms_dbfs: f32,
    pub side_rms_dbfs: f32,
    pub mid_side_ratio_db: f32,
    pub stereo_width_percent: f32,
    pub lr_balance_percent: f32, // -100% = hard right, 0% = centered, +100% = hard left
}

pub struct StereoMeter {
    is_stereo: bool,
    sum_l_times_r: f64,
    sum_l_sq: f64,
    sum_r_sq: f64,
    sum_mid_sq: f64,
    sum_side_sq: f64,
    sample_count: u64,
}

impl StereoMeter {
    pub fn new(channels: usize) -> Self {
        Self {
            is_stereo: channels >= 2,
            sum_l_times_r: 0.0,
            sum_l_sq: 0.0,
            sum_r_sq: 0.0,
            sum_mid_sq: 0.0,
            sum_side_sq: 0.0,
            sample_count: 0,
        }
    }

    /// Process interleaved frames [L0, R0, L1, R1, ...]
    pub fn process_frames(&mut self, frames: &[f32], channels: usize) {
        if !self.is_stereo || channels < 2 || frames.is_empty() {
            return;
        }

        let num_frames = frames.len() / channels;
        let inv_sqrt_2 = 1.0f64 / 2.0f64.sqrt();

        for i in 0..num_frames {
            let offset = i * channels;
            let l = frames[offset] as f64;
            let r = frames[offset + 1] as f64;

            self.sum_l_times_r += l * r;
            self.sum_l_sq += l * l;
            self.sum_r_sq += r * r;

            let mid = (l + r) * inv_sqrt_2;
            let side = (l - r) * inv_sqrt_2;

            self.sum_mid_sq += mid * mid;
            self.sum_side_sq += side * side;
            self.sample_count += 1;
        }
    }

    pub fn finish(self) -> StereoStats {
        if !self.is_stereo || self.sample_count == 0 {
            return StereoStats {
                is_stereo: false,
                phase_correlation: 1.0,
                mono_compatibility_risk: false,
                mid_rms_dbfs: -144.0,
                side_rms_dbfs: -144.0,
                mid_side_ratio_db: 0.0,
                stereo_width_percent: 0.0,
                lr_balance_percent: 0.0,
            };
        }

        let denom = (self.sum_l_sq * self.sum_r_sq).sqrt();
        let phase_corr = if denom > 1e-12 {
            (self.sum_l_times_r / denom).clamp(-1.0, 1.0) as f32
        } else {
            1.0
        };

        let n = self.sample_count as f64;
        let mid_rms = (self.sum_mid_sq / n).sqrt() as f32;
        let side_rms = (self.sum_side_sq / n).sqrt() as f32;

        let mid_rms_dbfs = linear_to_dbfs(mid_rms);
        let side_rms_dbfs = linear_to_dbfs(side_rms);

        let mid_side_ratio_db = side_rms_dbfs - mid_rms_dbfs;
        let total_ms = mid_rms + side_rms;
        let stereo_width_percent = if total_ms > 1e-6 {
            (side_rms / total_ms) * 200.0 // 0% = pure mono, 100% = standard balanced stereo
        } else {
            0.0
        };

        let l_rms = (self.sum_l_sq / n).sqrt();
        let r_rms = (self.sum_r_sq / n).sqrt();
        let total_lr = l_rms + r_rms;
        let lr_balance_percent = if total_lr > 1e-6 {
            ((l_rms - r_rms) / total_lr * 100.0) as f32
        } else {
            0.0
        };

        // Mono risk if correlation is negative
        let mono_compatibility_risk = phase_corr < 0.0;

        StereoStats {
            is_stereo: true,
            phase_correlation: phase_corr,
            mono_compatibility_risk,
            mid_rms_dbfs,
            side_rms_dbfs,
            mid_side_ratio_db,
            stereo_width_percent,
            lr_balance_percent,
        }
    }
}
