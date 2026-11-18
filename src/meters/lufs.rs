use std::f64::consts::PI;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LufsStats {
    pub integrated_lufs: f32,
    pub momentary_max_lufs: f32,
    pub short_term_max_lufs: f32,
    pub loudness_range_lra: f32,
    pub streaming_compliance: StreamingCompliance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingCompliance {
    pub spotify_diff_lu: f32,    // target: -14.0 LUFS
    pub youtube_diff_lu: f32,    // target: -14.0 LUFS
    pub apple_music_diff_lu: f32,// target: -16.0 LUFS
    pub podcast_diff_lu: f32,    // target: -16.0 LUFS
    pub broadcast_diff_lu: f32,  // target: -23.0 LUFS (EBU R128)
}

#[derive(Clone)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0, b1, b2, a1, a2,
            x1: 0.0, x2: 0.0,
            y1: 0.0, y2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub struct LufsMeter {
    channels: usize,
    _sample_rate: u32,
    channel_weights: Vec<f64>,
    pre_filters: Vec<Biquad>,
    rlb_filters: Vec<Biquad>,

    // Block buffers (400ms momentary blocks, 100ms step)
    block_size_samples: usize,
    step_size_samples: usize,
    ring_buf: Vec<Vec<f64>>,
    ring_idx: usize,
    samples_since_step: usize,

    // Collected 400ms block energies for integrated LUFS
    block_energies: Vec<f64>,
    momentary_max: f64,

    // Short-term (3-second) window collection for LRA
    short_term_blocks_count: usize,
    short_term_energies: Vec<f64>,
    short_term_max: f64,
}

impl LufsMeter {
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        let fs = sample_rate as f64;

        // BS.1770-4 Stage 1: Pre-filter high-shelf
        // f0 = 1681.974450955533 Hz, Gain = 3.999843853973347 dB, Q = 0.7071752369554193
        let f0_shelf = 1681.974450955533;
        let gain_db = 3.999843853973347;
        let _q_shelf = 0.7071752369554193;

        let v0 = 10.0f64.powf(gain_db / 20.0);
        let k_shelf = (PI * f0_shelf / fs).tan();
        let alpha_shelf = 1.0 + (2.0 * v0).sqrt() * k_shelf + v0 * k_shelf * k_shelf;
        let b0_shelf = (v0 + (2.0 * v0).sqrt() * k_shelf + k_shelf * k_shelf) / alpha_shelf;
        let b1_shelf = 2.0 * (k_shelf * k_shelf - v0) / alpha_shelf;
        let b2_shelf = (v0 - (2.0 * v0).sqrt() * k_shelf + k_shelf * k_shelf) / alpha_shelf;
        let a1_shelf = 2.0 * (k_shelf * k_shelf - 1.0) / alpha_shelf;
        let a2_shelf = (1.0 - 2.0f64.sqrt() * k_shelf + k_shelf * k_shelf) / alpha_shelf;

        // BS.1770-4 Stage 2: RLB filter high-pass
        // f0 = 38.13547087602444 Hz, Q = 0.5003270373237373
        let f0_rlb = 38.13547087602444;
        let q_rlb = 0.5003270373237373;
        let k_rlb = (PI * f0_rlb / fs).tan();
        let alpha_rlb = 1.0 + k_rlb / q_rlb + k_rlb * k_rlb;
        let b0_rlb = 1.0 / alpha_rlb;
        let b1_rlb = -2.0 / alpha_rlb;
        let b2_rlb = 1.0 / alpha_rlb;
        let a1_rlb = 2.0 * (k_rlb * k_rlb - 1.0) / alpha_rlb;
        let a2_rlb = (1.0 - k_rlb / q_rlb + k_rlb * k_rlb) / alpha_rlb;

        let mut pre_filters = Vec::with_capacity(channels);
        let mut rlb_filters = Vec::with_capacity(channels);
        let mut channel_weights = Vec::with_capacity(channels);

        for ch in 0..channels {
            pre_filters.push(Biquad::new(b0_shelf, b1_shelf, b2_shelf, a1_shelf, a2_shelf));
            rlb_filters.push(Biquad::new(b0_rlb, b1_rlb, b2_rlb, a1_rlb, a2_rlb));

            // BS.1770 channel weight: Left/Right/Center = 1.0, Surround = 1.41
            let weight = if ch < 3 { 1.0 } else { 1.41 };
            channel_weights.push(weight);
        }

        let block_size_samples = (fs * 0.400).round() as usize; // 400ms
        let step_size_samples = (fs * 0.100).round() as usize;  // 100ms
        let ring_buf = vec![vec![0.0; block_size_samples]; channels];

        Self {
            channels,
            _sample_rate: sample_rate,
            channel_weights,
            pre_filters,
            rlb_filters,
            block_size_samples,
            step_size_samples,
            ring_buf,
            ring_idx: 0,
            samples_since_step: 0,
            block_energies: Vec::new(),
            momentary_max: -144.0,
            short_term_blocks_count: 30, // 30 steps of 100ms = 3.0s
            short_term_energies: Vec::new(),
            short_term_max: -144.0,
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
                let raw_sample = frames[offset + ch] as f64;
                let s_pre = self.pre_filters[ch].process(raw_sample);
                let s_k = self.rlb_filters[ch].process(s_pre);
                let s_k_sq = s_k * s_k;

                self.ring_buf[ch][self.ring_idx] = s_k_sq;
            }

            self.ring_idx = (self.ring_idx + 1) % self.block_size_samples;
            self.samples_since_step += 1;

            if self.samples_since_step >= self.step_size_samples {
                self.samples_since_step = 0;

                // Compute block energy for 400ms window
                let mut total_block_energy = 0.0f64;
                for ch in 0..self.channels {
                    let ch_sum: f64 = self.ring_buf[ch].iter().sum();
                    let ch_mean = ch_sum / self.block_size_samples as f64;
                    total_block_energy += self.channel_weights[ch] * ch_mean;
                }

                if total_block_energy > 0.0 {
                    self.block_energies.push(total_block_energy);
                    let m_lufs = -0.691 + 10.0 * total_block_energy.log10();
                    if m_lufs > self.momentary_max {
                        self.momentary_max = m_lufs;
                    }
                }

                // Short-term (3-second) sliding window
                if self.block_energies.len() >= self.short_term_blocks_count {
                    let start = self.block_energies.len() - self.short_term_blocks_count;
                    let st_energy: f64 = self.block_energies[start..].iter().sum::<f64>()
                        / self.short_term_blocks_count as f64;
                    if st_energy > 0.0 {
                        self.short_term_energies.push(st_energy);
                        let st_lufs = -0.691 + 10.0 * st_energy.log10();
                        if st_lufs > self.short_term_max {
                            self.short_term_max = st_lufs;
                        }
                    }
                }
            }
        }
    }

    pub fn finish(self) -> LufsStats {
        if self.block_energies.is_empty() {
            return LufsStats {
                integrated_lufs: -144.0,
                momentary_max_lufs: -144.0,
                short_term_max_lufs: -144.0,
                loudness_range_lra: 0.0,
                streaming_compliance: StreamingCompliance {
                    spotify_diff_lu: 0.0,
                    youtube_diff_lu: 0.0,
                    apple_music_diff_lu: 0.0,
                    podcast_diff_lu: 0.0,
                    broadcast_diff_lu: 0.0,
                },
            };
        }

        // Absolute threshold gating: discard blocks < -70 LUFS
        // -0.691 + 10 * log10(z) > -70.0 => 10 * log10(z) > -69.309 => z > 10^(-6.9309)
        let abs_threshold_energy = 10.0f64.powf((-70.0 + 0.691) / 10.0);
        let blocks_above_abs: Vec<f64> = self
            .block_energies
            .iter()
            .copied()
            .filter(|&e| e > abs_threshold_energy)
            .collect();

        let integrated_lufs = if blocks_above_abs.is_empty() {
            -144.0
        } else {
            // Compute average of surviving blocks to get relative threshold
            let avg_abs_energy: f64 = blocks_above_abs.iter().sum::<f64>() / blocks_above_abs.len() as f64;
            let rel_threshold_energy = avg_abs_energy * 10.0f64.powf(-10.0 / 10.0); // -10 LU relative gate

            let blocks_above_rel: Vec<f64> = blocks_above_abs
                .into_iter()
                .filter(|&e| e > rel_threshold_energy)
                .collect();

            if blocks_above_rel.is_empty() {
                -144.0
            } else {
                let final_energy = blocks_above_rel.iter().sum::<f64>() / blocks_above_rel.len() as f64;
                (-0.691 + 10.0 * final_energy.log10()) as f32
            }
        };

        // Compute Loudness Range (LRA) from short-term energies
        let lra = if self.short_term_energies.len() < 5 {
            0.0
        } else {
            let mut st_lufs_vals: Vec<f64> = self
                .short_term_energies
                .iter()
                .filter(|&&e| e > abs_threshold_energy)
                .map(|&e| -0.691 + 10.0 * e.log10())
                .collect();

            if st_lufs_vals.len() < 5 {
                0.0
            } else {
                st_lufs_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let p10_idx = ((st_lufs_vals.len() as f64) * 0.10).floor() as usize;
                let p95_idx = (((st_lufs_vals.len() as f64) * 0.95).floor() as usize)
                    .min(st_lufs_vals.len() - 1);
                (st_lufs_vals[p95_idx] - st_lufs_vals[p10_idx]).max(0.0) as f32
            }
        };

        let momentary_max = self.momentary_max.max(-144.0) as f32;
        let short_term_max = self.short_term_max.max(-144.0) as f32;

        LufsStats {
            integrated_lufs,
            momentary_max_lufs: momentary_max,
            short_term_max_lufs: short_term_max,
            loudness_range_lra: lra,
            streaming_compliance: StreamingCompliance {
                spotify_diff_lu: integrated_lufs - (-14.0),
                youtube_diff_lu: integrated_lufs - (-14.0),
                apple_music_diff_lu: integrated_lufs - (-16.0),
                podcast_diff_lu: integrated_lufs - (-16.0),
                broadcast_diff_lu: integrated_lufs - (-23.0),
            },
        }
    }
}
