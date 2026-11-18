pub mod au;
pub mod opus;
pub mod symphonia;

use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::error::AnalyzerError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMetadata {
    pub file_name: String,
    pub format_name: String,
    pub codec_name: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub bits_per_sample: Option<u32>,
    pub duration_seconds: Option<f64>,
    pub total_frames: Option<u64>,
    pub file_size_bytes: u64,
    pub bitrate_kbps: Option<f64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl AudioMetadata {
    /// Returns true if this audio stream originates from or is wrapped in a FLAC container/codec.
    pub fn is_flac(&self) -> bool {
        self.file_name.to_lowercase().ends_with(".flac")
            || self.format_name.to_lowercase().contains("flac")
            || self.codec_name.to_lowercase().contains("flac")
    }
}

pub trait AudioDecoder {
    fn metadata(&self) -> &AudioMetadata;
    /// Reads up to max_frames into `output`.
    /// `output` will contain interleaved samples: [L0, R0, L1, R1, ...] (for stereo).
    /// Returns the number of frames (time steps) read. 0 means EOF.
    fn read_frames(&mut self, output: &mut Vec<f32>, max_frames: usize) -> Result<usize, AnalyzerError>;
}

/// Open an audio file and return an appropriate decoder.
pub fn open_audio_file(path: &Path) -> Result<Box<dyn AudioDecoder>, AnalyzerError> {
    if !path.exists() {
        return Err(AnalyzerError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File not found: {}", path.display()),
        )));
    }

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // Gracefully report informative errors for known unsupported formats
    let unsupported_label = match extension.as_str() {
        "wma" => Some("Windows Media Audio (.wma)"),
        "ac3" | "eac3" => Some("Dolby Digital (.ac3 / .eac3)"),
        "dsf" | "dff" => Some("Direct Stream Digital / DSD (.dsf / .dff)"),
        "ra" | "rm" => Some("RealAudio (.ra / .rm)"),
        "wv" => Some("WavPack (.wv)"),
        "ape" => Some("Monkey's Audio (.ape)"),
        _ => None,
    };
    if let Some(label) = unsupported_label {
        return Err(AnalyzerError::UnsupportedFormat(format!(
            "{} is not currently supported by LayaCore. Supported formats: FLAC, WAV, MP3, M4A/AAC, ALAC, OGG Vorbis, Ogg Opus (.opus), AIFF, CAF, AU/SND.",
            label
        )));
    }

    // Check if it's Sun AU / SND format
    if extension == "au" || extension == "snd" {
        match au::AuDecoder::open(path) {
            Ok(decoder) => return Ok(Box::new(decoder)),
            Err(e) => {
                // If AU parser failed, fall through to Symphonia as fallback
                eprintln!("AU parser skipped ({}). Trying generic demuxer...", e);
            }
        }
    }

    // Symphonia universal demuxer + decoder (with pure-Rust Opus delegation for Ogg Opus)
    symphonia::open_symphonia_or_opus(path)
}
