# LayaCore

[![CI](https://github.com/PhamBoy47/LayaCore/actions/workflows/ci.yml/badge.svg)](https://github.com/PhamBoy47/LayaCore/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-red.svg)](LICENSE)
[![Pure Rust](https://img.shields.io/badge/Pure%20Rust-0%20C%2FFFI%20Deps-orange.svg)](#)

**LayaCore** is a fast, offline, **pure-Rust** audio mastering, dynamics, spectral analysis, fake lossless transcode verification, and batch album scanning suite — with **zero external C/FFI dependencies** (no FFmpeg required).

---

## Features

- **Loudness & Broadcast Compliance (ITU-R BS.1770 / EBU R128)**
  - Integrated LUFS, Short-Term Max (3.0s), Momentary Max (400ms), Loudness Range (LRA), and Spotify/YouTube `-14.0 LUFS` target offset.
- **4× Oversampled True Peak & Defect Detection**
  - Inter-sample True Peak (`dBTP`) with streaming headroom compliance (`<= -1.0 dBTP`), Sample Peak (`dBFS`), RMS, Crest Factor / Dynamic Range, Digital Clipping sample/event counters, and silence detection (`< -60 dBFS`).
- **Fake Lossless & Lossy Transcode Detector**
  - Detects high-frequency brickwall cutoff shelves left by lossy encoders (`128k`, `192k`, `320k` MP3) upscaled into lossless containers.
  - Automatically runs during **Full Audio Analysis** for `.flac` files, or standalone for any file via `-t / --transcode`.
- **Stereo Field & Mono Compatibility**
  - Phase correlation (`-1.0` to `+1.0`), stereo width percentage, Mid/Side RMS ratio, and L/R pan balance.
- **Spectral & Tonal Analysis (FFT)**
  - 31-Band ISO frequency spectrum terminal graph, 6-band acoustic register energy distribution (Sub-Bass through Brilliance/Air), Spectral Centroid, 85% Rolloff, and Spectral Flatness.
- **Musical Information Retrieval (MIR)**
  - Tempo (BPM) estimation via spectral flux onset novelty + autocorrelation, Musical Key & Scale detection via 12-tone Chroma & Krumhansl–Schmuckler profiles, and Chroma visualization.
- **Automated Mastering & Mixing Advisor**
  - Generates a `0–100` release-readiness score and plain-English, actionable recommendations for headroom, loudness normalization, dynamics, stereo phase, EQ balance, silence trimming, and FLAC authenticity.
- **Parallel Batch Folder / Album Scanner**
  - Multi-core parallel scanning across entire albums or music libraries with album-level loudness spread (`LU`), outlier flagging, clipping & fake-lossless audits, and `.csv` / `.json` export.
- **Rich Visual & Structured Outputs**
  - Interactive terminal launcher with arrow-key navigation and drag-and-drop path support.
  - High-resolution multi-panel PNG/SVG mastering dashboard via `plotters` (`--plot dashboard.png`).
  - Full structured JSON and spreadsheet-ready CSV exports.

---

## Supported Formats

Decoded natively in pure Rust via `symphonia` and a custom Sun AU decoder:

`FLAC` · `WAV` · `MP3` · `M4A / AAC` · `ALAC` · `OGG / Vorbis` · `OPUS` · `AIFF` · `MP4` · `AU / SND`

---

## Installation & Build

Requires [Rust](https://rustup.rs/) (stable toolchain).

```bash
git clone https://github.com/PhamBoy47/LayaCore.git
cd LayaCore

# Build an optimized release binary (recommended: ~10–15x faster DSP & batch scanning)
cargo build --release

# Or install globally into Cargo's bin directory
cargo install --path .
```

---

## Usage

### Interactive Launcher
Run without arguments to open the interactive terminal menu (supports `↑`/`↓` arrow keys, direct number keys `1–6`, and Windows drag-and-drop paths):

```bash
layacore
# or during development:
cargo run --release
```

```text
  ╔════════════════════════════════════════════════════════════════════════╗
  ║                                                                        ║
  ║   ██╗      █████╗ ██╗   ██╗ █████╗  ██████╗ ██████╗ ██████╗ ███████╗   ║
  ║   ██║     ██╔══██╗╚██╗ ██╔╝██╔══██╗██╔════╝██╔═══██╗██╔══██╗██╔════╝   ║
  ║   ██║     ███████║ ╚████╔╝ ███████║██║     ██║   ██║██████╔╝█████╗     ║
  ║   ██║     ██╔══██║  ╚██╔╝  ██╔══██║██║     ██║   ██║██╔══██╗██╔══╝     ║
  ║   ███████╗██║  ██║   ██║   ██║  ██║╚██████╗╚██████╔╝██║  ██║███████╗   ║
  ║   ╚══════╝╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚═╝  ╚═╝╚══════╝   ║
  ║                                                                        ║
  ║                            L A Y A C O R E                             ║
  ║        Pure Rust Audio Mastering, Dynamics & Authenticity Suite        ║
  ║         v0.1.0  •  ITU-R BS.1770  •  EBU R128  •  MIR  •  FFT          ║
  ║                                                                        ║
  ╚════════════════════════════════════════════════════════════════════════╝

  Select an analysis option (Use ↑/↓ arrows or press [1-6], then Enter):

  ❯ [1] Full Audio Analysis (Mastering & Diagnostics)
    [2] Fake Lossless & Lossy Transcode Detector
    [3] Batch Folder / Album Scanner
    [4] Generate Visual Mastering Dashboard Image (PNG)
    [5] Export Audio Analysis Data to JSON
    [6] Exit
```

### CLI Commands

```bash
# Full audio analysis (automatically includes Fake Lossless Detector for .flac files)
layacore "track.flac"
layacore -f "mix.wav"

# Dedicated Fake Lossless & Lossy Transcode Detector
layacore -t "track.flac"

# Batch scan an entire album / music folder (auto-detected when passing a directory)
layacore "D:\Music\Album"
layacore --scan "D:\Music\Album" --no-recursive

# Batch Lossless & Transcode Audit across a FLAC library
layacore -t "D:\Music\FLAC_Library"

# Export batch album audit to CSV or JSON (passing a folder saves inside that folder)
layacore "D:\Music\Album" -o album_audit.csv
layacore "D:\Music\Album" -o album_audit.json

# Generate high-resolution visual mastering dashboard (PNG or SVG)
layacore "track.flac" --plot dashboard.png

# Output raw structured JSON to stdout or file
layacore "track.flac" --json -o report.json
```

---

## Library Usage

`layacore` can also be embedded directly in Rust projects:

```rust
use layacore::{analyze_file_simple, scan_directory_simple, AnalysisOptions, BatchScanOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Single-track analysis
    let report = analyze_file_simple("track.flac", &AnalysisOptions::default())?;
    println!("Integrated Loudness: {:.1} LUFS", report.lufs.integrated_lufs);
    println!("True Peak: {:.2} dBTP", report.true_peak.max_true_peak_dbtp);
    println!("Lossless Verdict: {:?}", report.quality.verdict);
    println!("Mastering Score: {}/100 ({})", report.advisor.readiness_score, report.advisor.readiness_grade);

    // Parallel album scan
    let album = scan_directory_simple("./album", &BatchScanOptions::default())?;
    println!("Album Avg LUFS: {:.1} (Spread: {:.1} LU)", album.summary.average_lufs, album.summary.lufs_spread_lu);
    Ok(())
}
```

---

## Project Structure

```text
src/
├── lib.rs              # Single-pass streaming analysis pipeline & public library API
├── main.rs             # CLI interface (clap) & progress bars
├── menu.rs             # Interactive TUI launcher with keyboard navigation
├── batch.rs            # Multi-core parallel folder/album scanner & aggregator
├── advisor.rs          # Automated mastering & mixing advisor + readiness scoring
├── error.rs            # Typed error handling
├── decoder/            # Pure-Rust audio decoders (Symphonia + Sun AU)
├── meters/             # LUFS (BS.1770), 4x True Peak, Time Domain, Stereo, Waveform
├── spectral/           # FFT engine, 31-band ISO spectrum, MIR (BPM/Key), Transcode detector
└── report/             # Terminal tables, TermiChart graphs, Plotters PNG/SVG, JSON/CSV export
tests/                  # Unit & integration test suites
```

---

## Running Tests

```bash
cargo test
```

---

## License

Licensed under the [MIT License](LICENSE).
