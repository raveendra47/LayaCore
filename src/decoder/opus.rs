use rusty_opus::OpusDecoder;
use symphonia::core::codecs::{CodecParameters, CODEC_TYPE_OPUS};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatReader;

use crate::decoder::{AudioDecoder, AudioMetadata};
use crate::error::AnalyzerError;

const OPUS_MAGIC: &[u8; 8] = b"OpusHead";
const OPUS_SAMPLE_RATE: u32 = 48_000;
/// Maximum decoded Opus packet duration at 48 kHz: 120 ms = 5760 samples per channel.
const MAX_OPUS_FRAME_SIZE_48K: usize = 5_760;

/// Parsed fields from an RFC 7845 Ogg Opus identification header (`OpusHead`).
#[derive(Debug, Clone, Copy)]
pub struct OpusHead {
    pub version: u8,
    pub channels: usize,
    pub pre_skip: u16,
    pub input_sample_rate: u32,
    /// Signed Q7.8 output gain in dB (divide by 256.0 for dB).
    pub output_gain_q78: i16,
    pub channel_mapping_family: u8,
}

impl OpusHead {
    /// Parse an RFC 7845 `OpusHead` header from `CodecParameters::extra_data`.
    pub fn parse(extra: &[u8]) -> Result<Self, AnalyzerError> {
        if extra.len() < 19 || &extra[..8] != OPUS_MAGIC {
            return Err(AnalyzerError::DecodeError(
                "Invalid or missing OpusHead identification header".into(),
            ));
        }

        let version = extra[8];
        // Symphonia's ISOBMFF (MP4/M4A) demuxer prepends "OpusHead" onto the raw big-endian
        // `dOps` atom whose version byte is 0. Reject it explicitly rather than misreading
        // big-endian gain/pre-skip bytes as little-endian.
        if version == 0 {
            return Err(AnalyzerError::UnsupportedFormat(
                "Opus inside MP4/M4A containers (dOps) is not currently supported. Please use Ogg Opus (.opus / .ogg).".into(),
            ));
        }
        if version > 0x0f {
            return Err(AnalyzerError::UnsupportedFormat(format!(
                "Unsupported Ogg Opus header version: {}",
                version
            )));
        }

        let channels = extra[9] as usize;
        if channels == 0 {
            return Err(AnalyzerError::DecodeError(
                "OpusHead reports 0 channels".into(),
            ));
        }

        let pre_skip = u16::from_le_bytes([extra[10], extra[11]]);
        let input_sample_rate = u32::from_le_bytes([extra[12], extra[13], extra[14], extra[15]]);
        let output_gain_q78 = i16::from_le_bytes([extra[16], extra[17]]);
        let channel_mapping_family = extra[18];

        match channel_mapping_family {
            0 if channels <= 2 => {}
            1 if channels <= 2 => {}
            1 => {
                return Err(AnalyzerError::UnsupportedFormat(format!(
                    "Multichannel surround Opus ({} channels, mapping family 1) is not currently supported; LayaCore supports Mono & Stereo Ogg Opus.",
                    channels
                )));
            }
            2 | 3 | 255 => {
                return Err(AnalyzerError::UnsupportedFormat(format!(
                    "Opus channel mapping family {} (Ambisonics/custom) is not supported.",
                    channel_mapping_family
                )));
            }
            other => {
                return Err(AnalyzerError::UnsupportedFormat(format!(
                    "Unsupported Opus channel mapping family: {}",
                    other
                )));
            }
        }

        Ok(Self {
            version,
            channels,
            pre_skip,
            input_sample_rate,
            output_gain_q78,
            channel_mapping_family,
        })
    }

    /// Linear amplitude scale factor corresponding to the RFC 7845 Q7.8 output gain.
    pub fn linear_gain(&self) -> f32 {
        if self.output_gain_q78 == 0 {
            1.0
        } else {
            10.0f32.powf((self.output_gain_q78 as f32) / (20.0 * 256.0))
        }
    }
}

/// Pure-Rust Ogg Opus decoder combining Symphonia's Ogg demuxer with `opus-decoder`.
pub struct OpusAudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: OpusDecoder,
    track_id: u32,
    metadata: AudioMetadata,
    gain_linear: f32,
    remaining_pre_skip: usize,
    max_emitted_frames: Option<u64>,
    emitted_frames: u64,
    decode_buf: Vec<f32>,
    leftover_samples: Vec<f32>,
    reached_eof: bool,
}

impl OpusAudioDecoder {
    /// Instantiate from an already-probed Symphonia `FormatReader` whose track codec is `CODEC_TYPE_OPUS`.
    pub fn from_probed(
        format: Box<dyn FormatReader>,
        track_id: u32,
        codec_params: &CodecParameters,
        file_name: String,
        file_size: u64,
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
    ) -> Result<Self, AnalyzerError> {
        if codec_params.codec != CODEC_TYPE_OPUS {
            return Err(AnalyzerError::DecodeError(
                "OpusAudioDecoder called with non-Opus track".into(),
            ));
        }

        let extra = codec_params.extra_data.as_deref().ok_or_else(|| {
            AnalyzerError::DecodeError("Opus stream missing OpusHead extra_data".into())
        })?;

        let head = OpusHead::parse(extra)?;
        let channels = head.channels;
        let gain_linear = head.linear_gain();

        let decoder = OpusDecoder::new(OPUS_SAMPLE_RATE as i32, channels).map_err(|e| {
            AnalyzerError::DecodeError(format!("Failed to initialize pure-Rust Opus decoder: {:?}", e))
        })?;

        // Symphonia's Ogg n_frames reflects final granule position (which includes pre_skip in Ogg Opus).
        let total_frames = codec_params
            .n_frames
            .map(|n| n.saturating_sub(head.pre_skip as u64));
        let duration_seconds = total_frames.map(|f| f as f64 / OPUS_SAMPLE_RATE as f64);
        let bitrate_kbps = duration_seconds.and_then(|d| {
            if d > 0.0 {
                Some((file_size as f64 * 8.0) / (d * 1000.0))
            } else {
                None
            }
        });

        let metadata = AudioMetadata {
            file_name,
            format_name: "Ogg Opus Container".to_string(),
            codec_name: "Opus (RFC 6716 / 7845)".to_string(),
            sample_rate: OPUS_SAMPLE_RATE,
            channels,
            bits_per_sample: None,
            duration_seconds,
            total_frames,
            file_size_bytes: file_size,
            bitrate_kbps,
            title,
            artist,
            album,
        };

        // 120 ms max Opus packet duration at 48 kHz = 5760 samples per channel
        let decode_buf = vec![0.0f32; MAX_OPUS_FRAME_SIZE_48K * channels];

        Ok(Self {
            format,
            decoder,
            track_id,
            metadata,
            gain_linear,
            remaining_pre_skip: head.pre_skip as usize,
            max_emitted_frames: total_frames,
            emitted_frames: 0,
            decode_buf,
            leftover_samples: Vec::new(),
            reached_eof: false,
        })
    }
}

impl AudioDecoder for OpusAudioDecoder {
    fn metadata(&self) -> &AudioMetadata {
        &self.metadata
    }

    fn read_frames(
        &mut self,
        output: &mut Vec<f32>,
        max_frames: usize,
    ) -> Result<usize, AnalyzerError> {
        output.clear();
        let channels = self.metadata.channels;
        let max_samples = max_frames * channels;

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
            if let Some(limit) = self.max_emitted_frames {
                if self.emitted_frames >= limit {
                    self.reached_eof = true;
                    break;
                }
            }

            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    self.reached_eof = true;
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    continue;
                }
                Err(_) => {
                    self.reached_eof = true;
                    break;
                }
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let raw = packet.buf();
            if raw.is_empty() {
                continue;
            }

            let decoded_frames = match self
                .decoder
                .decode(raw, MAX_OPUS_FRAME_SIZE_48K, &mut self.decode_buf)
            {
                Ok(n) => n,
                Err(_) => {
                    // Recoverable corrupted packet; skip cleanly
                    continue;
                }
            };

            if decoded_frames == 0 {
                continue;
            }

            // 1. Apply RFC 7845 OpusHead pre_skip leading-sample trim
            let mut start_frame = 0usize;
            let mut end_frame = decoded_frames;

            if self.remaining_pre_skip > 0 {
                let skip = self.remaining_pre_skip.min(end_frame);
                start_frame += skip;
                self.remaining_pre_skip -= skip;
            }

            // 2. Apply trailing trim: prefer exact (final_granule_pos - pre_skip) limit when known,
            //    falling back to Symphonia's packet.trim_end() for unseekable streams.
            if let Some(limit) = self.max_emitted_frames {
                let remaining_allowed = limit.saturating_sub(self.emitted_frames) as usize;
                let avail = end_frame.saturating_sub(start_frame);
                if avail > remaining_allowed {
                    end_frame = start_frame + remaining_allowed;
                }
            } else {
                let pkt_trim_end = (packet.trim_end() as usize).min(end_frame.saturating_sub(start_frame));
                end_frame -= pkt_trim_end;
            }

            if start_frame >= end_frame {
                continue;
            }

            self.emitted_frames += (end_frame - start_frame) as u64;

            let sample_start = start_frame * channels;
            let sample_end = end_frame * channels;

            // 3. Apply RFC 7845 OpusHead Q7.8 output gain (never tag/ReplayGain gains)
            if (self.gain_linear - 1.0).abs() > 1e-6 {
                for s in &mut self.decode_buf[sample_start..sample_end] {
                    *s *= self.gain_linear;
                }
            }

            let samples = &self.decode_buf[sample_start..sample_end];
            let needed = max_samples - output.len();
            if samples.len() <= needed {
                output.extend_from_slice(samples);
            } else {
                output.extend_from_slice(&samples[..needed]);
                self.leftover_samples.extend_from_slice(&samples[needed..]);
                break;
            }
        }

        Ok(output.len() / channels)
    }
}
