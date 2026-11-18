use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use layacore::decoder::opus::OpusHead;
use layacore::decoder::open_audio_file;
use layacore::error::AnalyzerError;
use layacore::{analyze_file_simple, AnalysisOptions, LosslessVerdict};

/// Standard Ogg CRC-32 (polynomial 0x04C11DB7, MSB-first, init 0, no reflection/xorout).
fn ogg_crc32(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            if (crc & 0x8000_0000) != 0 {
                crc = (crc << 1) ^ 0x04C1_1DB7;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Build a single valid Ogg page containing one or more packets (each packet < 255 bytes).
fn build_ogg_page(
    header_type: u8,
    granule_pos: u64,
    serial: u32,
    page_seq: u32,
    packets: &[&[u8]],
) -> Vec<u8> {
    let mut page = Vec::new();
    page.extend_from_slice(b"OggS");
    page.push(0); // stream_structure_version
    page.push(header_type);
    page.extend_from_slice(&granule_pos.to_le_bytes());
    page.extend_from_slice(&serial.to_le_bytes());
    page.extend_from_slice(&page_seq.to_le_bytes());
    page.extend_from_slice(&0u32.to_le_bytes()); // CRC placeholder (offset 22..26)

    // Segment table (each packet assumed < 255 bytes for synthetic test)
    page.push(packets.len() as u8);
    for pkt in packets {
        assert!(pkt.len() < 255);
        page.push(pkt.len() as u8);
    }

    for pkt in packets {
        page.extend_from_slice(pkt);
    }

    let crc = ogg_crc32(&page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

/// Create a spec-compliant RFC 7845 Ogg Opus file with configurable pre_skip and Q7.8 output gain.
fn write_synthetic_ogg_opus(
    path: &Path,
    channels: u8,
    pre_skip: u16,
    output_gain_q78: i16,
    num_20ms_packets: usize,
    final_granule_pos: u64,
    packet_payload: &[u8],
) {
    let serial = 0x1A2B_3C4Du32;

    // 1. OpusHead (19 bytes)
    let mut opus_head = Vec::with_capacity(19);
    opus_head.extend_from_slice(b"OpusHead");
    opus_head.push(1); // version 1 (Ogg Opus)
    opus_head.push(channels);
    opus_head.extend_from_slice(&pre_skip.to_le_bytes());
    opus_head.extend_from_slice(&48_000u32.to_le_bytes());
    opus_head.extend_from_slice(&output_gain_q78.to_le_bytes());
    opus_head.push(0); // mapping family 0

    // 2. OpusTags
    let vendor = b"LayaCore-Test";
    let tag_title = b"TITLE=Opus Test";
    let tag_artist = b"ARTIST=LayaCore";
    let mut opus_tags = Vec::new();
    opus_tags.extend_from_slice(b"OpusTags");
    opus_tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    opus_tags.extend_from_slice(vendor);
    opus_tags.extend_from_slice(&2u32.to_le_bytes());
    opus_tags.extend_from_slice(&(tag_title.len() as u32).to_le_bytes());
    opus_tags.extend_from_slice(tag_title);
    opus_tags.extend_from_slice(&(tag_artist.len() as u32).to_le_bytes());
    opus_tags.extend_from_slice(tag_artist);

    let mut f = File::create(path).expect("create opus file");
    f.write_all(&build_ogg_page(0x02, 0, serial, 0, &[&opus_head]))
        .unwrap();
    f.write_all(&build_ogg_page(0x00, 0, serial, 1, &[&opus_tags]))
        .unwrap();

    if num_20ms_packets >= 2 {
        let mid = num_20ms_packets / 2;
        let p1: Vec<&[u8]> = vec![packet_payload; mid];
        let p2: Vec<&[u8]> = vec![packet_payload; num_20ms_packets - mid];
        let mid_granule = (mid as u64) * 960;
        f.write_all(&build_ogg_page(0x00, mid_granule, serial, 2, &p1))
            .unwrap();
        f.write_all(&build_ogg_page(0x04, final_granule_pos, serial, 3, &p2))
            .unwrap();
    } else {
        let pkts: Vec<&[u8]> = vec![packet_payload; num_20ms_packets];
        f.write_all(&build_ogg_page(0x04, final_granule_pos, serial, 2, &pkts))
            .unwrap();
    }
}

#[test]
fn test_opus_head_parsing_and_validation() {
    // Valid Ogg OpusHead with +6.0 dB output gain (+1536 Q7.8) and pre_skip = 312
    let mut head = Vec::new();
    head.extend_from_slice(b"OpusHead");
    head.push(1);
    head.push(2);
    head.extend_from_slice(&312u16.to_le_bytes());
    head.extend_from_slice(&48_000u32.to_le_bytes());
    head.extend_from_slice(&(6i16 * 256).to_le_bytes());
    head.push(0);

    let parsed = OpusHead::parse(&head).expect("valid Ogg OpusHead");
    assert_eq!(parsed.channels, 2);
    assert_eq!(parsed.pre_skip, 312);
    assert_eq!(parsed.output_gain_q78, 1536);
    let gain = parsed.linear_gain();
    assert!((gain - 1.99526).abs() < 0.002, "Expected ~2.0x linear gain, got {}", gain);

    // Reject MP4 dOps (version 0 big-endian)
    let mut mp4_dops = head.clone();
    mp4_dops[8] = 0;
    assert!(matches!(
        OpusHead::parse(&mp4_dops),
        Err(AnalyzerError::UnsupportedFormat(_))
    ));

    // Reject 5.1 surround (mapping family 1, 6 channels)
    let mut surround = head.clone();
    surround[9] = 6;
    surround[18] = 1;
    assert!(matches!(
        OpusHead::parse(&surround),
        Err(AnalyzerError::UnsupportedFormat(_))
    ));

    // Reject mapping families 2, 3, 255
    for fam in [2u8, 3, 255] {
        let mut bad_fam = head.clone();
        bad_fam[18] = fam;
        assert!(matches!(
            OpusHead::parse(&bad_fam),
            Err(AnalyzerError::UnsupportedFormat(_))
        ));
    }
}

#[test]
fn test_ogg_opus_end_to_end_decode_preskip_and_gain() {
    let dir = std::env::temp_dir().join("layacore_opus_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // Valid 20 ms CELT Fullband Stereo Opus packet (TOC = 0xFC, non-zero coded frame)
    let celt_pkt: &[u8] = &[0xFC, 0xFF, 0xFE, 0x55, 0xAA, 0x33, 0xCC, 0x77, 0x11, 0x88];

    // 50 packets * 960 frames = 48,000 raw frames; pre_skip = 312; trailing trim = 100
    // => final granule pos = 47,900 => exact playable frames = 47,900 - 312 = 47,588
    let p_0db = dir.join("test_0db.opus");
    write_synthetic_ogg_opus(&p_0db, 2, 312, 0, 50, 47_900, celt_pkt);

    // Count exact emitted frames from decoder directly
    let mut dec = open_audio_file(&p_0db).expect("open 0dB opus");
    assert_eq!(dec.metadata().sample_rate, 48_000);
    assert_eq!(dec.metadata().channels, 2);
    assert_eq!(dec.metadata().title.as_deref(), Some("Opus Test"));
    assert_eq!(dec.metadata().artist.as_deref(), Some("LayaCore"));

    let mut buf = Vec::new();
    let mut total_emitted = 0usize;
    let mut energy_0db = 0.0f64;
    loop {
        let n = dec.read_frames(&mut buf, 4096).expect("read opus frames");
        if n == 0 {
            break;
        }
        total_emitted += n;
        for &s in &buf {
            energy_0db += (s as f64) * (s as f64);
        }
    }
    assert_eq!(
        total_emitted, 47_588,
        "Expected exact pre_skip (312) and trailing (100) trim: 47,588 frames"
    );

    // Same stream with +6.0 dB RFC 7845 OpusHead output gain (+1536 Q7.8)
    let p_6db = dir.join("test_plus6db.opus");
    write_synthetic_ogg_opus(&p_6db, 2, 312, 6 * 256, 50, 47_900, celt_pkt);

    let mut dec6 = open_audio_file(&p_6db).expect("open +6dB opus");
    let mut energy_6db = 0.0f64;
    loop {
        let n = dec6.read_frames(&mut buf, 4096).expect("read +6dB frames");
        if n == 0 {
            break;
        }
        for &s in &buf {
            energy_6db += (s as f64) * (s as f64);
        }
    }

    if energy_0db > 1e-12 {
        let db_diff = 10.0 * (energy_6db / energy_0db).log10();
        assert!(
            (db_diff - 6.0).abs() < 0.05,
            "Expected +6.0 dB energy increase from OpusHead gain, got {:.3} dB",
            db_diff
        );
    }

    // Run full analysis pipeline on the Ogg Opus file
    let analysis = analyze_file_simple(&p_0db, &AnalysisOptions::default()).expect("analyze opus");
    assert_eq!(analysis.metadata.sample_rate, 48_000);
    assert_eq!(analysis.quality.verdict, LosslessVerdict::NativeLossy);
    assert!(!analysis.metadata.is_flac());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_graceful_unsupported_format_errors() {
    let dir = std::env::temp_dir().join("layacore_unsupported_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    for ext in ["wma", "ac3", "dsf", "wv"] {
        let p = dir.join(format!("sample.{}", ext));
        fs::write(&p, b"dummy").unwrap();
        let err = open_audio_file(&p).err().expect("expected unsupported error");
        assert!(
            matches!(err, AnalyzerError::UnsupportedFormat(_)),
            "Expected UnsupportedFormat for .{}, got {:?}",
            ext,
            err
        );
        let msg = err.to_string();
        assert!(
            msg.contains("not currently supported"),
            "Expected friendly message for .{}, got: {}",
            ext,
            msg
        );
    }

    let _ = fs::remove_dir_all(&dir);
}
