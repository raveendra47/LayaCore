pub mod features;
pub mod fft;
pub mod rhythm;
pub mod transcode;

use features::SpectralFeatureExtractor;
use fft::FftEngine;
use rhythm::RhythmAndKeyAnalyzer;
use transcode::TranscodeDetector;

pub use features::{BandEnergy, FrequencyBalance, SpectralStats, ISO_31_BANDS};
pub use rhythm::{MusicalStats, NOTE_NAMES};
pub use transcode::{LosslessVerdict, QualityReport};

pub struct SpectralMeter {
    channels: usize,
    fft_engine: FftEngine,
    feature_extractor: SpectralFeatureExtractor,
    rhythm_analyzer: RhythmAndKeyAnalyzer,
    transcode_detector: TranscodeDetector,
    
    // Sliding sample buffer
    fft_size: usize,
    hop_size: usize,
    sample_ring: Vec<f32>,
    samples_in_ring: usize,
    
    // Work buffers
    window_buf: Vec<f32>,
    mag_buf: Vec<f32>,
    pwr_buf: Vec<f32>,
}

impl SpectralMeter {
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        let fft_size = 2048;
        let hop_size = 1024;
        let num_bins = fft_size / 2 + 1;

        Self {
            channels,
            fft_engine: FftEngine::new(fft_size),
            feature_extractor: SpectralFeatureExtractor::new(sample_rate, fft_size),
            rhythm_analyzer: RhythmAndKeyAnalyzer::new(sample_rate, fft_size, hop_size),
            transcode_detector: TranscodeDetector::new(sample_rate, fft_size),
            fft_size,
            hop_size,
            sample_ring: Vec::with_capacity(fft_size + hop_size * 4),
            samples_in_ring: 0,
            window_buf: vec![0.0f32; fft_size],
            mag_buf: vec![0.0f32; num_bins],
            pwr_buf: vec![0.0f32; num_bins],
        }
    }

    /// Process interleaved frames [L0, R0, L1, R1, ...]
    pub fn process_frames(&mut self, frames: &[f32]) {
        if frames.is_empty() || self.channels == 0 {
            return;
        }

        let num_frames = frames.len() / self.channels;
        let inv_channels = 1.0f32 / self.channels as f32;

        for i in 0..num_frames {
            let offset = i * self.channels;
            let mut mono = 0.0f32;
            for ch in 0..self.channels {
                mono += frames[offset + ch];
            }
            mono *= inv_channels;

            self.sample_ring.push(mono);
            self.samples_in_ring += 1;

            if self.samples_in_ring >= self.fft_size {
                // Extract fft_size window from ring buffer
                let start_idx = self.sample_ring.len() - self.samples_in_ring;
                self.window_buf.copy_from_slice(&self.sample_ring[start_idx..start_idx + self.fft_size]);

                // Run FFT
                let complex_spectrum = self.fft_engine.process_frame(&self.window_buf);

                // Compute magnitude and power spectra
                for (k, c) in complex_spectrum.iter().enumerate() {
                    let norm_sqr = c.norm_sqr();
                    self.pwr_buf[k] = norm_sqr;
                    self.mag_buf[k] = norm_sqr.sqrt();
                }

                // Update feature accumulators
                self.feature_extractor.process_frame(&self.pwr_buf, &self.mag_buf);
                self.rhythm_analyzer.process_frame(&self.mag_buf, &self.pwr_buf);
                self.transcode_detector.process_frame(&self.pwr_buf);

                // Advance by hop_size
                self.samples_in_ring -= self.hop_size;

                // Keep ring buffer size reasonable
                if self.sample_ring.len() > self.fft_size * 4 {
                    let drain_amount = self.sample_ring.len() - self.samples_in_ring;
                    self.sample_ring.drain(..drain_amount);
                }
            }
        }
    }

    pub fn finish(self, is_native_lossy: bool) -> (SpectralStats, MusicalStats, QualityReport) {
        let spectral_stats = self.feature_extractor.finish();
        let musical_stats = self.rhythm_analyzer.finish();
        let quality_report = self.transcode_detector.finish(is_native_lossy);
        (spectral_stats, musical_stats, quality_report)
    }
}
