use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::decoder::{AudioDecoder, AudioMetadata};
use crate::error::AnalyzerError;

pub struct SymphoniaDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    metadata: AudioMetadata,
    sample_buf: Option<SampleBuffer<f32>>,
    leftover_samples: Vec<f32>,
    reached_eof: bool,
}

impl SymphoniaDecoder {
    pub fn open(path: &Path) -> Result<Self, AnalyzerError> {
        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| AnalyzerError::DecodeError(format!("Failed to probe audio container: {}", e)))?;

        let mut format = probed.format;

        // Find the first default/supported audio track
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| AnalyzerError::UnsupportedFormat("No supported audio tracks found in file".into()))?;

        let track_id = track.id;
        let codec_params = track.codec_params.clone();
        let sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
        let bits_per_sample = codec_params.bits_per_sample;
        let total_frames = codec_params.n_frames;

        let duration_seconds = total_frames.map(|f| f as f64 / sample_rate as f64);
        let bitrate_kbps = duration_seconds.map(|d| {
            if d > 0.0 {
                (file_size as f64 * 8.0) / (d * 1000.0)
            } else {
                0.0
            }
        });

        // Extract metadata tags if present
        let mut title = None;
        let mut artist = None;
        let mut album = None;

        if let Some(metadata_rev) = format.metadata().current() {
            for tag in metadata_rev.tags() {
                match tag.std_key {
                    Some(symphonia::core::meta::StandardTagKey::TrackTitle) => {
                        title = Some(tag.value.to_string());
                    }
                    Some(symphonia::core::meta::StandardTagKey::Artist) => {
                        artist = Some(tag.value.to_string());
                    }
                    Some(symphonia::core::meta::StandardTagKey::Album) => {
                        album = Some(tag.value.to_string());
                    }
                    _ => {}
                }
            }
        }

        let decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| AnalyzerError::DecodeError(format!("Failed to instantiate audio codec decoder: {}", e)))?;

        let file_name = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown")
            .to_string();

        let codec_name = match codec_params.codec {
            symphonia::core::codecs::CODEC_TYPE_FLAC => "FLAC (Free Lossless Audio Codec)".to_string(),
            symphonia::core::codecs::CODEC_TYPE_MP3 => "MP3 (MPEG Audio Layer III)".to_string(),
            symphonia::core::codecs::CODEC_TYPE_AAC => "AAC (Advanced Audio Coding)".to_string(),
            symphonia::core::codecs::CODEC_TYPE_VORBIS => "Ogg Vorbis".to_string(),
            symphonia::core::codecs::CODEC_TYPE_ALAC => "ALAC (Apple Lossless)".to_string(),
            c if c == symphonia::core::codecs::CODEC_TYPE_PCM_S16LE
                || c == symphonia::core::codecs::CODEC_TYPE_PCM_S16BE
                || c == symphonia::core::codecs::CODEC_TYPE_PCM_S24LE
                || c == symphonia::core::codecs::CODEC_TYPE_PCM_S24BE
                || c == symphonia::core::codecs::CODEC_TYPE_PCM_S32LE
                || c == symphonia::core::codecs::CODEC_TYPE_PCM_F32LE => "Linear PCM".to_string(),
            other => format!("{:?}", other),
        };

        let format_name = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| match ext.to_lowercase().as_str() {
                "flac" => "FLAC Container",
                "wav" | "wave" => "WAV / RIFF",
                "mp3" => "MPEG Audio Container",
                "m4a" | "mp4" => "MPEG-4 Audio (M4A)",
                "ogg" | "oga" => "Ogg Multimedia Container",
                "aiff" | "aif" => "Apple AIFF",
                "caf" => "Apple Core Audio Format",
                _ => "Universal Audio Container",
            })
            .unwrap_or("Universal Audio Container")
            .to_string();

        let metadata = AudioMetadata {
            file_name,
            format_name,
            codec_name,
            sample_rate,
            channels,
            bits_per_sample,
            duration_seconds,
            total_frames,
            file_size_bytes: file_size,
            bitrate_kbps,
            title,
            artist,
            album,
        };

        Ok(Self {
            format,
            decoder,
            track_id,
            metadata,
            sample_buf: None,
            leftover_samples: Vec::new(),
            reached_eof: false,
        })
    }
}

impl AudioDecoder for SymphoniaDecoder {
    fn metadata(&self) -> &AudioMetadata {
        &self.metadata
    }

    fn read_frames(&mut self, output: &mut Vec<f32>, max_frames: usize) -> Result<usize, AnalyzerError> {
        output.clear();
        let channels = self.metadata.channels;
        let max_samples = max_frames * channels;

        // Drain leftovers first
        if !self.leftover_samples.is_empty() {
            let to_take = self.leftover_samples.len().min(max_samples);
            output.extend(self.leftover_samples.drain(..to_take));
            if output.len() == max_samples {
                return Ok(output.len() / channels);
            }
        }

        if self.reached_eof {
            return Ok(output.len() / channels);
        }

        while output.len() < max_samples {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.reached_eof = true;
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(_e) => {
                    // Treat decoding error or EOF cleanly
                    self.reached_eof = true;
                    break;
                }
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(audio_buf) => {
                    if self.sample_buf.is_none() {
                        let spec = *audio_buf.spec();
                        let duration = audio_buf.capacity() as u64;
                        self.sample_buf = Some(SampleBuffer::new(duration, spec));
                    }

                    if let Some(sample_buf) = &mut self.sample_buf {
                        sample_buf.copy_interleaved_ref(audio_buf);
                        let samples = sample_buf.samples();

                        let needed = max_samples - output.len();
                        if samples.len() <= needed {
                            output.extend_from_slice(samples);
                        } else {
                            output.extend_from_slice(&samples[..needed]);
                            self.leftover_samples.extend_from_slice(&samples[needed..]);
                            break;
                        }
                    }
                }
                Err(SymphoniaError::DecodeError(_e)) => {
                    // Recoverable packet decode error; continue to next packet
                    continue;
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                }
                Err(_) => {
                    self.reached_eof = true;
                    break;
                }
            }
        }

        Ok(output.len() / channels)
    }
}
