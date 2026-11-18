pub mod advisor;
pub mod batch;
pub mod decoder;
pub mod error;
pub mod meters;
pub mod report;
pub mod spectral;

use std::path::Path;
use serde::{Deserialize, Serialize};

pub use crate::advisor::{
    generate_mastering_advice, AdviceSeverity, MasteringAdvice, MasteringAdvisorReport,
};
pub use crate::batch::{
    discover_audio_files, is_supported_audio_file, scan_directory, scan_directory_simple,
    AlbumSummary, AlertSeverity, BatchAlert, BatchScanOptions, BatchScanReport, TrackScanResult,
    SUPPORTED_AUDIO_EXTENSIONS,
};
pub use crate::decoder::AudioMetadata;
pub use crate::meters::{
    LufsStats, StereoStats, TimeDomainStats, TruePeakStats, WaveformData, WaveformBucket,
};
pub use crate::spectral::{
    BandEnergy, FrequencyBalance, LosslessVerdict, MusicalStats, QualityReport, SpectralStats,
};

use crate::decoder::open_audio_file;
use crate::error::AnalyzerError;
use crate::meters::{LufsMeter, StereoMeter, TimeDomainMeter, TruePeakMeter, WaveformGenerator};
use crate::spectral::SpectralMeter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioAnalysis {
    pub metadata: AudioMetadata,
    pub time_domain: TimeDomainStats,
    pub stereo: StereoStats,
    pub lufs: LufsStats,
    pub true_peak: TruePeakStats,
    pub waveform: WaveformData,
    pub spectral: SpectralStats,
    pub musical: MusicalStats,
    pub quality: QualityReport,
    pub advisor: MasteringAdvisorReport,
}

#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub waveform_buckets_per_second: usize,
    pub block_size_frames: usize,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            waveform_buckets_per_second: 50,
            block_size_frames: 4096,
        }
    }
}

/// Analyze an audio file without progress callback
pub fn analyze_file_simple<P: AsRef<Path>>(
    path: P,
    options: &AnalysisOptions,
) -> Result<AudioAnalysis, AnalyzerError> {
    analyze_file(path, options, None::<fn(u64, Option<u64>)>)
}

/// Analyze an audio file through a single-pass streaming pipeline
pub fn analyze_file<P, F>(
    path: P,
    options: &AnalysisOptions,
    mut progress_callback: Option<F>,
) -> Result<AudioAnalysis, AnalyzerError>
where
    P: AsRef<Path>,
    F: FnMut(u64, Option<u64>), // (frames_processed, total_frames_hint)
{
    let mut decoder = open_audio_file(path.as_ref())?;
    let metadata = decoder.metadata().clone();

    let channels = metadata.channels;
    let sample_rate = metadata.sample_rate;
    let total_frames_hint = metadata.total_frames;

    let mut time_meter = TimeDomainMeter::new(channels, sample_rate);
    let mut stereo_meter = StereoMeter::new(channels);
    let mut waveform_gen = WaveformGenerator::new(channels, sample_rate, options.waveform_buckets_per_second);
    let mut lufs_meter = LufsMeter::new(channels, sample_rate);
    let mut true_peak_meter = TruePeakMeter::new(channels);
    let mut spectral_meter = SpectralMeter::new(channels, sample_rate);

    let mut sample_buffer = Vec::with_capacity(options.block_size_frames * channels);
    let mut total_frames_read = 0u64;

    loop {
        let frames_read = decoder.read_frames(&mut sample_buffer, options.block_size_frames)?;
        if frames_read == 0 {
            break;
        }

        total_frames_read += frames_read as u64;

        // Process this block through all meters in a single streaming pass
        time_meter.process_frames(&sample_buffer);
        stereo_meter.process_frames(&sample_buffer, channels);
        waveform_gen.process_frames(&sample_buffer);
        lufs_meter.process_frames(&sample_buffer);
        true_peak_meter.process_frames(&sample_buffer);
        spectral_meter.process_frames(&sample_buffer);

        if let Some(cb) = &mut progress_callback {
            cb(total_frames_read, total_frames_hint);
        }
    }

    let format_lower = metadata.format_name.to_lowercase();
    let codec_lower = metadata.codec_name.to_lowercase();
    let is_native_lossy = format_lower.contains("mp3")
        || format_lower.contains("aac")
        || format_lower.contains("ogg")
        || format_lower.contains("vorbis")
        || format_lower.contains("opus")
        || codec_lower.contains("mp3")
        || codec_lower.contains("aac")
        || codec_lower.contains("vorbis")
        || codec_lower.contains("opus");

    let (spectral_stats, musical_stats, quality_report) = spectral_meter.finish(is_native_lossy);

    let mut final_metadata = metadata;
    // If duration was unknown in header, compute it accurately from total frames decoded
    if final_metadata.total_frames.is_none() || final_metadata.total_frames == Some(0) {
        final_metadata.total_frames = Some(total_frames_read);
        final_metadata.duration_seconds = Some(total_frames_read as f64 / sample_rate as f64);
        if let Some(d) = final_metadata.duration_seconds {
            if d > 0.0 {
                final_metadata.bitrate_kbps = Some((final_metadata.file_size_bytes as f64 * 8.0) / (d * 1000.0));
            }
        }
    }

    let time_domain = time_meter.finish();
    let stereo = stereo_meter.finish();
    let lufs = lufs_meter.finish();
    let true_peak = true_peak_meter.finish();
    let waveform = waveform_gen.finish();
    let advisor = generate_mastering_advice(
        &final_metadata,
        &time_domain,
        &stereo,
        &lufs,
        &true_peak,
        &spectral_stats,
        &quality_report,
    );

    Ok(AudioAnalysis {
        metadata: final_metadata,
        time_domain,
        stereo,
        lufs,
        true_peak,
        waveform,
        spectral: spectral_stats,
        musical: musical_stats,
        quality: quality_report,
        advisor,
    })
}
