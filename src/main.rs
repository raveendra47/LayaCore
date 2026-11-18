mod menu;

use std::path::PathBuf;
use std::process::exit;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

use layacore::report::{
    format_batch_csv, format_batch_json, format_json, generate_visual_charts, print_batch_report,
    print_terminal_report, print_transcode_report,
};
use layacore::{
    analyze_file, discover_audio_files, scan_directory, AnalysisOptions, BatchScanOptions,
};

#[derive(Parser, Debug)]
#[command(
    name = "layacore_cli",
    author = "LayaCore",
    version = "0.1.0",
    about = "LayaCore — Pure Rust offline audio mastering, loudness (ITU-R BS.1770 / EBU R128), fake lossless transcode verification & batch album scanner"
)]
struct Cli {
    /// Path to an audio file or album folder (positional argument)
    #[arg(value_name = "PATH")]
    file: Option<PathBuf>,

    /// Path to an audio file (as an option: -f, --file)
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    file_opt: Option<PathBuf>,

    /// Batch scan a folder / album directory (supports recursive discovery & parallel analysis)
    #[arg(
        short = 's',
        long = "scan",
        alias = "batch",
        alias = "dir",
        alias = "album",
        value_name = "DIR"
    )]
    scan: Option<PathBuf>,

    /// Do not recurse into subdirectories during batch folder scanning
    #[arg(long = "no-recursive", default_value_t = false)]
    no_recursive: bool,

    /// Run only the Fake Lossless & Lossy Transcode Detector (detect MP3-upsampled to FLAC/WAV)
    #[arg(
        short = 't',
        long = "transcode",
        alias = "verify",
        alias = "check-lossless",
        alias = "transcode-check"
    )]
    transcode: bool,

    /// Output raw JSON instead of terminal tables
    #[arg(long, short = 'j')]
    json: bool,

    /// Format JSON with indentation
    #[arg(long, default_value_t = true)]
    pretty: bool,

    /// Save analysis report to an output file (supports .json or .csv for batch scans)
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,

    /// Generate high-resolution visual charts image (PNG or SVG, e.g. --plot dashboard.png)
    #[arg(long, short = 'p')]
    plot: Option<PathBuf>,

    /// Suppress progress bar
    #[arg(long, short = 'q')]
    quiet: bool,

    /// Target waveform envelope points per second (default: 50)
    #[arg(long, default_value_t = 50)]
    waveform_rate: usize,
}

fn run_batch_cli(dir: &PathBuf, args: &Cli) {
    if !dir.exists() {
        eprintln!("Error: Directory not found: {}", dir.display());
        exit(1);
    }
    if !dir.is_dir() {
        eprintln!("Error: Not a directory: {}", dir.display());
        exit(1);
    }

    let recursive = !args.no_recursive;
    let total_files = match discover_audio_files(dir, recursive) {
        Ok(files) if !files.is_empty() => files.len(),
        Ok(_) => {
            eprintln!("No supported audio files found in '{}'", dir.display());
            exit(1);
        }
        Err(e) => {
            eprintln!("Error scanning directory '{}': {}", dir.display(), e);
            exit(1);
        }
    };

    let progress_bar = if !args.quiet && !args.json {
        let pb = ProgressBar::new(total_files as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.cyan} [{elapsed_precise}] [{bar:32.red/white}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("█▓░"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb.set_message("Scanning album tracks in parallel...");
        Some(pb)
    } else {
        None
    };

    let pb_clone = progress_bar.clone();
    let batch_options = BatchScanOptions {
        recursive,
        analysis_options: AnalysisOptions {
            waveform_buckets_per_second: 10,
            ..Default::default()
        },
        max_threads: None,
    };

    let result = scan_directory(
        dir,
        &batch_options,
        Some(move |done: usize, _total: usize, path: &std::path::Path| {
            if let Some(pb) = &pb_clone {
                pb.set_position(done as u64);
                if let Some(name) = path.file_name() {
                    pb.set_message(name.to_string_lossy().to_string());
                }
            }
        }),
    );

    if let Some(pb) = progress_bar {
        pb.finish_and_clear();
    }

    match result {
        Ok(report) => {
            let resolve_cli_out = |raw: &PathBuf| -> PathBuf {
                if raw.is_dir() {
                    let stem = dir
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "batch".to_string());
                    raw.join(format!("{}_report.json", stem))
                } else {
                    raw.clone()
                }
            };

            if args.json {
                match format_batch_json(&report, args.pretty) {
                    Ok(json_str) => {
                        if let Some(raw_out) = &args.output {
                            let out_path = resolve_cli_out(raw_out);
                            if let Err(e) = std::fs::write(&out_path, &json_str) {
                                eprintln!("Error writing JSON output to {}: {}", out_path.display(), e);
                                exit(1);
                            }
                            println!("Batch JSON report saved to {}", out_path.display());
                        } else {
                            println!("{}", json_str);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize batch report: {}", e);
                        exit(1);
                    }
                }
            } else {
                print_batch_report(&report, args.transcode);

                if let Some(raw_out) = &args.output {
                    let out_path = resolve_cli_out(raw_out);
                    let is_csv = out_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.eq_ignore_ascii_case("csv"))
                        .unwrap_or(false);

                    if is_csv {
                        let csv_str = format_batch_csv(&report);
                        if let Err(e) = std::fs::write(&out_path, csv_str) {
                            eprintln!("Error saving CSV report: {}", e);
                        } else {
                            println!("Batch CSV report saved to {}", out_path.display());
                        }
                    } else {
                        match format_batch_json(&report, args.pretty) {
                            Ok(json_str) => {
                                if let Err(e) = std::fs::write(&out_path, json_str) {
                                    eprintln!("Error saving JSON report: {}", e);
                                } else {
                                    println!("Batch JSON report saved to {}", out_path.display());
                                }
                            }
                            Err(e) => eprintln!("Error formatting batch JSON: {}", e),
                        }
                    }
                }
            }
        }
        Err(err) => {
            eprintln!("\nError scanning folder '{}': {}", dir.display(), err);
            exit(1);
        }
    }
}

fn main() {
    let args = Cli::parse();

    if let Some(scan_dir) = &args.scan {
        run_batch_cli(scan_dir, &args);
        return;
    }

    let target_file = match args.file_opt.clone().or_else(|| args.file.clone()) {
        Some(f) => f,
        None => {
            // When no file or folder is provided, open the interactive LayaCore menu
            menu::run_interactive_menu();
            return;
        }
    };

    if !target_file.exists() {
        eprintln!("Error: Path not found: {}", target_file.display());
        exit(1);
    }

    // Automatically run Batch Folder / Album Scanning when a directory is passed directly
    if target_file.is_dir() {
        run_batch_cli(&target_file, &args);
        return;
    }

    let progress_bar = if !args.quiet && !args.json {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} [{elapsed_precise}] Processing audio frames... {msg}")
                .unwrap(),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let pb_clone = progress_bar.clone();
    let options = AnalysisOptions {
        waveform_buckets_per_second: args.waveform_rate,
        ..Default::default()
    };

    let result = analyze_file(
        &target_file,
        &options,
        Some(move |frames_processed, total_hint| {
            if let Some(pb) = &pb_clone {
                if let Some(total) = total_hint {
                    if total > 0 {
                        let pct = (frames_processed as f64 / total as f64 * 100.0).min(100.0);
                        pb.set_message(format!("{:.1}% ({}k frames)", pct, frames_processed / 1000));
                        return;
                    }
                }
                pb.set_message(format!("{}k frames", frames_processed / 1000));
            }
        }),
    );

    if let Some(pb) = progress_bar {
        pb.finish_and_clear();
    }

    match result {
        Ok(analysis) => {
            if args.json {
                match format_json(&analysis, args.pretty) {
                    Ok(json_str) => {
                        if let Some(out_path) = args.output {
                            if let Err(e) = std::fs::write(&out_path, &json_str) {
                                eprintln!("Error writing JSON output to {}: {}", out_path.display(), e);
                                exit(1);
                            }
                            println!("Analysis saved to {}", out_path.display());
                        } else {
                            println!("{}", json_str);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize report: {}", e);
                        exit(1);
                    }
                }
            } else if args.transcode {
                print_transcode_report(&analysis);

                if let Some(out_path) = args.output {
                    match format_json(&analysis, true) {
                        Ok(json_str) => {
                            if let Err(e) = std::fs::write(&out_path, &json_str) {
                                eprintln!("Error saving report: {}", e);
                            } else {
                                println!("JSON report saved to {}", out_path.display());
                            }
                        }
                        Err(e) => eprintln!("Error formatting JSON: {}", e),
                    }
                }
            } else {
                print_terminal_report(&analysis);

                if let Some(out_path) = args.output {
                    match format_json(&analysis, true) {
                        Ok(json_str) => {
                            if let Err(e) = std::fs::write(&out_path, &json_str) {
                                eprintln!("Error saving report: {}", e);
                            } else {
                                println!("JSON report saved to {}", out_path.display());
                            }
                        }
                        Err(e) => eprintln!("Error formatting JSON: {}", e),
                    }
                }
            }

            // Export visual charts if requested
            if let Some(plot_path) = args.plot {
                match generate_visual_charts(&analysis, &plot_path) {
                    Ok(()) => println!("Visual mastering dashboard generated at: {}", plot_path.display()),
                    Err(e) => eprintln!("Failed to generate visual chart: {}", e),
                }
            }
        }
        Err(err) => {
            eprintln!("\nError analyzing file '{}': {}", target_file.display(), err);
            exit(1);
        }
    }
}
