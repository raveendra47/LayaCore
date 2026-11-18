use std::f32::consts::PI;
use std::sync::Arc;
use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;

pub struct FftEngine {
    pub fft_size: usize,
    r2c: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    input_buffer: Vec<f32>,
    output_buffer: Vec<Complex<f32>>,
}

impl FftEngine {
    pub fn new(fft_size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(fft_size);
        let input_buffer = r2c.make_input_vec();
        let output_buffer = r2c.make_output_vec();

        // Hann window: w[n] = 0.5 * (1 - cos(2*pi*n / (N-1)))
        let mut window = Vec::with_capacity(fft_size);
        let n_minus_1 = (fft_size - 1) as f32;
        for i in 0..fft_size {
            let w = 0.5 * (1.0 - (2.0 * PI * i as f32 / n_minus_1).cos());
            window.push(w);
        }

        Self {
            fft_size,
            r2c,
            window,
            input_buffer,
            output_buffer,
        }
    }

    /// Process a time-domain window of samples (must have length == fft_size).
    /// Returns reference to output complex spectrum (length = fft_size / 2 + 1).
    pub fn process_frame(&mut self, samples: &[f32]) -> &[Complex<f32>] {
        assert_eq!(samples.len(), self.fft_size);

        // Apply Hann window
        for (i, (&s, &w)) in samples.iter().zip(&self.window).enumerate() {
            self.input_buffer[i] = s * w;
        }

        self.r2c
            .process(&mut self.input_buffer, &mut self.output_buffer)
            .expect("FFT forward transformation failed");

        &self.output_buffer
    }
}
