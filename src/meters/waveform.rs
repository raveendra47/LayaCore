use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformBucket {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformData {
    pub buckets_per_second: usize,
    pub total_buckets: usize,
    pub buckets: Vec<WaveformBucket>,
}

pub struct WaveformGenerator {
    channels: usize,
    samples_per_bucket: usize,
    buckets_per_second: usize,
    current_min: f32,
    current_max: f32,
    current_sum_sq: f64,
    current_count: usize,
    buckets: Vec<WaveformBucket>,
}

impl WaveformGenerator {
    pub fn new(channels: usize, sample_rate: u32, buckets_per_second: usize) -> Self {
        let bps = buckets_per_second.max(1);
        let samples_per_bucket = (sample_rate as usize / bps).max(1);
        Self {
            channels,
            samples_per_bucket,
            buckets_per_second: bps,
            current_min: 0.0,
            current_max: 0.0,
            current_sum_sq: 0.0,
            current_count: 0,
            buckets: Vec::new(),
        }
    }

    pub fn process_frames(&mut self, frames: &[f32]) {
        if frames.is_empty() || self.channels == 0 {
            return;
        }

        let num_frames = frames.len() / self.channels;
        for i in 0..num_frames {
            let offset = i * self.channels;
            // Mix down to mono for envelope
            let mut mono_sample = 0.0f32;
            for ch in 0..self.channels {
                mono_sample += frames[offset + ch];
            }
            mono_sample /= self.channels as f32;

            if mono_sample < self.current_min {
                self.current_min = mono_sample;
            }
            if mono_sample > self.current_max {
                self.current_max = mono_sample;
            }
            self.current_sum_sq += (mono_sample as f64) * (mono_sample as f64);
            self.current_count += 1;

            if self.current_count >= self.samples_per_bucket {
                let rms = (self.current_sum_sq / self.current_count as f64).sqrt() as f32;
                self.buckets.push(WaveformBucket {
                    min: self.current_min,
                    max: self.current_max,
                    rms,
                });
                self.current_min = 0.0;
                self.current_max = 0.0;
                self.current_sum_sq = 0.0;
                self.current_count = 0;
            }
        }
    }

    pub fn finish(mut self) -> WaveformData {
        if self.current_count > 0 {
            let rms = (self.current_sum_sq / self.current_count as f64).sqrt() as f32;
            self.buckets.push(WaveformBucket {
                min: self.current_min,
                max: self.current_max,
                rms,
            });
        }

        WaveformData {
            buckets_per_second: self.buckets_per_second,
            total_buckets: self.buckets.len(),
            buckets: self.buckets,
        }
    }
}

/// Render a compact ASCII/Unicode waveform graph for terminal display
pub fn render_ascii_waveform(buckets: &[WaveformBucket], width: usize) -> String {
    if buckets.is_empty() || width == 0 {
        return String::new();
    }

    let block_chars = [' ', ' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bucket_chunk_size = (buckets.len() as f64 / width as f64).max(1.0);

    let mut result = String::with_capacity(width);
    for col in 0..width {
        let start_idx = (col as f64 * bucket_chunk_size) as usize;
        let end_idx = (((col + 1) as f64 * bucket_chunk_size) as usize).min(buckets.len());

        if start_idx >= buckets.len() {
            break;
        }

        let mut max_peak = 0.0f32;
        for b in &buckets[start_idx..end_idx.max(start_idx + 1)] {
            let peak = b.max.abs().max(b.min.abs());
            if peak > max_peak {
                max_peak = peak;
            }
        }

        let char_idx = ((max_peak * (block_chars.len() - 1) as f32).round() as usize)
            .min(block_chars.len() - 1);
        result.push(block_chars[char_idx]);
    }

    result
}
