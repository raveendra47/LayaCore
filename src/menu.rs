use std::io::{self, Write};
use std::path::{Path, PathBuf};
use colored::Colorize;
use console::{Key, Term};
use indicatif::{ProgressBar, ProgressStyle};

use layacore::report::{
    format_batch_csv, format_batch_json, format_json, generate_visual_charts, print_batch_report,
    print_terminal_report, print_transcode_report,
};
use layacore::{
    analyze_file, discover_audio_files, scan_directory, AnalysisOptions, BatchScanOptions,
};

pub struct MenuItem {
    pub key: char,
    pub title: &'static str,
    pub description: &'static str,
}

pub const MENU_ITEMS: &[MenuItem] = &[
    MenuItem {
        key: '1',
        title: "Full Audio Analysis (Mastering & Diagnostics)",
        description: "LUFS Loudness, True Peak, Clipping, Stereo, Spectrum, Key & Waveform",
    },
    MenuItem {
        key: '2',
        title: "Fake Lossless & Lossy Transcode Detector",
        description: "Inspect high-frequency cutoff shelves (128k/192k/320k MP3) & authenticity",
    },
    MenuItem {
        key: '3',
        title: "Batch Folder / Album Scanner",
        description: "Scan an entire folder/album for loudness consistency, peaks & fake lossless",
    },
    MenuItem {
        key: '4',
        title: "Generate Visual Mastering Dashboard Image (PNG)",
        description: "Export high-resolution Plotters multi-panel chart to file (e.g. dashboard.png)",
    },
    MenuItem {
        key: '5',
        title: "Export Audio Analysis Data to JSON",
        description: "Serialize all acoustic measurements into a structured JSON report",
    },
    MenuItem {
        key: '6',
        title: "Exit",
        description: "Quit LayaCore",
    },
];

const BANNER_ART: [&str; 6] = [
    "   ██╗      █████╗ ██╗   ██╗ █████╗  ██████╗ ██████╗ ██████╗ ███████╗   ",
    "   ██║     ██╔══██╗╚██╗ ██╔╝██╔══██╗██╔════╝██╔═══██╗██╔══██╗██╔════╝   ",
    "   ██║     ███████║ ╚████╔╝ ███████║██║     ██║   ██║██████╔╝█████╗     ",
    "   ██║     ██╔══██║  ╚██╔╝  ██╔══██║██║     ██║   ██║██╔══██╗██╔══╝     ",
    "   ███████╗██║  ██║   ██║   ██║  ██║╚██████╗╚██████╔╝██║  ██║███████╗   ",
    "   ╚══════╝╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚═╝  ╚═╝╚══════╝   ",
];

const SUB_LINE_1: &str = "                            L A Y A C O R E                             ";
const SUB_LINE_2: &str = "        Pure Rust Audio Mastering, Dynamics & Authenticity Suite        ";
const SUB_LINE_3: &str = "         v0.1.0  •  ITU-R BS.1770  •  EBU R128  •  MIR  •  FFT          ";

pub fn print_banner() {
    let box_inner = 72;
    let border = "═".repeat(box_inner);
    let red_border = format!("\x1b[38;2;225;20;45m╔{}╗\x1b[0m", border);
    let red_bottom = format!("\x1b[38;2;225;20;45m╚{}╝\x1b[0m", border);
    let side = "\x1b[38;2;225;20;45m║\x1b[0m";
    let empty_row = " ".repeat(box_inner);

    println!("\n  {}", red_border);
    println!("  {}{}{}", side, empty_row, side);

    for line in BANNER_ART {
        println!(
            "  {}\x1b[1m\x1b[38;2;255;45;65m{}\x1b[0m{}",
            side, line, side
        );
    }

    println!("  {}{}{}", side, empty_row, side);
    println!(
        "  {}\x1b[1m\x1b[38;2;255;255;255m{}\x1b[0m{}",
        side, SUB_LINE_1, side
    );
    println!(
        "  {}\x1b[38;2;215;215;220m{}\x1b[0m{}",
        side, SUB_LINE_2, side
    );
    println!(
        "  {}\x1b[38;2;255;65;80m{}\x1b[0m{}",
        side, SUB_LINE_3, side
    );
    println!("  {}{}{}", side, empty_row, side);
    println!("  {}\n", red_bottom);
}

const MENU_LINES: usize = 19;

fn render_menu_lines(selected_idx: usize) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "  {}",
        "Select an analysis option (Use ↑/↓ arrows or press [1-6], then Enter):"
            .bright_white()
            .bold()
    ));
    lines.push(String::new());

    for (i, item) in MENU_ITEMS.iter().enumerate() {
        if i == selected_idx {
            lines.push(format!(
                "  \x1b[1m\x1b[38;2;255;50;65m❯\x1b[0m [\x1b[1m\x1b[38;2;255;50;65m{}\x1b[0m] \x1b[1m\x1b[38;2;255;255;255m{}\x1b[0m",
                item.key, item.title
            ));
            lines.push(format!("      \x1b[38;2;255;160;175m{}\x1b[0m", item.description));
        } else {
            lines.push(format!(
                "    [\x1b[38;2;225;20;40m{}\x1b[0m] \x1b[38;2;220;220;220m{}\x1b[0m",
                item.key, item.title
            ));
            lines.push(format!("      \x1b[38;2;140;140;140m{}\x1b[0m", item.description));
        }
        if i + 1 < MENU_ITEMS.len() {
            lines.push(String::new());
        }
    }

    lines
}

pub fn run_interactive_menu() {
    let term = Term::stdout();

    loop {
        print_banner();

        let mut selected_idx = 0;

        if term.is_term() {
            // Initial render
            let lines = render_menu_lines(selected_idx);
            for line in &lines {
                println!("{}", line);
            }

            loop {
                match term.read_key() {
                    Ok(Key::ArrowUp) => {
                        if selected_idx > 0 {
                            selected_idx -= 1;
                        } else {
                            selected_idx = MENU_ITEMS.len() - 1;
                        }
                        let _ = term.clear_last_lines(MENU_LINES);
                        let updated = render_menu_lines(selected_idx);
                        for line in &updated {
                            println!("{}", line);
                        }
                    }
                    Ok(Key::ArrowDown) => {
                        if selected_idx + 1 < MENU_ITEMS.len() {
                            selected_idx += 1;
                        } else {
                            selected_idx = 0;
                        }
                        let _ = term.clear_last_lines(MENU_LINES);
                        let updated = render_menu_lines(selected_idx);
                        for line in &updated {
                            println!("{}", line);
                        }
                    }
                    Ok(Key::Char(c)) if ('1'..='6').contains(&c) => {
                        selected_idx = (c as usize) - ('1' as usize);
                        let _ = term.clear_last_lines(MENU_LINES);
                        let updated = render_menu_lines(selected_idx);
                        for line in &updated {
                            println!("{}", line);
                        }
                        break;
                    }
                    Ok(Key::Enter) => {
                        break;
                    }
                    Ok(Key::Char('q')) | Ok(Key::Char('Q')) | Ok(Key::Escape) => {
                        println!("\n  Goodbye!\n");
                        return;
                    }
                    _ => {}
                }
            }
        } else {
            // Non-interactive fallback
            let lines = render_menu_lines(selected_idx);
            for line in &lines {
                println!("{}", line);
            }
            print!("\n  Enter option [1-6]: ");
            let _ = io::stdout().flush();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                return;
            }
            let trimmed = input.trim();
            if let Ok(num) = trimmed.parse::<usize>() {
                if num >= 1 && num <= MENU_ITEMS.len() {
                    selected_idx = num - 1;
                } else {
                    continue;
                }
            } else if trimmed.eq_ignore_ascii_case("q") {
                return;
            } else {
                continue;
            }
        }

        // If Exit was selected
        if selected_idx == 5 {
            println!("\n  Exiting LayaCore. Goodbye!\n");
            return;
        }

        // Batch Folder / Album Scanner
        if selected_idx == 2 {
            println!();
            if let Some(dir_path) = prompt_for_directory() {
                run_batch_action(&dir_path, None);
            } else {
                continue;
            }
        } else {
            // Prompt for audio file path
            println!();
            let file_path = match prompt_for_file() {
                Some(p) => p,
                None => continue,
            };

            if file_path.is_dir() {
                // Gracefully run batch scan if the user dropped a folder into [1] or [2]
                let transcode_only = selected_idx == 1;
                run_batch_action(&file_path, Some(transcode_only));
            } else {
                // Run the action corresponding to selected_idx
                run_action(selected_idx, &file_path);
            }
        }

        // Pause before looping back
        println!();
        println!("  {}", "─".repeat(68).bright_black());
        print!(
            "  Press [{}] to return to menu, or [{}] to exit: ",
            "Enter".bright_yellow().bold(),
            "q".bright_red().bold()
        );
        let _ = io::stdout().flush();
        let mut exit_buf = String::new();
        let _ = io::stdin().read_line(&mut exit_buf);
        if exit_buf.trim().eq_ignore_ascii_case("q") {
            println!("\n  Goodbye!\n");
            break;
        }
    }
}

fn strip_outer_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return &s[1..s.len() - 1];
    }
    s
}

fn sanitize_filename_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "batch".to_string()
    } else {
        trimmed.to_string()
    }
}

fn resolve_export_path(input: &str, default_filename: &str, default_ext: &str) -> PathBuf {
    let cleaned = strip_outer_quotes(input);
    let path = PathBuf::from(cleaned);

    // If user entered a folder path (existing directory or trailing separator),
    // automatically place the default report filename inside that folder.
    if path.is_dir() || cleaned.ends_with('\\') || cleaned.ends_with('/') {
        return path.join(default_filename);
    }

    if path.extension().is_none() {
        return path.with_extension(default_ext);
    }

    path
}

fn prompt_for_directory() -> Option<PathBuf> {
    loop {
        print!(
            "  \x1b[1m\x1b[38;2;255;50;70m▶\x1b[0m \x1b[1m\x1b[38;2;255;255;255mEnter album / folder path\x1b[0m (or drag & drop folder here, Enter to cancel): "
        );
        let _ = io::stdout().flush();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return None;
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        let clean_path = strip_outer_quotes(trimmed);
        let path = PathBuf::from(clean_path);

        if path.exists() && path.is_dir() {
            return Some(path);
        } else if path.exists() {
            println!(
                "  {} Path is a file, not a folder: {}\n",
                "✖".bright_red(),
                path.display()
            );
        } else {
            println!(
                "  {} Folder not found: {}\n",
                "✖".bright_red(),
                path.display()
            );
        }
    }
}

fn run_batch_action(dir_path: &Path, forced_transcode_only: Option<bool>) {
    let transcode_only = match forced_transcode_only {
        Some(v) => v,
        None => {
            print!(
                "  Scan mode: [{}] Full Album Analysis (default) or [{}] Transcode Audit only: ",
                "1".bright_yellow().bold(),
                "2".bright_yellow().bold()
            );
            let _ = io::stdout().flush();
            let mut mode_buf = String::new();
            let _ = io::stdin().read_line(&mut mode_buf);
            mode_buf.trim() == "2"
        }
    };

    let recursive = true;
    let total_files = match discover_audio_files(dir_path, recursive) {
        Ok(files) if !files.is_empty() => files.len(),
        Ok(_) => {
            println!(
                "\n  {} No supported audio files found in: {}\n",
                "✖".bright_red(),
                dir_path.display()
            );
            return;
        }
        Err(e) => {
            eprintln!(
                "\n  {} Failed to scan '{}': {}\n",
                "✖".bright_red(),
                dir_path.display(),
                e
            );
            return;
        }
    };

    println!(
        "\n  {} Batch scanning {} track(s) in: {}\n",
        "⚙".bright_cyan(),
        total_files,
        dir_path.display()
    );

    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} [{elapsed_precise}] [{bar:32.red/white}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let pb_clone = pb.clone();
    let options = BatchScanOptions {
        recursive,
        ..Default::default()
    };

    let result = scan_directory(
        dir_path,
        &options,
        Some(move |done: usize, _total: usize, path: &Path| {
            pb_clone.set_position(done as u64);
            if let Some(name) = path.file_name() {
                pb_clone.set_message(name.to_string_lossy().to_string());
            }
        }),
    );

    pb.finish_and_clear();

    match result {
        Ok(report) => {
            print_batch_report(&report, transcode_only);

            let folder_stem = dir_path
                .file_name()
                .map(|n| sanitize_filename_stem(&n.to_string_lossy()))
                .unwrap_or_else(|| "batch".to_string());
            let default_report_name = format!("{}_report.json", folder_stem);

            print!(
                "  Export batch report? [Enter to skip, or file/folder path e.g. album.csv / album.json]: "
            );
            let _ = io::stdout().flush();
            let mut export_buf = String::new();
            let _ = io::stdin().read_line(&mut export_buf);
            let export_str = strip_outer_quotes(&export_buf);

            if !export_str.is_empty() {
                let out_path = resolve_export_path(export_str, &default_report_name, "json");
                if let Some(parent) = out_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }

                let is_csv = out_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.eq_ignore_ascii_case("csv"))
                    .unwrap_or(false);

                if is_csv {
                    let content = format_batch_csv(&report);
                    if let Err(e) = std::fs::write(&out_path, content) {
                        eprintln!("\n  {} Failed to write CSV: {}\n", "✖".bright_red(), e);
                    } else {
                        println!(
                            "\n  {} Batch CSV report saved to: {}\n",
                            "✔".bright_green(),
                            out_path.display()
                        );
                    }
                } else {
                    match format_batch_json(&report, true) {
                        Ok(content) => {
                            if let Err(e) = std::fs::write(&out_path, content) {
                                eprintln!("\n  {} Failed to write JSON: {}\n", "✖".bright_red(), e);
                            } else {
                                println!(
                                    "\n  {} Batch JSON report saved to: {}\n",
                                    "✔".bright_green(),
                                    out_path.display()
                                );
                            }
                        }
                        Err(e) => eprintln!("\n  {} Failed to format JSON: {}\n", "✖".bright_red(), e),
                    }
                }
            }
        }
        Err(err) => {
            eprintln!(
                "\n  {} Error scanning '{}': {}\n",
                "✖".bright_red(),
                dir_path.display(),
                err
            );
        }
    }
}

fn prompt_for_file() -> Option<PathBuf> {
    loop {
        print!(
            "  \x1b[1m\x1b[38;2;255;50;70m▶\x1b[0m \x1b[1m\x1b[38;2;255;255;255mEnter audio file path\x1b[0m (or drag & drop file here, Enter to cancel): "
        );
        let _ = io::stdout().flush();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return None;
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Strip matching outer quotes from drag-and-drop on Windows
        let clean_path = strip_outer_quotes(trimmed);
        let path = PathBuf::from(clean_path);

        if path.exists() {
            return Some(path);
        } else {
            println!("  {} File not found: {}\n", "✖".bright_red(), path.display());
        }
    }
}

fn run_action(idx: usize, file_path: &Path) {
    let options = AnalysisOptions::default();

    println!("\n  {} Analyzing: {}\n", "⚙".bright_cyan(), file_path.display());

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} [{elapsed_precise}] Processing audio frames... {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let pb_clone = pb.clone();
    let result = analyze_file(
        file_path,
        &options,
        Some(move |frames_processed, total_hint| {
            if let Some(total) = total_hint {
                if total > 0 {
                    let pct = (frames_processed as f64 / total as f64 * 100.0).min(100.0);
                    pb_clone.set_message(format!("{:.1}% ({}k frames)", pct, frames_processed / 1000));
                    return;
                }
            }
            pb_clone.set_message(format!("{}k frames", frames_processed / 1000));
        }),
    );

    pb.finish_and_clear();

    match result {
        Ok(analysis) => match idx {
            0 => {
                // Full Report
                print_terminal_report(&analysis);
            }
            1 => {
                // Fake Lossless & Transcode Detector
                print_transcode_report(&analysis);
            }
            3 => {
                // Generate Visual Mastering Dashboard
                print!("  Output image path [default: dashboard.png]: ");
                let _ = io::stdout().flush();
                let mut img_buf = String::new();
                let _ = io::stdin().read_line(&mut img_buf);
                let img_str = strip_outer_quotes(&img_buf);
                let img_path = if img_str.is_empty() {
                    PathBuf::from("dashboard.png")
                } else {
                    resolve_export_path(img_str, "dashboard.png", "png")
                };
                if let Some(parent) = img_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }

                match generate_visual_charts(&analysis, &img_path) {
                    Ok(()) => println!(
                        "\n  {} Mastering dashboard generated at: {}\n",
                        "✔".bright_green(),
                        img_path.display()
                    ),
                    Err(e) => eprintln!("\n  {} Failed to generate chart: {}\n", "✖".bright_red(), e),
                }
            }
            4 => {
                // Export JSON
                let default_json = file_path.with_extension("json");
                let default_name = default_json
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "analysis.json".to_string());
                print!("  Output JSON path [default: {}]: ", default_json.display());
                let _ = io::stdout().flush();
                let mut json_buf = String::new();
                let _ = io::stdin().read_line(&mut json_buf);
                let json_str = strip_outer_quotes(&json_buf);
                let json_path = if json_str.is_empty() {
                    default_json
                } else {
                    resolve_export_path(json_str, &default_name, "json")
                };
                if let Some(parent) = json_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }

                match format_json(&analysis, true) {
                    Ok(content) => {
                        if let Err(e) = std::fs::write(&json_path, content) {
                            eprintln!("\n  {} Failed to write JSON: {}\n", "✖".bright_red(), e);
                        } else {
                            println!(
                                "\n  {} Analysis JSON saved to: {}\n",
                                "✔".bright_green(),
                                json_path.display()
                            );
                        }
                    }
                    Err(e) => eprintln!("\n  {} Failed to format JSON: {}\n", "✖".bright_red(), e),
                }
            }
            _ => {}
        },
        Err(err) => {
            eprintln!(
                "\n  {} Error analyzing '{}': {}\n",
                "✖".bright_red(),
                file_path.display(),
                err
            );
        }
    }
}
