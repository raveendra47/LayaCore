use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use crate::decoder::{AudioDecoder, AudioMetadata};
use crate::error::AnalyzerError;

pub struct AuDecoder {
    reader: BufReader<File>,
    metadata: AudioMetadata,
    encoding: u32,
    _data_offset: u64,
    bytes_remaining: Option<u64>,
}

impl AuDecoder {
    pub fn open(path: &Path) -> Result<Self, AnalyzerError> {
        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let mut reader = BufReader::new(file);

        let mut header = [0u8; 24];
        reader.read_exact(&mut header).map_err(|e| {
            AnalyzerError::DecodeError(format!("Failed to read AU header: {}", e))
        })?;

        // Magic number: 0x2e736e64 (".snd")
        let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if magic != 0x2e736e64 {
            return Err(AnalyzerError::UnsupportedFormat(format!(
                "Invalid AU magic: 0x{:08x}", magic
            )));
        }

        let data_offset = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as u64;
        let data_size = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
        let encoding = u32::from_be_bytes([header[12], header[13], header[14], header[15]]);
        let sample_rate = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
        let channels = u32::from_be_bytes([header[20], header[21], header[22], header[23]]) as usize;

        if channels == 0 || sample_rate == 0 {
            return Err(AnalyzerError::InvalidData("Invalid sample rate or channel count".into()));
        }

        let (encoding_name, bits_per_sample, bytes_per_sample) = match encoding {
            1 => ("8-bit ISDN μ-law", Some(8), 1),
            2 => ("8-bit linear PCM", Some(8), 1),
            3 => ("16-bit linear PCM", Some(16), 2),
            4 => ("24-bit linear PCM", Some(24), 3),
            5 => ("32-bit linear PCM", Some(32), 4),
            6 => ("32-bit IEEE float", Some(32), 4),
            7 => ("64-bit IEEE float", Some(64), 8),
            27 => ("8-bit A-law", Some(8), 1),
            other => {
                return Err(AnalyzerError::UnsupportedFormat(format!(
                    "Unsupported AU encoding format id: {}", other
                )));
            }
        };

        let bytes_data = if data_size == 0xffffffff || data_size == 0 {
            if file_size > data_offset {
                Some(file_size - data_offset)
            } else {
                None
            }
        } else {
            Some(data_size as u64)
        };

        let total_frames = bytes_data.map(|b| b / (channels as u64 * bytes_per_sample as u64));
        let duration_seconds = total_frames.map(|f| f as f64 / sample_rate as f64);
        let bitrate_kbps = duration_seconds.map(|d| {
            if d > 0.0 {
                (file_size as f64 * 8.0) / (d * 1000.0)
            } else {
                0.0
            }
        });

        // Seek to data start
        reader.seek(SeekFrom::Start(data_offset))?;

        let file_name = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown.au")
            .to_string();

        let metadata = AudioMetadata {
            file_name,
            format_name: "Sun/NeXT AU".to_string(),
            codec_name: encoding_name.to_string(),
            sample_rate,
            channels,
            bits_per_sample,
            duration_seconds,
            total_frames,
            file_size_bytes: file_size,
            bitrate_kbps,
            title: None,
            artist: None,
            album: None,
        };

        Ok(Self {
            reader,
            metadata,
            encoding,
            _data_offset: data_offset,
            bytes_remaining: bytes_data,
        })
    }
}

impl AudioDecoder for AuDecoder {
    fn metadata(&self) -> &AudioMetadata {
        &self.metadata
    }

    fn read_frames(&mut self, output: &mut Vec<f32>, max_frames: usize) -> Result<usize, AnalyzerError> {
        output.clear();
        let channels = self.metadata.channels;

        let bytes_per_sample = match self.encoding {
            1 | 2 | 27 => 1,
            3 => 2,
            4 => 3,
            5 | 6 => 4,
            7 => 8,
            _ => return Ok(0),
        };

        let bytes_per_frame = channels * bytes_per_sample;
        let mut frames_read = 0;
        let mut frame_buf = vec![0u8; bytes_per_frame];

        while frames_read < max_frames {
            if let Some(remaining) = self.bytes_remaining {
                if remaining < bytes_per_frame as u64 {
                    break;
                }
            }

            match self.reader.read_exact(&mut frame_buf) {
                Ok(()) => {
                    if let Some(rem) = &mut self.bytes_remaining {
                        *rem -= bytes_per_frame as u64;
                    }

                    // Decode each channel in this frame
                    for ch in 0..channels {
                        let sample_bytes = &frame_buf[ch * bytes_per_sample..(ch + 1) * bytes_per_sample];
                        let sample_f32 = match self.encoding {
                            1 => ulaw_to_f32(sample_bytes[0]),
                            2 => (sample_bytes[0] as i8 as f32) / 128.0,
                            3 => {
                                let val = i16::from_be_bytes([sample_bytes[0], sample_bytes[1]]);
                                val as f32 / 32768.0
                            }
                            4 => {
                                let val = ((sample_bytes[0] as i32) << 24
                                    | (sample_bytes[1] as i32) << 16
                                    | (sample_bytes[2] as i32) << 8) >> 8;
                                val as f32 / 8388608.0
                            }
                            5 => {
                                let val = i32::from_be_bytes([
                                    sample_bytes[0], sample_bytes[1], sample_bytes[2], sample_bytes[3],
                                ]);
                                val as f32 / 2147483648.0
                            }
                            6 => f32::from_be_bytes([
                                sample_bytes[0], sample_bytes[1], sample_bytes[2], sample_bytes[3],
                            ]),
                            7 => {
                                let val = f64::from_be_bytes([
                                    sample_bytes[0], sample_bytes[1], sample_bytes[2], sample_bytes[3],
                                    sample_bytes[4], sample_bytes[5], sample_bytes[6], sample_bytes[7],
                                ]);
                                val as f32
                            }
                            27 => alaw_to_f32(sample_bytes[0]),
                            _ => 0.0,
                        };
                        output.push(sample_f32);
                    }
                    frames_read += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(AnalyzerError::IoError(e)),
            }
        }

        Ok(frames_read)
    }
}

/// Decode G.711 μ-law byte to normalized f32 [-1.0, 1.0]
fn ulaw_to_f32(u: u8) -> f32 {
    let u = !u;
    let sign = (u & 0x80) != 0;
    let exponent = ((u >> 4) & 0x07) as u32;
    let mantissa = (u & 0x0F) as u32;
    let mut sample = ((mantissa << 3) + 0x84) << exponent;
    sample -= 0x84;
    let s16 = if sign { -(sample as i32) } else { sample as i32 };
    (s16 as f32) / 32768.0
}

/// Decode G.711 A-law byte to normalized f32 [-1.0, 1.0]
fn alaw_to_f32(a: u8) -> f32 {
    let a = a ^ 0x55;
    let sign = (a & 0x80) != 0;
    let exponent = ((a >> 4) & 0x07) as u32;
    let mantissa = (a & 0x0F) as u32;
    let sample = if exponent == 0 {
        (mantissa << 4) + 8
    } else {
        ((mantissa << 4) + 0x108) << (exponent - 1)
    };
    let s16 = if sign { -(sample as i32) } else { sample as i32 };
    (s16 as f32) / 32768.0
}
