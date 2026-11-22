use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iced::widget::operation::{scroll_to, AbsoluteOffset};
use iced::widget::{
    button, canvas, column, container, image as iced_image, progress_bar, row, rule, scrollable,
    text, Id as WidgetId, MouseArea, Space,
};
use iced::{
    clipboard, event, keyboard, mouse, time, window, Alignment, Color, Element, Length,
    Subscription, Task, Theme,
};

pub const ID_SCROLL_HOME: &str = "scroll_home";
pub const ID_SCROLL_MASTERING: &str = "scroll_mastering";
pub const ID_SCROLL_SPECTRAL: &str = "scroll_spectral";
pub const ID_SCROLL_FORENSICS: &str = "scroll_forensics";
pub const ID_SCROLL_BATCH: &str = "scroll_batch";
pub const ID_SCROLL_SETTINGS: &str = "scroll_settings";

#[derive(Debug, Clone)]
pub struct SmoothScroll {
    pub current_y: f32,
    pub target_y: f32,
    pub max_y: f32,
    pub is_animating: bool,
    pub last_tick: std::time::Instant,
    pub last_wheel_time: std::time::Instant,
    pub wheel_streak: u32,
}

impl Default for SmoothScroll {
    fn default() -> Self {
        let now = std::time::Instant::now();
        Self {
            current_y: 0.0,
            target_y: 0.0,
            max_y: 999999.0,
            is_animating: false,
            last_tick: now,
            last_wheel_time: now,
            wheel_streak: 0,
        }
    }
}

use layacore::report::{format_batch_csv, format_batch_json, format_json, generate_visual_charts};
use layacore::{
    analyze_file, scan_directory, AdviceSeverity, AlertSeverity, AnalysisOptions, AudioAnalysis,
    BatchScanOptions, BatchScanReport, LosslessVerdict, TrackScanResult,
};

use crate::charts::{
    format_mmss, BroadcastMeterRack, ChromaCanvas, MiniMeterCanvas, PhaseMeterCanvas,
    ReadinessGauge, SpectrumCanvas, StereoFieldScope, WaveformCanvas,
};
use crate::icons::{icon, IconKind};
use crate::settings::{AppSettings, TargetStandard, TruePeakCeiling};
use crate::theme::{
    self, ACCENT_AMBER, ACCENT_AMBER_DIM, ACCENT_BLUE, ACCENT_BLUE_DIM, ACCENT_CYAN, ACCENT_GREEN,
    ACCENT_GREEN_DIM, ACCENT_ORANGE, ACCENT_PURPLE, ACCENT_PURPLE_DIM, ACCENT_RED, ACCENT_RED_DIM,
    FONT_BOLD, FONT_MONO, FONT_MONO_BOLD, FONT_SEMIBOLD, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

pub const ICON_PNG_BYTES: &[u8] = include_bytes!("../../assets/layacore_icon.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    Home,
    Mastering,
    Spectral,
    Forensics,
    Batch,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchFilter {
    All,
    IssuesOnly,
    FakeLossless,
    Clipping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchSortColumn {
    Name,
    Lufs,
    TruePeak,
    DynamicRange,
    Score,
}

#[derive(Debug, Clone)]
enum JobOutcome {
    SingleTrack {
        path: PathBuf,
        result: Result<AudioAnalysis, String>,
    },
    BatchFolder {
        path: PathBuf,
        result: Result<BatchScanReport, String>,
    },
}

struct ActiveJob {
    label: String,
    progress: Arc<Mutex<(f32, String)>>,
    outcome: Arc<Mutex<Option<JobOutcome>>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectTab(ViewTab),
    OpenFileDialog,
    OpenFolderDialog,
    ToggleRecursive,
    AnalyzePath(PathBuf),
    WindowEvent(iced::Event),
    Tick,
    // Batch controls
    SetBatchFilter(BatchFilter),
    ToggleBatchSort(BatchSortColumn),
    InspectBatchTrack(usize),
    // Export actions
    CopySummary,
    ExportJson,
    ExportBatchCsv,
    SavePngDashboard,
    DismissToast,
    // Settings & Customization
    SetTargetStandard(TargetStandard),
    SetTruePeakCeiling(TruePeakCeiling),
    SetMinDynamicRange(f32),
    SetStrictForensics(bool),
    SetMaxBatchThreads(usize),
    ToggleAutoExportJson,
    ToggleAutoExportCsv,
    ToggleAutoExportPng,
    PickOutputDir,
    ClearOutputDir,
    OpenOutputDirInExplorer,
    ResetSettingsDefaults,
    SaveSettings,
    // Smooth scrolling & momentum
    WheelScrolled(mouse::ScrollDelta),
    KeyScroll(f32),
    CancelSmoothScroll,
    Scrolled { offset_y: f32, max_y: f32 },
    SmoothScrollTick,
}

#[derive(Default)]
pub struct ChartCaches {
    pub broadcast_meter: canvas::Cache,
    pub waveform: canvas::Cache,
    pub readiness: canvas::Cache,
    pub stereo_scope: canvas::Cache,
    pub phase_meter: canvas::Cache,
    pub mini_lufs: canvas::Cache,
    pub mini_tp: canvas::Cache,
    pub mini_dr: canvas::Cache,
    pub mini_tempo: canvas::Cache,
    pub spectrum_mir: canvas::Cache,
    pub chroma_mir: canvas::Cache,
    pub spectrum_forensics: canvas::Cache,
}

impl ChartCaches {
    pub fn clear(&mut self) {
        self.broadcast_meter.clear();
        self.waveform.clear();
        self.readiness.clear();
        self.stereo_scope.clear();
        self.phase_meter.clear();
        self.mini_lufs.clear();
        self.mini_tp.clear();
        self.mini_dr.clear();
        self.mini_tempo.clear();
        self.spectrum_mir.clear();
        self.chroma_mir.clear();
        self.spectrum_forensics.clear();
    }
}

struct FpsTracker {
    last_instant: std::time::Instant,
    fps: f32,
    frame_ms: f32,
}

pub struct LayaCoreApp {
    pub settings: AppSettings,
    icon_handle: iced_image::Handle,
    active_tab: ViewTab,
    recursive_scan: bool,
    file_drag_hovered: bool,

    // Real-time rendering telemetry
    fps_tracker: Arc<Mutex<FpsTracker>>,

    // Active single-track state
    current_track_path: Option<PathBuf>,
    current_analysis: Option<AudioAnalysis>,

    // Vector chart render caches (zero-overhead 60 FPS scrolling on CPU)
    chart_caches: ChartCaches,

    // Active batch scan state
    current_batch_path: Option<PathBuf>,
    current_batch: Option<BatchScanReport>,
    batch_filter: BatchFilter,
    batch_sort_col: BatchSortColumn,
    batch_sort_asc: bool,

    // Recent files / folders
    recent_paths: Vec<PathBuf>,

    // Background worker state
    active_job: Option<ActiveJob>,
    progress_frac: f32,
    progress_detail: String,

    // Status toast (message, is_error)
    toast: Option<(String, bool)>,

    // Physics-interpolated smooth scrolling
    smooth_scroll: SmoothScroll,
}

impl Default for LayaCoreApp {
    fn default() -> Self {
        Self::new()
    }
}

impl LayaCoreApp {
    pub fn new() -> Self {
        let settings = AppSettings::load();
        let recursive = settings.default_recursive_scan;
        Self {
            settings,
            icon_handle: iced_image::Handle::from_bytes(ICON_PNG_BYTES.to_vec()),
            active_tab: ViewTab::Home,
            recursive_scan: recursive,
            file_drag_hovered: false,
            fps_tracker: Arc::new(Mutex::new(FpsTracker {
                last_instant: std::time::Instant::now(),
                fps: 60.0,
                frame_ms: 16.6,
            })),
            current_track_path: None,
            current_analysis: None,
            chart_caches: ChartCaches::default(),
            current_batch_path: None,
            current_batch: None,
            batch_filter: BatchFilter::All,
            batch_sort_col: BatchSortColumn::Name,
            batch_sort_asc: true,
            recent_paths: Vec::new(),
            active_job: None,
            progress_frac: 0.0,
            progress_detail: String::new(),
            toast: None,
            smooth_scroll: SmoothScroll::default(),
        }
    }

    fn active_scroll_id(&self) -> Option<WidgetId> {
        match self.active_tab {
            ViewTab::Home => Some(WidgetId::new(ID_SCROLL_HOME)),
            ViewTab::Mastering => Some(WidgetId::new(ID_SCROLL_MASTERING)),
            ViewTab::Spectral => Some(WidgetId::new(ID_SCROLL_SPECTRAL)),
            ViewTab::Forensics => Some(WidgetId::new(ID_SCROLL_FORENSICS)),
            ViewTab::Batch => Some(WidgetId::new(ID_SCROLL_BATCH)),
            ViewTab::Settings => Some(WidgetId::new(ID_SCROLL_SETTINGS)),
        }
    }

    pub fn title(&self) -> String {
        match (&self.active_tab, &self.current_analysis, &self.current_batch) {
            (ViewTab::Settings, _, _) => "LayaCore — Settings & Mastering Configuration".to_string(),
            (ViewTab::Batch, _, Some(batch)) => format!("LayaCore — Batch: {}", batch.directory),
            (_, Some(a), _) if self.active_tab != ViewTab::Home => {
                format!("LayaCore — {}", a.metadata.file_name)
            }
            _ => "LayaCore — Pro Audio Mastering & Forensics Suite".to_string(),
        }
    }

    pub fn read_telemetry(&self) -> (f32, f32) {
        if let Ok(mut t) = self.fps_tracker.lock() {
            let now = std::time::Instant::now();
            let dt = now.duration_since(t.last_instant).as_secs_f32();
            t.last_instant = now;
            if dt > 0.001 && dt < 0.150 {
                let inst_fps = (1.0 / dt).clamp(1.0, 360.0);
                let inst_ms = dt * 1000.0;
                t.fps = t.fps * 0.82 + inst_fps * 0.18;
                t.frame_ms = t.frame_ms * 0.82 + inst_ms * 0.18;
            } else if dt >= 0.150 {
                t.fps = 60.0;
                t.frame_ms = 16.6;
            }
            (t.fps, t.frame_ms)
        } else {
            (60.0, 16.6)
        }
    }

    pub fn theme(&self) -> Theme {
        theme::layacore_theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let events_sub = event::listen_with(|ev, _status, _id| match &ev {
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if modifiers.command() {
                    match key {
                        keyboard::Key::Character(c) if c.eq_ignore_ascii_case("o") => {
                            if modifiers.shift() {
                                return Some(Message::OpenFolderDialog);
                            } else {
                                return Some(Message::OpenFileDialog);
                            }
                        }
                        keyboard::Key::Character(c) if c.eq_ignore_ascii_case("b") => {
                            return Some(Message::OpenFolderDialog);
                        }
                        keyboard::Key::Character(c) if c.eq_ignore_ascii_case("s") => {
                            return Some(Message::SavePngDashboard);
                        }
                        keyboard::Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                            return Some(Message::CopySummary);
                        }
                        keyboard::Key::Character(c) if c == "," => {
                            return Some(Message::SelectTab(ViewTab::Settings));
                        }
                        _ => {}
                    }
                }
                match key {
                    keyboard::Key::Named(keyboard::key::Named::F1) => {
                        Some(Message::SelectTab(ViewTab::Home))
                    }
                    keyboard::Key::Named(keyboard::key::Named::F2) => {
                        Some(Message::SelectTab(ViewTab::Mastering))
                    }
                    keyboard::Key::Named(keyboard::key::Named::F3) => {
                        Some(Message::SelectTab(ViewTab::Spectral))
                    }
                    keyboard::Key::Named(keyboard::key::Named::F4) => {
                        Some(Message::SelectTab(ViewTab::Forensics))
                    }
                    keyboard::Key::Named(keyboard::key::Named::F5) => {
                        Some(Message::SelectTab(ViewTab::Batch))
                    }
                    keyboard::Key::Named(keyboard::key::Named::F6) => {
                        Some(Message::SelectTab(ViewTab::Settings))
                    }
                    keyboard::Key::Named(keyboard::key::Named::Escape) => {
                        Some(Message::DismissToast)
                    }
                    keyboard::Key::Named(keyboard::key::Named::PageDown) => {
                        Some(Message::KeyScroll(400.0))
                    }
                    keyboard::Key::Named(keyboard::key::Named::PageUp) => {
                        Some(Message::KeyScroll(-400.0))
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                        Some(Message::KeyScroll(85.0))
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                        Some(Message::KeyScroll(-85.0))
                    }
                    keyboard::Key::Named(keyboard::key::Named::Home) => {
                        Some(Message::KeyScroll(-999999.0))
                    }
                    keyboard::Key::Named(keyboard::key::Named::End) => {
                        Some(Message::KeyScroll(999999.0))
                    }
                    _ => None,
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                Some(Message::CancelSmoothScroll)
            }
            iced::Event::Window(win_ev) => match win_ev {
                window::Event::FileHovered(_)
                | window::Event::FilesHoveredLeft
                | window::Event::FileDropped(_) => Some(Message::WindowEvent(ev)),
                _ => None,
            },
            _ => None,
        });

        if self.smooth_scroll.is_animating {
            let scroll_tick = time::every(Duration::from_millis(8)).map(|_| Message::SmoothScrollTick);
            if self.active_job.is_some() {
                let job_tick = time::every(Duration::from_millis(75)).map(|_| Message::Tick);
                Subscription::batch([events_sub, scroll_tick, job_tick])
            } else {
                Subscription::batch([events_sub, scroll_tick])
            }
        } else if self.active_job.is_some() {
            let job_tick = time::every(Duration::from_millis(75)).map(|_| Message::Tick);
            Subscription::batch([events_sub, job_tick])
        } else {
            events_sub
        }
    }

    fn record_recent(&mut self, path: PathBuf) {
        self.recent_paths.retain(|p| p != &path);
        self.recent_paths.insert(0, path);
        self.recent_paths.truncate(6);
    }

    fn start_analysis(&mut self, path: PathBuf) {
        if self.active_job.is_some() {
            self.toast = Some(("An analysis is already running. Please wait...".into(), true));
            return;
        }

        self.chart_caches.clear();
        self.toast = None;
        self.progress_frac = 0.0;

        let progress = Arc::new(Mutex::new((0.0_f32, "Initializing...".to_string())));
        let outcome: Arc<Mutex<Option<JobOutcome>>> = Arc::new(Mutex::new(None));

        if path.is_dir() {
            let label = format!(
                "Scanning folder: {}",
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Album")
            );
            self.progress_detail = "Discovering audio files...".to_string();
            let recursive = self.recursive_scan;
            let threads = self.settings.max_batch_threads;
            let p_clone = Arc::clone(&progress);
            let o_clone = Arc::clone(&outcome);
            let path_clone = path.clone();

            std::thread::spawn(move || {
                let opts = BatchScanOptions {
                    recursive,
                    max_threads: if threads > 0 { Some(threads) } else { None },
                    ..Default::default()
                };
                let res = scan_directory(
                    &path_clone,
                    &opts,
                    Some(|done: usize, total: usize, file_path: &std::path::Path| {
                        let frac = if total > 0 {
                            done as f32 / total as f32
                        } else {
                            0.0
                        };
                        let name = file_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Track");
                        if let Ok(mut guard) = p_clone.lock() {
                            *guard = (frac, format!("[{}/{}] {}", done, total, name));
                        }
                    }),
                )
                .map_err(|e| e.to_string());

                if let Ok(mut guard) = o_clone.lock() {
                    *guard = Some(JobOutcome::BatchFolder {
                        path: path_clone,
                        result: res,
                    });
                }
            });

            self.active_job = Some(ActiveJob {
                label,
                progress,
                outcome,
            });
        } else {
            let fname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Track")
                .to_string();
            let label = format!("Analyzing: {}", fname);
            self.progress_detail = "Decoding & streaming DSP meters...".to_string();
            let p_clone = Arc::clone(&progress);
            let o_clone = Arc::clone(&outcome);
            let path_clone = path.clone();

            std::thread::spawn(move || {
                let opts = AnalysisOptions::default();
                let res = analyze_file(
                    &path_clone,
                    &opts,
                    Some(|frames: u64, total_hint: Option<u64>| {
                        let frac = match total_hint {
                            Some(t) if t > 0 => (frames as f32 / t as f32).clamp(0.0, 0.99),
                            _ => 0.5,
                        };
                        if let Ok(mut guard) = p_clone.lock() {
                            *guard = (frac, format!("Processed {} frames...", frames));
                        }
                    }),
                )
                .map_err(|e| e.to_string());

                if let Ok(mut guard) = o_clone.lock() {
                    *guard = Some(JobOutcome::SingleTrack {
                        path: path_clone,
                        result: res,
                    });
                }
            });

            self.active_job = Some(ActiveJob {
                label,
                progress,
                outcome,
            });
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectTab(tab) => {
                self.active_tab = tab;
                self.smooth_scroll.is_animating = false;
                self.smooth_scroll.current_y = 0.0;
                self.smooth_scroll.target_y = 0.0;
                self.smooth_scroll.max_y = 999999.0;
                self.smooth_scroll.wheel_streak = 0;
                Task::none()
            }
            Message::WheelScrolled(delta) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -y,
                    mouse::ScrollDelta::Pixels { y, .. } => -y / 24.0,
                };
                let now = std::time::Instant::now();
                let dt_since_last = now.duration_since(self.smooth_scroll.last_wheel_time).as_secs_f32();
                self.smooth_scroll.last_wheel_time = now;

                // Kinetic momentum flywheel: successive notches within 220ms build progressive velocity
                let same_dir = (lines > 0.0) == (self.smooth_scroll.target_y >= self.smooth_scroll.current_y);
                if dt_since_last < 0.22 && same_dir {
                    self.smooth_scroll.wheel_streak = (self.smooth_scroll.wheel_streak + 1).min(6);
                } else {
                    self.smooth_scroll.wheel_streak = 0;
                }

                let momentum_factor = 1.0 + (self.smooth_scroll.wheel_streak as f32) * 0.18;
                let step = lines * 108.0 * momentum_factor;
                let new_target = (self.smooth_scroll.target_y + step).clamp(0.0, self.smooth_scroll.max_y);
                self.smooth_scroll.target_y = new_target;
                self.smooth_scroll.is_animating = true;
                self.smooth_scroll.last_tick = now;
                Task::none()
            }
            Message::KeyScroll(offset) => {
                let new_target = (self.smooth_scroll.target_y + offset).clamp(0.0, self.smooth_scroll.max_y);
                self.smooth_scroll.target_y = new_target;
                self.smooth_scroll.is_animating = true;
                self.smooth_scroll.last_tick = std::time::Instant::now();
                Task::none()
            }
            Message::CancelSmoothScroll => {
                if self.smooth_scroll.is_animating {
                    self.smooth_scroll.is_animating = false;
                    self.smooth_scroll.target_y = self.smooth_scroll.current_y;
                    self.smooth_scroll.wheel_streak = 0;
                }
                Task::none()
            }
            Message::Scrolled { offset_y, max_y } => {
                if max_y > 0.0 {
                    self.smooth_scroll.max_y = max_y;
                }
                if !self.smooth_scroll.is_animating {
                    self.smooth_scroll.current_y = offset_y;
                    self.smooth_scroll.target_y = offset_y;
                }
                Task::none()
            }
            Message::SmoothScrollTick => {
                if !self.smooth_scroll.is_animating {
                    return Task::none();
                }
                let now = std::time::Instant::now();
                let dt = now.duration_since(self.smooth_scroll.last_tick).as_secs_f32().clamp(0.001, 0.050);
                self.smooth_scroll.last_tick = now;

                let diff = self.smooth_scroll.target_y - self.smooth_scroll.current_y;
                if diff.abs() > 0.35 {
                    // Frame-rate independent continuous exponential easing
                    // lambda = 15.0 produces silky smooth deceleration curve identical across 30-144 Hz
                    let decay = (-15.0_f32 * dt).exp();
                    self.smooth_scroll.current_y = self.smooth_scroll.target_y - diff * decay;
                    if let Some(id) = self.active_scroll_id() {
                        return scroll_to(
                            id,
                            AbsoluteOffset {
                                x: Some(0.0),
                                y: Some(self.smooth_scroll.current_y),
                            },
                        );
                    }
                } else {
                    self.smooth_scroll.current_y = self.smooth_scroll.target_y;
                    self.smooth_scroll.is_animating = false;
                    self.smooth_scroll.wheel_streak = 0;
                    if let Some(id) = self.active_scroll_id() {
                        return scroll_to(
                            id,
                            AbsoluteOffset {
                                x: Some(0.0),
                                y: Some(self.smooth_scroll.current_y),
                            },
                        );
                    }
                }
                Task::none()
            }
            Message::ToggleRecursive => {
                self.recursive_scan = !self.recursive_scan;
                Task::none()
            }
            Message::OpenFileDialog => {
                let picked = rfd::FileDialog::new()
                    .set_title("Open Audio Track — LayaCore")
                    .add_filter(
                        "Supported Audio",
                        &[
                            "flac", "wav", "mp3", "m4a", "aac", "ogg", "opus", "aiff", "aif",
                            "alac", "caf", "au", "snd",
                        ],
                    )
                    .pick_file();
                if let Some(path) = picked {
                    self.start_analysis(path);
                }
                Task::none()
            }
            Message::OpenFolderDialog => {
                let picked = rfd::FileDialog::new()
                    .set_title("Select Album / Music Folder — LayaCore")
                    .pick_folder();
                if let Some(path) = picked {
                    self.start_analysis(path);
                }
                Task::none()
            }
            Message::AnalyzePath(path) => {
                self.start_analysis(path);
                Task::none()
            }
            Message::WindowEvent(ev) => {
                if let iced::Event::Window(win_ev) = ev {
                    match win_ev {
                        window::Event::FileHovered(_) => {
                            self.file_drag_hovered = true;
                        }
                        window::Event::FilesHoveredLeft => {
                            self.file_drag_hovered = false;
                        }
                        window::Event::FileDropped(path) => {
                            self.file_drag_hovered = false;
                            self.start_analysis(path);
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
            Message::Tick => {
                let mut completed: Option<JobOutcome> = None;
                if let Some(job) = &self.active_job {
                    if let Ok(g) = job.progress.lock() {
                        self.progress_frac = g.0;
                        self.progress_detail = g.1.clone();
                    }
                    if let Ok(mut og) = job.outcome.lock() {
                        completed = og.take();
                    }
                }
                if let Some(outcome) = completed {
                    self.active_job = None;
                    self.progress_frac = 1.0;
                    match outcome {
                        JobOutcome::SingleTrack { path, result } => match result {
                            Ok(analysis) => {
                                self.chart_caches.clear();
                                self.record_recent(path.clone());
                                self.current_track_path = Some(path);

                                // Auto-export handling if enabled in Settings
                                let mut auto_msgs = Vec::new();
                                if self.settings.auto_export_json {
                                    let dir = self.settings.effective_output_dir();
                                    let j_path = dir.join(format!("{}_report.json", stem_of(&analysis.metadata.file_name)));
                                    if let Ok(json) = format_json(&analysis, true) {
                                        if std::fs::write(&j_path, json).is_ok() {
                                            auto_msgs.push("JSON");
                                        }
                                    }
                                }
                                if self.settings.auto_export_png {
                                    let dir = self.settings.effective_output_dir();
                                    let p_path = dir.join(format!("{}_dashboard.png", stem_of(&analysis.metadata.file_name)));
                                    if generate_visual_charts(&analysis, &p_path).is_ok() {
                                        auto_msgs.push("PNG");
                                    }
                                }
                                if !auto_msgs.is_empty() {
                                    self.toast = Some((format!("Analysis complete. Auto-saved {} to output folder.", auto_msgs.join(" & ")), false));
                                }

                                self.current_analysis = Some(analysis);
                                if self.active_tab == ViewTab::Home
                                    || self.active_tab == ViewTab::Batch
                                {
                                    self.active_tab = ViewTab::Mastering;
                                }
                            }
                            Err(err) => {
                                self.toast = Some((err, true));
                            }
                        },
                        JobOutcome::BatchFolder { path, result } => match result {
                            Ok(report) => {
                                self.record_recent(path.clone());
                                self.current_batch_path = Some(path.clone());

                                if self.settings.auto_export_csv {
                                    let dir = self.settings.effective_output_dir();
                                    let album_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("album");
                                    let c_path = dir.join(format!("{}_audit.csv", album_name));
                                    let csv = format_batch_csv(&report);
                                    if std::fs::write(&c_path, csv).is_ok() {
                                        self.toast = Some((format!("Album scan finished. CSV saved to {}", c_path.display()), false));
                                    }
                                }

                                self.current_batch = Some(report);
                                self.batch_filter = BatchFilter::All;
                                self.active_tab = ViewTab::Batch;
                            }
                            Err(err) => {
                                self.toast = Some((err, true));
                            }
                        },
                    }
                }
                Task::none()
            }
            Message::SetBatchFilter(filter) => {
                self.batch_filter = filter;
                Task::none()
            }
            Message::ToggleBatchSort(col) => {
                if self.batch_sort_col == col {
                    self.batch_sort_asc = !self.batch_sort_asc;
                } else {
                    self.batch_sort_col = col;
                    self.batch_sort_asc = col == BatchSortColumn::Name;
                }
                Task::none()
            }
            Message::InspectBatchTrack(idx) => {
                if let Some(batch) = &self.current_batch {
                    if let Some(track) = batch.tracks.get(idx) {
                        if let Some(analysis) = &track.analysis {
                            let full_path =
                                PathBuf::from(&batch.directory).join(&track.relative_path);
                            self.chart_caches.clear();
                            self.current_track_path = Some(full_path);
                            self.current_analysis = Some(analysis.clone());
                            self.active_tab = ViewTab::Mastering;
                        }
                    }
                }
                Task::none()
            }
            Message::CopySummary => {
                if self.active_tab == ViewTab::Batch {
                    if let Some(batch) = &self.current_batch {
                        let s = &batch.summary;
                        let summary = format!(
                            "LayaCore Batch Audit — {}\nTracks: {}/{} | Avg LUFS: {:.1} (Spread: {:.1} LU) | Worst True Peak: {:.2} dBTP | Clipping: {} | Fake Lossless: {}",
                            batch.directory,
                            s.succeeded,
                            s.total_files_found,
                            s.average_lufs,
                            s.lufs_spread_lu,
                            s.worst_true_peak_dbtp,
                            s.clipping_tracks,
                            s.suspected_320k_tracks + s.suspected_192k_tracks + s.suspected_128k_tracks
                        );
                        self.toast = Some(("Album summary copied to clipboard.".into(), false));
                        return clipboard::write(summary);
                    }
                } else if let Some(a) = &self.current_analysis {
                    let summary = format!(
                        "LayaCore Mastering Summary — {}\nFormat: {} ({}) | {} Hz | {} ch\nIntegrated: {:.1} LUFS | True Peak: {:.2} dBTP | DR: {:.1} dB | Phase: {:+.2}\nBPM: {:.1} | Key: {} | Readiness: {}/100 ({}) | Authenticity: {:?}",
                        a.metadata.file_name,
                        a.metadata.format_name,
                        a.metadata.codec_name,
                        a.metadata.sample_rate,
                        a.metadata.channels,
                        a.lufs.integrated_lufs,
                        a.true_peak.max_true_peak_dbtp,
                        a.time_domain.dynamic_range_db,
                        a.stereo.phase_correlation,
                        a.musical.estimated_bpm,
                        a.musical.estimated_key,
                        a.advisor.readiness_score,
                        a.advisor.readiness_grade,
                        a.quality.verdict
                    );
                    self.toast = Some(("Mastering summary copied to clipboard.".into(), false));
                    return clipboard::write(summary);
                }
                Task::none()
            }
            Message::ExportJson => {
                let is_batch = self.active_tab == ViewTab::Batch;
                let default_name = if is_batch {
                    "layacore_album_audit.json".to_string()
                } else if let Some(a) = &self.current_analysis {
                    format!("{}_report.json", stem_of(&a.metadata.file_name))
                } else {
                    return Task::none();
                };

                let out_dir = self.settings.effective_output_dir();
                if let Some(save_path) = rfd::FileDialog::new()
                    .set_title("Export Analysis to JSON")
                    .set_directory(&out_dir)
                    .set_file_name(&default_name)
                    .add_filter("JSON Report", &["json"])
                    .save_file()
                {
                    let res = if is_batch {
                        self.current_batch
                            .as_ref()
                            .ok_or_else(|| "No batch report loaded".to_string())
                            .and_then(|b| format_batch_json(b, true).map_err(|e| e.to_string()))
                    } else {
                        self.current_analysis
                            .as_ref()
                            .ok_or_else(|| "No track analysis loaded".to_string())
                            .and_then(|a| format_json(a, true).map_err(|e| e.to_string()))
                    };

                    match res.and_then(|json| {
                        std::fs::write(&save_path, json).map_err(|e| e.to_string())
                    }) {
                        Ok(()) => {
                            self.toast = Some((
                                format!("Exported JSON to {}", save_path.display()),
                                false,
                            ));
                        }
                        Err(e) => {
                            self.toast = Some((format!("JSON export failed: {}", e), true));
                        }
                    }
                }
                Task::none()
            }
            Message::ExportBatchCsv => {
                if let Some(batch) = &self.current_batch {
                    let out_dir = self.settings.effective_output_dir();
                    if let Some(save_path) = rfd::FileDialog::new()
                        .set_title("Export Album Audit to CSV")
                        .set_directory(&out_dir)
                        .set_file_name("layacore_album_audit.csv")
                        .add_filter("CSV Spreadsheet", &["csv"])
                        .save_file()
                    {
                        let csv = format_batch_csv(batch);
                        match std::fs::write(&save_path, csv) {
                            Ok(()) => {
                                self.toast = Some((
                                    format!("Exported CSV to {}", save_path.display()),
                                    false,
                                ));
                            }
                            Err(e) => {
                                self.toast = Some((format!("CSV export failed: {}", e), true));
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::SavePngDashboard => {
                if let Some(analysis) = &self.current_analysis {
                    let default_name =
                        format!("{}_dashboard.png", stem_of(&analysis.metadata.file_name));
                    let out_dir = self.settings.effective_output_dir();
                    if let Some(save_path) = rfd::FileDialog::new()
                        .set_title("Save Visual Mastering Dashboard (PNG / SVG)")
                        .set_directory(&out_dir)
                        .set_file_name(&default_name)
                        .add_filter("PNG Image", &["png"])
                        .add_filter("SVG Vector", &["svg"])
                        .save_file()
                    {
                        match generate_visual_charts(analysis, &save_path) {
                            Ok(()) => {
                                self.toast = Some((
                                    format!("Saved visual dashboard to {}", save_path.display()),
                                    false,
                                ));
                            }
                            Err(e) => {
                                self.toast = Some((format!("Dashboard export failed: {}", e), true));
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::DismissToast => {
                self.toast = None;
                Task::none()
            }
            Message::SetTargetStandard(std) => {
                self.settings.target_standard = std;
                let _ = self.settings.save();
                self.toast = Some((format!("Mastering target standard set to: {}", std.name()), false));
                Task::none()
            }
            Message::SetTruePeakCeiling(ceiling) => {
                self.settings.true_peak_ceiling = ceiling;
                let _ = self.settings.save();
                self.toast = Some((format!("True Peak ceiling set to: {:.1} dBTP", ceiling.dbtp()), false));
                Task::none()
            }
            Message::SetMinDynamicRange(dr) => {
                self.settings.min_dynamic_range_db = dr;
                let _ = self.settings.save();
                self.toast = Some((format!("Dynamic range warning floor set to: {:.1} dB", dr), false));
                Task::none()
            }
            Message::SetStrictForensics(strict) => {
                self.settings.strict_forensics = strict;
                let _ = self.settings.save();
                self.toast = Some((format!("Lossless forensics mode: {}", if strict { "Strict Detection" } else { "Balanced (Default)" }), false));
                Task::none()
            }
            Message::SetMaxBatchThreads(threads) => {
                self.settings.max_batch_threads = threads;
                let _ = self.settings.save();
                let desc = if threads == 0 { "Auto (All logical cores)".to_string() } else { format!("{} threads", threads) };
                self.toast = Some((format!("Batch scanner concurrency set to: {}", desc), false));
                Task::none()
            }
            Message::ToggleAutoExportJson => {
                self.settings.auto_export_json = !self.settings.auto_export_json;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleAutoExportCsv => {
                self.settings.auto_export_csv = !self.settings.auto_export_csv;
                let _ = self.settings.save();
                Task::none()
            }
            Message::ToggleAutoExportPng => {
                self.settings.auto_export_png = !self.settings.auto_export_png;
                let _ = self.settings.save();
                Task::none()
            }
            Message::PickOutputDir => {
                let current_dir = self.settings.effective_output_dir();
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title("Select Default Output Directory for LayaCore Reports")
                    .set_directory(&current_dir)
                    .pick_folder()
                {
                    self.settings.default_output_dir = Some(folder.clone());
                    let _ = self.settings.save();
                    self.toast = Some((format!("Default output directory set to: {}", folder.display()), false));
                }
                Task::none()
            }
            Message::ClearOutputDir => {
                self.settings.default_output_dir = None;
                let _ = self.settings.save();
                self.toast = Some(("Output directory reset to Documents default.".into(), false));
                Task::none()
            }
            Message::OpenOutputDirInExplorer => {
                let dir = self.settings.effective_output_dir();
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("explorer.exe").arg(&dir).spawn();
                }
                Task::none()
            }
            Message::ResetSettingsDefaults => {
                self.settings = AppSettings::default();
                self.recursive_scan = self.settings.default_recursive_scan;
                let _ = self.settings.save();
                self.toast = Some(("All studio settings reset to factory defaults.".into(), false));
                Task::none()
            }
            Message::SaveSettings => {
                match self.settings.save() {
                    Ok(()) => {
                        self.toast = Some((format!("Preferences saved to {}", AppSettings::config_path().display()), false));
                    }
                    Err(e) => {
                        self.toast = Some((format!("Failed to save settings: {}", e), true));
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = self.view_sidebar();
        let topbar = self.view_topbar();

        let main_body: Element<'_, Message> = if let Some(job) = &self.active_job {
            self.view_loading(job)
        } else {
            match self.active_tab {
                ViewTab::Home => self.view_home(),
                ViewTab::Mastering => self.view_mastering(),
                ViewTab::Spectral => self.view_spectral(),
                ViewTab::Forensics => self.view_forensics(),
                ViewTab::Batch => self.view_batch(),
                ViewTab::Settings => self.view_settings(),
            }
        };

        let mut right_col = column![topbar]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill);

        if self.file_drag_hovered && self.active_tab != ViewTab::Home {
            let drop_banner = container(
                row![
                    badge("DROP DETECTED", ACCENT_RED, ACCENT_RED_DIM),
                    text("Release anywhere to analyze this audio track or album folder immediately")
                        .size(12.5)
                        .font(FONT_SEMIBOLD)
                        .color(TEXT_PRIMARY),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([9, 16])
            .width(Length::Fill)
            .style(theme::card_accent_style(ACCENT_RED, ACCENT_RED_DIM));
            right_col = right_col.push(container(drop_banner).padding([10, 20]));
        }

        if let Some((msg, is_err)) = &self.toast {
            let (border, bg, fg, lbl) = if *is_err {
                (ACCENT_RED, ACCENT_RED_DIM, ACCENT_RED, "ERROR")
            } else {
                (ACCENT_GREEN, ACCENT_GREEN_DIM, ACCENT_GREEN, "DONE")
            };
            let banner = container(
                row![
                    badge(lbl, fg, bg),
                    text(msg).size(12.5).color(TEXT_PRIMARY).width(Length::Fill),
                    button(text("Dismiss").size(11.5).font(FONT_SEMIBOLD))
                        .on_press(Message::DismissToast)
                        .style(theme::secondary_button)
                        .padding([4, 10]),
                ]
                .align_y(Alignment::Center)
                .spacing(10),
            )
            .padding([8, 14])
            .width(Length::Fill)
            .style(theme::card_accent_style(border, bg));
            right_col = right_col.push(container(banner).padding([10, 20]));
        }

        right_col = right_col.push(
            container(main_body)
                .width(Length::Fill)
                .height(Length::Fill),
        );

        row![sidebar, right_col]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    // ==================== SIDEBAR ====================

    fn view_sidebar(&self) -> Element<'_, Message> {
        let brand = row![
            iced_image::Image::new(self.icon_handle.clone())
                .width(38)
                .height(38),
            column![
                text("LAYACORE")
                    .size(16)
                    .font(FONT_BOLD)
                    .color(TEXT_PRIMARY),
                text("PRO SUITE  •  v0.1.0")
                    .size(9.5)
                    .font(FONT_MONO_BOLD)
                    .color(ACCENT_RED),
            ]
            .spacing(2),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let rec_dot_color = if self.recursive_scan {
            ACCENT_GREEN
        } else {
            TEXT_MUTED
        };

        let actions = column![
            button(
                row![
                    icon(IconKind::OpenFile, 14.0, Color::WHITE),
                    text("Open Audio Track").size(13).font(FONT_SEMIBOLD),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            )
            .on_press(Message::OpenFileDialog)
            .style(theme::primary_button)
            .padding([10, 14])
            .width(Length::Fill),
            button(
                row![
                    icon(IconKind::ScanFolder, 14.0, TEXT_PRIMARY),
                    text("Scan Album Folder").size(13).font(FONT_SEMIBOLD),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            )
            .on_press(Message::OpenFolderDialog)
            .style(theme::secondary_button)
            .padding([9, 14])
            .width(Length::Fill),
            button(
                row![
                    text("●").size(9.5).color(rec_dot_color),
                    text(if self.recursive_scan {
                        "Include Subfolders: ON"
                    } else {
                        "Include Subfolders: OFF"
                    })
                    .size(11)
                    .color(TEXT_SECONDARY),
                ]
                .spacing(7)
                .align_y(Alignment::Center)
            )
            .on_press(Message::ToggleRecursive)
            .style(theme::table_header_button(false))
            .padding([5, 8])
            .width(Length::Fill),
        ]
        .spacing(8);

        let has_track = self.current_analysis.is_some();
        let has_batch = self.current_batch.is_some();
        let batch_label = match &self.current_batch {
            Some(b) => format!("Batch Album ({})", b.summary.succeeded),
            None => "Batch Album Scanner".to_string(),
        };

        let nav_item = |kind: IconKind, label: String, tab: ViewTab, enabled: bool| {
            let active = self.active_tab == tab;
            let icon_col = if active {
                ACCENT_RED
            } else if enabled {
                TEXT_SECONDARY
            } else {
                Color::from_rgb(0.30, 0.34, 0.40)
            };
            let indicator: Element<'_, Message> = if active {
                container(Space::new().width(3).height(16))
                    .style(theme::accent_strip_style(ACCENT_RED))
                    .into()
            } else {
                Space::new().width(3).height(16).into()
            };
            let mut btn = button(
                row![
                    indicator,
                    icon(kind, 15.0, icon_col),
                    text(label)
                        .size(12.5)
                        .font(if active { FONT_SEMIBOLD } else { theme::FONT_REGULAR }),
                ]
                .spacing(9)
                .align_y(Alignment::Center),
            )
            .style(theme::nav_button(active))
            .padding([8, 10])
            .width(Length::Fill);
            if enabled {
                btn = btn.on_press(Message::SelectTab(tab));
            }
            btn
        };

        let nav = column![
            text("CONSOLE")
                .size(9.5)
                .font(FONT_BOLD)
                .color(TEXT_MUTED),
            nav_item(IconKind::Home, "Home / Drop Zone".into(), ViewTab::Home, true),
            nav_item(IconKind::Mastering, "Mastering Cockpit".into(), ViewTab::Mastering, has_track),
            nav_item(IconKind::Spectrum, "Spectrum & MIR".into(), ViewTab::Spectral, has_track),
            nav_item(IconKind::Forensics, "Lossless Forensics".into(), ViewTab::Forensics, has_track),
            nav_item(IconKind::Batch, batch_label, ViewTab::Batch, has_batch),
            nav_item(IconKind::Settings, "Settings & Config".into(), ViewTab::Settings, true),
        ]
        .spacing(5);

        let mut sidebar_col = column![
            brand,
            rule::horizontal(1).style(theme::subtle_rule),
            actions,
            rule::horizontal(1).style(theme::subtle_rule),
            nav,
        ]
        .spacing(13)
        .width(Length::Fill);

        if !self.recent_paths.is_empty() {
            let mut rec_col = column![
                text("RECENT SESSIONS")
                    .size(9.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED)
            ]
            .spacing(3);
            for p in &self.recent_paths {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("File")
                    .to_string();
                let dot_col = if p.is_dir() { ACCENT_AMBER } else { ACCENT_CYAN };
                let label = truncate_str(&name, 22);
                rec_col = rec_col.push(
                    button(
                        row![
                            text("•").size(12).color(dot_col),
                            text(label).size(11.5).color(TEXT_SECONDARY),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::AnalyzePath(p.clone()))
                    .style(theme::table_header_button(false))
                    .padding([5, 8])
                    .width(Length::Fill),
                );
            }
            sidebar_col = sidebar_col
                .push(rule::horizontal(1).style(theme::subtle_rule))
                .push(rec_col);
        }

        let (fps, frame_ms) = self.read_telemetry();
        let footer = container(
            column![
                row![
                    text("●").size(8.5).color(if fps >= 50.0 { ACCENT_GREEN } else { ACCENT_AMBER }),
                    text(format!("{:.0} FPS  •  {:.1} ms", fps, frame_ms))
                        .size(9.0)
                        .font(FONT_MONO_BOLD)
                        .color(if fps >= 50.0 { ACCENT_GREEN } else { ACCENT_AMBER }),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                text("DirectX 12 / Vulkan GPU\nBS.1770-4  •  Hardware 120 FPS")
                    .size(9.5)
                    .font(FONT_MONO)
                    .color(TEXT_MUTED),
            ]
            .spacing(4),
        )
        .padding([10, 10])
        .width(Length::Fill)
        .style(theme::card_alt_style);

        sidebar_col = sidebar_col
            .push(Space::new().height(Length::Fill))
            .push(footer);

        container(sidebar_col)
            .width(240)
            .height(Length::Fill)
            .padding(16)
            .style(theme::sidebar_style)
            .into()
    }

    // ==================== EXECUTIVE TOP BAR ====================

    fn view_topbar(&self) -> Element<'_, Message> {
        let header_left: Element<'_, Message> = if self.active_tab == ViewTab::Batch {
            if let Some(batch) = &self.current_batch {
                let s = &batch.summary;
                column![
                    text(format!("Album Audit — {}", truncate_str(&batch.directory, 42)))
                        .size(15.5)
                        .font(FONT_BOLD)
                        .color(TEXT_PRIMARY),
                    row![
                        meta_pill(format!("{} Tracks", s.succeeded)),
                        meta_pill(format!("Avg {:.1} LUFS", s.average_lufs)),
                        meta_pill(format!("Spread {:.1} LU", s.lufs_spread_lu)),
                        meta_pill(format!("Worst {:.2} dBTP", s.worst_true_peak_dbtp)),
                    ]
                    .spacing(6),
                ]
                .spacing(5)
                .into()
            } else {
                column![
                    text("Batch Album Scanner")
                        .size(15.5)
                        .font(FONT_BOLD)
                        .color(TEXT_PRIMARY),
                    text("Drop a folder anywhere to audit an entire album in parallel")
                        .size(11.5)
                        .color(TEXT_MUTED),
                ]
                .spacing(4)
                .into()
            }
        } else if self.active_tab == ViewTab::Settings {
            let target_str = format!("Target: {}", self.settings.target_standard.name());
            let ceil_str = format!("Ceil: {}", self.settings.true_peak_ceiling.label());
            let thread_str = if self.settings.max_batch_threads == 0 {
                "Auto Threads".to_string()
            } else {
                format!("{} Threads", self.settings.max_batch_threads)
            };
            let out_dir_str = self
                .settings
                .default_output_dir
                .as_ref()
                .map(|p| truncate_str(&p.to_string_lossy(), 28))
                .unwrap_or_else(|| "Auto Output Folder".to_string());
            column![
                text("Studio Preferences & Configuration")
                    .size(15.5)
                    .font(FONT_BOLD)
                    .color(TEXT_PRIMARY),
                row![
                    meta_pill(target_str),
                    meta_pill(ceil_str),
                    meta_pill(thread_str),
                    meta_pill(out_dir_str),
                ]
                .spacing(6),
            ]
            .spacing(5)
            .into()
        } else if self.active_tab == ViewTab::Home {
            column![
                text("LayaCore Pro Mastering Console")
                    .size(15.5)
                    .font(FONT_BOLD)
                    .color(TEXT_PRIMARY),
                text("Single-pass ITU-R BS.1770-4 • True Peak • Lossless Forensics • Batch Scanning")
                    .size(11.5)
                    .color(TEXT_MUTED),
            ]
            .spacing(4)
            .into()
        } else if let Some(a) = &self.current_analysis {
            let m = &a.metadata;
            let bit_str = m
                .bits_per_sample
                .map(|b| format!("{}-bit", b))
                .unwrap_or_else(|| "Float / Lossy".into());
            let sr_khz = m.sample_rate as f64 / 1000.0;
            let dur = format_mmss(m.duration_seconds.unwrap_or(0.0));
            let size_mb = m.file_size_bytes as f64 / (1024.0 * 1024.0);
            let ch_lbl = match m.channels {
                1 => "Mono".to_string(),
                2 => "Stereo".to_string(),
                c => format!("{}ch", c),
            };
            let (sc_col, sc_bg) = score_colors(a.advisor.readiness_score);

            column![
                row![
                    text(truncate_filename(&m.file_name, 40))
                        .size(15.5)
                        .font(FONT_BOLD)
                        .color(TEXT_PRIMARY),
                    badge_owned(
                        format!(
                            "SCORE {}/100",
                            a.advisor.readiness_score
                        ),
                        sc_col,
                        sc_bg,
                    ),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    meta_pill(short_codec_label(&m.format_name, &m.codec_name)),
                    meta_pill(format!("{:.1} kHz", sr_khz)),
                    meta_pill(bit_str),
                    meta_pill(ch_lbl),
                    meta_pill(dur),
                    meta_pill(format!("{:.1} MB", size_mb)),
                ]
                .spacing(6),
            ]
            .spacing(5)
            .into()
        } else {
            column![
                text("LayaCore Pro Mastering Console")
                    .size(15.5)
                    .font(FONT_BOLD)
                    .color(TEXT_PRIMARY),
                text("Drop an audio file or album folder anywhere on the window to begin")
                    .size(11.5)
                    .color(TEXT_MUTED),
            ]
            .spacing(4)
            .into()
        };

        let mut export_row = row![].spacing(8).align_y(Alignment::Center);
        let mut has_actions = false;

        if self.active_tab == ViewTab::Batch && self.current_batch.is_some() {
            export_row = export_row
                .push(toolbar_btn(IconKind::Copy, "Copy", Message::CopySummary))
                .push(toolbar_btn(IconKind::ExportData, "CSV", Message::ExportBatchCsv))
                .push(toolbar_btn(IconKind::ExportData, "JSON", Message::ExportJson));
            has_actions = true;
        } else if self.active_tab == ViewTab::Settings {
            export_row = export_row
                .push(toolbar_btn(IconKind::Reset, "Reset Defaults", Message::ResetSettingsDefaults))
                .push(toolbar_btn(IconKind::Pass, "Save Config", Message::SaveSettings));
            has_actions = true;
        } else if matches!(self.active_tab, ViewTab::Mastering | ViewTab::Spectral | ViewTab::Forensics) && self.current_analysis.is_some() {
            export_row = export_row
                .push(toolbar_btn(IconKind::Copy, "Copy", Message::CopySummary))
                .push(toolbar_btn(IconKind::ExportImage, "PNG", Message::SavePngDashboard))
                .push(toolbar_btn(IconKind::ExportData, "JSON", Message::ExportJson));
            has_actions = true;
        }

        if has_actions {
            export_row = export_row.push(
                container(rule::vertical(1).style(theme::subtle_rule)).height(18)
            );
        }

        let (fps, frame_ms) = self.read_telemetry();
        let (fps_fg, fps_bg) = if fps >= 55.0 {
            (ACCENT_GREEN, ACCENT_GREEN_DIM)
        } else if fps >= 30.0 {
            (ACCENT_AMBER, ACCENT_AMBER_DIM)
        } else {
            (ACCENT_RED, ACCENT_RED_DIM)
        };
        export_row = export_row.push(badge_owned(
            format!("{:.0} FPS • {:.1}ms", fps, frame_ms),
            fps_fg,
            fps_bg,
        ));

        container(
            row![
                header_left,
                Space::new().width(Length::Fill),
                export_row,
            ]
            .align_y(Alignment::Center)
            .spacing(16),
        )
        .width(Length::Fill)
        .padding([14, 24])
        .style(theme::topbar_style)
        .into()
    }

    // ==================== LOADING OVERLAY ====================

    fn view_loading<'a>(&'a self, job: &'a ActiveJob) -> Element<'a, Message> {
        let pct = (self.progress_frac * 100.0).clamp(0.0, 100.0);
        let card = container(
            column![
                iced_image::Image::new(self.icon_handle.clone())
                    .width(60)
                    .height(60),
                text(&job.label)
                    .size(18)
                    .font(FONT_BOLD)
                    .color(TEXT_PRIMARY),
                text(&self.progress_detail)
                    .size(12.5)
                    .font(FONT_MONO)
                    .color(TEXT_MUTED),
                progress_bar(0.0..=1.0, self.progress_frac)
                    .girth(10)
                    .style(theme::accent_progress_bar),
                text(format!("{:.0}%", pct))
                    .size(13)
                    .font(FONT_MONO_BOLD)
                    .color(ACCENT_RED),
            ]
            .spacing(12)
            .align_x(Alignment::Center)
            .width(460),
        )
        .padding(34)
        .style(theme::card_style);

        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    // ==================== HOME / DROP ZONE ====================

    fn view_home(&self) -> Element<'_, Message> {
        let format_row = row![
            meta_pill("FLAC".into()),
            meta_pill("WAV".into()),
            meta_pill("ALAC".into()),
            meta_pill("AIFF".into()),
            meta_pill("CAF".into()),
            meta_pill("MP3".into()),
            meta_pill("M4A / AAC".into()),
            meta_pill("OGG / Vorbis".into()),
            meta_pill("Ogg OPUS".into()),
            meta_pill("AU / SND".into()),
        ]
        .spacing(6);

        let out_dir_display = truncate_str(
            &self.settings.effective_output_dir().to_string_lossy(),
            30,
        );

        let active_config_strip = container(
            row![
                row![
                    icon(IconKind::Settings, 13.0, TEXT_MUTED),
                    text("ACTIVE CONFIG:").size(10.0).font(FONT_MONO_BOLD).color(TEXT_MUTED),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
                badge_owned(format!("Target: {}", self.settings.target_standard.name()), ACCENT_BLUE, ACCENT_BLUE_DIM),
                badge_owned(format!("Ceiling: {:.1} dBTP", self.settings.true_peak_ceiling.dbtp()), ACCENT_AMBER, ACCENT_AMBER_DIM),
                badge_owned(format!("Outputs: {}", out_dir_display), ACCENT_GREEN, ACCENT_GREEN_DIM),
                button(
                    row![
                        icon(IconKind::Settings, 11.0, TEXT_SECONDARY),
                        text("Settings").size(10.5).font(FONT_SEMIBOLD),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SelectTab(ViewTab::Settings))
                .style(theme::chip_button(false))
                .padding([4, 10]),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 14])
        .style(theme::card_alt_style);

        let console_badge = container(
            row![
                icon(
                    IconKind::DropZone,
                    22.0,
                    if self.file_drag_hovered {
                        ACCENT_RED
                    } else {
                        ACCENT_BLUE
                    },
                ),
                column![
                    text("STUDIO AUDIO INGEST CONSOLE")
                        .size(10.5)
                        .font(FONT_MONO_BOLD)
                        .color(if self.file_drag_hovered {
                            ACCENT_RED
                        } else {
                            ACCENT_BLUE
                        }),
                    text("Bit-Accurate Pure Rust DSP  •  GPU Accelerated UI")
                        .size(9.5)
                        .color(TEXT_MUTED),
                ]
                .spacing(2),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([7, 16])
        .style(theme::badge_style(
            if self.file_drag_hovered {
                ACCENT_RED_DIM
            } else {
                ACCENT_BLUE_DIM
            },
            if self.file_drag_hovered {
                ACCENT_RED
            } else {
                ACCENT_BLUE
            },
        ));

        let drop_card = container(
            column![
                console_badge,
                Space::new().height(4),
                text(if self.file_drag_hovered {
                    "Release to Analyze Immediately"
                } else {
                    "Drop an Audio File or Album Folder Here"
                })
                .size(24)
                .font(FONT_BOLD)
                .color(if self.file_drag_hovered {
                    ACCENT_RED
                } else {
                    TEXT_PRIMARY
                }),
                text("Single-pass ITU-R BS.1770-4 Loudness  •  4× True Peak  •  Lossless Forensics  •  31-Band FFT  •  MIR")
                    .size(12.5)
                    .color(TEXT_SECONDARY),
                Space::new().height(10),
                row![
                    button(
                        row![
                            icon(IconKind::OpenFile, 14.0, Color::WHITE),
                            text("Open Audio Track").size(14).font(FONT_SEMIBOLD),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    )
                    .on_press(Message::OpenFileDialog)
                    .style(theme::primary_button)
                    .padding([12, 24]),
                    button(
                        row![
                            icon(IconKind::ScanFolder, 14.0, TEXT_PRIMARY),
                            text("Scan Album Folder").size(14).font(FONT_SEMIBOLD),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    )
                    .on_press(Message::OpenFolderDialog)
                    .style(theme::secondary_button)
                    .padding([12, 24]),
                    button(
                        row![
                            icon(IconKind::Settings, 14.0, TEXT_PRIMARY),
                            text("Settings & Config").size(14).font(FONT_SEMIBOLD),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    )
                    .on_press(Message::SelectTab(ViewTab::Settings))
                    .style(theme::secondary_button)
                    .padding([12, 20]),
                ]
                .spacing(12),
                Space::new().height(10),
                format_row,
                Space::new().height(6),
                active_config_strip,
            ]
            .spacing(10)
            .align_x(Alignment::Center)
            .width(Length::Fill),
        )
        .padding(36)
        .max_width(840)
        .style(theme::dropzone_style(self.file_drag_hovered));

        let features = container(
            row![
                feature_pill(
                    "Hardware GPU  •  Pure Rust",
                    "Hardware-accelerated graphics (wgpu DirectX 12 / Vulkan) with automatic CPU fallback & zero FFmpeg.",
                    ACCENT_RED,
                ),
                feature_pill(
                    "Broadcast Precision",
                    "ITU-R BS.1770-4 LUFS (±0.02 LU), 4× polyphase True Peak, and interactive LED ladders.",
                    ACCENT_BLUE,
                ),
                feature_pill(
                    "Lossless Forensics",
                    "Detects MP3-upsampled fake FLACs automatically on single tracks or full libraries.",
                    ACCENT_GREEN,
                ),
            ]
            .spacing(14),
        )
        .max_width(820);

        let content = column![drop_card, features]
            .spacing(18)
            .align_x(Alignment::Center)
            .width(Length::Fill);

        let home_body = container(content)
            .width(Length::Fill)
            .padding(28)
            .center_x(Length::Fill);

        pro_scrollable(ID_SCROLL_HOME, home_body)
    }
}

fn pro_scrollable<'a>(
    id_str: &'static str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    scrollable(
        MouseArea::new(
            container(content.into())
                .width(Length::Fill)
                .height(Length::Shrink),
        )
        .on_scroll(Message::WheelScrolled),
    )
    .id(WidgetId::new(id_str))
    .on_scroll(|vp| Message::Scrolled {
        offset_y: vp.absolute_offset().y,
        max_y: (vp.content_bounds().height - vp.bounds().height).max(0.0),
    })
    .style(theme::studio_scrollable)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

impl LayaCoreApp {
    // ==================== PRO MASTERING COCKPIT ====================

    fn view_mastering(&self) -> Element<'_, Message> {
        let Some(a) = &self.current_analysis else {
            return self.view_home();
        };

        let lufs_color = if a.lufs.integrated_lufs > -10.0 {
            ACCENT_AMBER
        } else {
            ACCENT_BLUE
        };
        let tp_color = if a.true_peak.true_peak_clipping {
            ACCENT_RED
        } else if a.true_peak.inter_sample_clipping_risk {
            ACCENT_AMBER
        } else {
            ACCENT_GREEN
        };
        let dr_color = if a.time_domain.dynamic_range_db < 6.0 {
            ACCENT_RED
        } else if a.time_domain.dynamic_range_db < 8.0 {
            ACCENT_AMBER
        } else {
            ACCENT_GREEN
        };
        let phase_color = if a.stereo.phase_correlation < 0.0 {
            ACCENT_RED
        } else if a.stereo.phase_correlation < 0.3 {
            ACCENT_AMBER
        } else {
            ACCENT_GREEN
        };

        let lufs_norm = ((a.lufs.integrated_lufs + 30.0) / 26.0).clamp(0.0, 1.0);
        let tp_norm = ((a.true_peak.max_true_peak_dbtp + 12.0) / 14.0).clamp(0.0, 1.0);
        let dr_norm = (a.time_domain.dynamic_range_db / 20.0).clamp(0.0, 1.0);

        // Left side of Hero Cockpit: 4 KPI cards stacked above Interactive Waveform
        let kpi_row = row![
            kpi_card_with_meter(
                "INTEGRATED LUFS",
                format!("{:.1} LUFS", a.lufs.integrated_lufs),
                format!("LRA {:.1} LU", a.lufs.loudness_range_lra),
                lufs_color,
                lufs_norm,
                Some(16.0 / 26.0),
                &self.chart_caches.mini_lufs,
            ),
            kpi_card_with_meter(
                "TRUE PEAK (4×)",
                format!("{:.2} dBTP", a.true_peak.max_true_peak_dbtp),
                if a.true_peak.true_peak_clipping {
                    "CLIPPING (> 0.0)"
                } else if a.true_peak.inter_sample_clipping_risk {
                    "Hot (> -1.0)"
                } else {
                    "Streaming Safe"
                },
                tp_color,
                tp_norm,
                Some(11.0 / 14.0),
                &self.chart_caches.mini_tp,
            ),
            kpi_card_with_meter(
                "DYNAMIC RANGE",
                format!("{:.1} dB", a.time_domain.dynamic_range_db),
                format!("Clips: {}", a.time_domain.total_clipped_samples),
                dr_color,
                dr_norm,
                Some(0.40),
                &self.chart_caches.mini_dr,
            ),
            kpi_card_with_meter(
                "TEMPO & KEY",
                format!("{:.0} BPM • {}", a.musical.estimated_bpm, a.musical.estimated_key),
                format!("Phase {:+.2}", a.stereo.phase_correlation),
                phase_color,
                ((a.stereo.phase_correlation + 1.0) * 0.5).clamp(0.0, 1.0),
                Some(0.65),
                &self.chart_caches.mini_tempo,
            ),
        ]
        .spacing(10)
        .width(Length::Fill);

        let waveform_card = container(
            column![
                row![
                    text("INTERACTIVE WAVEFORM ENVELOPE")
                        .size(10.5)
                        .font(FONT_BOLD)
                        .color(TEXT_MUTED),
                    Space::new().width(Length::Fill),
                    text("Hover to inspect Peak / RMS")
                        .size(10)
                        .color(TEXT_MUTED),
                    meta_pill(format!("PK {:.2} dBFS", a.time_domain.overall_peak_dbfs)),
                    meta_pill(format!("RMS {:.1} dBFS", a.time_domain.overall_rms_dbfs)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                canvas::Canvas::new(WaveformCanvas {
                    buckets: &a.waveform.buckets,
                    duration_seconds: a.metadata.duration_seconds.unwrap_or(0.0),
                    cache: &self.chart_caches.waveform,
                })
                .width(Length::Fill)
                .height(142),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Length::Fill)
        .style(theme::card_style);

        let hero_left = column![kpi_row, waveform_card]
            .spacing(12)
            .width(Length::Fill);

        // Right side of Hero Cockpit: Vertical Broadcast LED Meter Rack!
        let (l_pk, l_rms, l_clip) = a
            .time_domain
            .channels
            .first()
            .map(|c| (c.peak_dbfs, c.rms_dbfs, c.clipped_samples > 0))
            .unwrap_or((a.time_domain.overall_peak_dbfs, a.time_domain.overall_rms_dbfs, a.time_domain.total_clipped_samples > 0));
        let (r_pk, r_rms, r_clip) = a
            .time_domain
            .channels
            .get(1)
            .map(|c| (c.peak_dbfs, c.rms_dbfs, c.clipped_samples > 0))
            .unwrap_or((l_pk, l_rms, l_clip));

        let meter_rack_card = container(
            column![
                row![
                    text("BROADCAST METER")
                        .size(10)
                        .font(FONT_BOLD)
                        .color(TEXT_MUTED),
                    Space::new().width(Length::Fill),
                    text("dBFS / LUFS")
                        .size(9)
                        .font(FONT_MONO)
                        .color(TEXT_MUTED),
                ]
                .align_y(Alignment::Center),
                canvas::Canvas::new(BroadcastMeterRack {
                    left_peak_dbfs: l_pk,
                    left_rms_dbfs: l_rms,
                    right_peak_dbfs: r_pk,
                    right_rms_dbfs: r_rms,
                    left_clipped: l_clip,
                    right_clipped: r_clip,
                    integrated_lufs: a.lufs.integrated_lufs,
                    short_term_lufs: a.lufs.short_term_max_lufs,
                    momentary_lufs: a.lufs.momentary_max_lufs,
                    true_peak_dbtp: a.true_peak.max_true_peak_dbtp,
                    cache: &self.chart_caches.broadcast_meter,
                })
                .width(Length::Fill)
                .height(254),
            ]
            .spacing(6),
        )
        .padding(11)
        .width(214)
        .style(theme::card_style);

        let hero_cockpit = row![hero_left, meter_rack_card]
            .spacing(12)
            .width(Length::Fill);

        let mut sections = column![hero_cockpit].spacing(12).width(Length::Fill);

        // Conditional FLAC Authenticity Forensic Banner
        if a.metadata.is_flac() {
            let (v_lbl, v_color, v_bg) = verdict_style(a.quality.verdict);
            let flac_banner = container(
                row![
                    icon(IconKind::Forensics, 16.0, v_color),
                    badge(v_lbl, v_color, v_bg),
                    text(&a.quality.verdict_description)
                        .size(12.5)
                        .font(FONT_SEMIBOLD)
                        .color(TEXT_PRIMARY)
                        .width(Length::Fill),
                    button(text("Forensics Details →").size(11.5).font(FONT_SEMIBOLD))
                        .on_press(Message::SelectTab(ViewTab::Forensics))
                        .style(theme::secondary_button)
                        .padding([6, 12]),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([9, 14])
            .width(Length::Fill)
            .style(theme::card_accent_style(v_color, v_bg));
            sections = sections.push(flac_banner);
        }

        // Streaming Platform Loudness Normalization Strip
        let sc = &a.lufs.streaming_compliance;
        let platform_strip = container(
            column![
                text("STREAMING PLATFORM NORMALIZATION (ESTIMATED GAIN ADJUSTMENT)")
                    .size(10)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED),
                row![
                    platform_tile("Spotify", "-14.0 LUFS", -sc.spotify_diff_lu),
                    platform_tile("YouTube", "-14.0 LUFS", -sc.youtube_diff_lu),
                    platform_tile("Apple Music", "-16.0 LUFS", -sc.apple_music_diff_lu),
                    platform_tile("Broadcast (EBU)", "-23.0 LUFS", -sc.broadcast_diff_lu),
                ]
                .spacing(10)
                .width(Length::Fill),
            ]
            .spacing(8),
        )
        .padding([11, 14])
        .width(Length::Fill)
        .style(theme::card_style);
        sections = sections.push(platform_strip);

        // Diagnostics Row: Left = Readiness + Stereo Vector Scope + Phase | Right = Mastering Advisor
        let phase_status = if a.stereo.phase_correlation < 0.0 {
            ("OUT OF PHASE", ACCENT_RED, ACCENT_RED_DIM)
        } else if a.stereo.phase_correlation < 0.3 {
            ("WIDE / CHECK MONO", ACCENT_AMBER, ACCENT_AMBER_DIM)
        } else {
            ("MONO SAFE", ACCENT_GREEN, ACCENT_GREEN_DIM)
        };

        let left_diag = container(
            column![
                text("RELEASE READINESS & STEREO FIELD")
                    .size(10.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED),
                row![
                    canvas::Canvas::new(ReadinessGauge {
                        score: a.advisor.readiness_score,
                        grade: a.advisor.readiness_grade.clone(),
                        cache: &self.chart_caches.readiness,
                    })
                    .width(Length::FillPortion(1))
                    .height(152),
                    canvas::Canvas::new(StereoFieldScope {
                        width_percent: a.stereo.stereo_width_percent,
                        lr_balance_percent: a.stereo.lr_balance_percent,
                        phase_correlation: a.stereo.phase_correlation,
                        cache: &self.chart_caches.stereo_scope,
                    })
                    .width(Length::FillPortion(1))
                    .height(152),
                ]
                .spacing(10),
                rule::horizontal(1).style(theme::subtle_rule),
                row![
                    text("STEREO PHASE CORRELATION")
                        .size(10)
                        .font(FONT_BOLD)
                        .color(TEXT_MUTED),
                    Space::new().width(Length::Fill),
                    badge(phase_status.0, phase_status.1, phase_status.2),
                ]
                .align_y(Alignment::Center),
                canvas::Canvas::new(PhaseMeterCanvas {
                    correlation: a.stereo.phase_correlation,
                    cache: &self.chart_caches.phase_meter,
                })
                .width(Length::Fill)
                .height(38),
                rule::horizontal(1).style(theme::subtle_rule),
                metric_line("Short-Term Max (3s)", format!("{:.1} LUFS", a.lufs.short_term_max_lufs)),
                metric_line("Momentary Max (400ms)", format!("{:.1} LUFS", a.lufs.momentary_max_lufs)),
                metric_line("Mid / Side Ratio", format!("{:+.1} dB", a.stereo.mid_side_ratio_db)),
                metric_line("L / R Pan Balance", format!("{:+.1}%", a.stereo.lr_balance_percent)),
                metric_line("Silence Floor (< -60 dB)", format!("{:.1}%", a.time_domain.silence_percentage)),
            ]
            .spacing(8),
        )
        .padding(14)
        .width(Length::FillPortion(2))
        .style(theme::card_style);

        let mut advisor_list = column![
            row![
                text("AUTOMATED MASTERING & MIXING ADVISOR")
                    .size(10.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED),
                Space::new().width(Length::Fill),
                meta_pill(format!("{} Diagnostics", a.advisor.items.len())),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(9);

        for item in &a.advisor.items {
            let (sev_icon, sev_lbl, sev_col, sev_bg) = match item.severity {
                AdviceSeverity::Critical => (IconKind::Critical, "CRITICAL", ACCENT_RED, ACCENT_RED_DIM),
                AdviceSeverity::Warning => (IconKind::Warning, "WARNING", ACCENT_AMBER, ACCENT_AMBER_DIM),
                AdviceSeverity::Tip => (IconKind::Tip, "TIP", ACCENT_BLUE, ACCENT_BLUE_DIM),
                AdviceSeverity::Good => (IconKind::Pass, "PASS", ACCENT_GREEN, ACCENT_GREEN_DIM),
            };
            let advice_row = container(
                row![
                    container(Space::new().width(3).height(Length::Fill))
                        .style(theme::accent_strip_style(sev_col)),
                    column![
                        row![
                            icon(sev_icon, 14.0, sev_col),
                            badge(sev_lbl, sev_col, sev_bg),
                            text(&item.category)
                                .size(11)
                                .font(FONT_BOLD)
                                .color(TEXT_PRIMARY),
                        ]
                        .spacing(7)
                        .align_y(Alignment::Center),
                        text(&item.summary)
                            .size(12.5)
                            .font(FONT_SEMIBOLD)
                            .color(TEXT_PRIMARY),
                        container(
                            text(format!("ACTION →  {}", item.action))
                                .size(11.5)
                                .color(TEXT_SECONDARY)
                        )
                        .padding([6, 9])
                        .width(Length::Fill)
                        .style(theme::advice_action_well),
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .height(Length::Shrink),
            )
            .padding([10, 12])
            .width(Length::Fill)
            .style(theme::card_alt_style);
            advisor_list = advisor_list.push(advice_row);
        }

        let advisor_card = container(advisor_list)
            .padding(14)
            .width(Length::FillPortion(3))
            .style(theme::card_style);

        sections = sections.push(row![left_diag, advisor_card].spacing(12).width(Length::Fill));

        pro_scrollable(
            ID_SCROLL_MASTERING,
            container(sections).padding(18).width(Length::Fill),
        )
    }

    // ==================== SPECTRUM & MIR VIEW ====================

    fn view_spectral(&self) -> Element<'_, Message> {
        let Some(a) = &self.current_analysis else {
            return self.view_home();
        };

        let spec_card = container(
            column![
                row![
                    text("31-BAND ISO FREQUENCY SPECTRUM (HOVER FOR LIVE BAND READOUT)")
                        .size(10.5)
                        .font(FONT_BOLD)
                        .color(TEXT_MUTED),
                    Space::new().width(Length::Fill),
                    row![
                        meta_pill(format!("Centroid {:.0} Hz", a.spectral.spectral_centroid_hz)),
                        meta_pill(format!("85% Rolloff {:.0} Hz", a.spectral.spectral_rolloff_hz)),
                        meta_pill(format!("Flatness {:.3}", a.spectral.spectral_flatness)),
                    ]
                    .spacing(6),
                ]
                .align_y(Alignment::Center),
                canvas::Canvas::new(SpectrumCanvas {
                    bands: &a.spectral.bands_31,
                    cutoff_hz: a.quality.detected_cutoff_hz,
                    cache: &self.chart_caches.spectrum_mir,
                })
                .width(Length::Fill)
                .height(270),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(Length::Fill)
        .style(theme::card_style);

        let bal = &a.spectral.balance;
        let balance_card = container(
            column![
                text("6-BAND ACOUSTIC REGISTER ENERGY")
                    .size(10.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED),
                Space::new().height(4),
                register_bar("Sub-Bass (20–60 Hz)", bal.sub_bass_db, ACCENT_RED),
                register_bar("Bass (60–250 Hz)", bal.bass_db, ACCENT_ORANGE),
                register_bar("Low Mids (250–500 Hz)", bal.low_mids_db, ACCENT_AMBER),
                register_bar("Mids (500–2k Hz)", bal.mids_db, ACCENT_AMBER),
                register_bar("High Mids (2k–6k Hz)", bal.high_mids_db, ACCENT_CYAN),
                register_bar("Brilliance / Air (6k–20k Hz)", bal.brilliance_db, ACCENT_BLUE),
            ]
            .spacing(11),
        )
        .padding(16)
        .width(Length::FillPortion(1))
        .style(theme::card_style);

        let mir_card = container(
            column![
                text("MUSICAL INFORMATION RETRIEVAL (MIR)")
                    .size(10.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED),
                row![
                    kpi_card(
                        "ESTIMATED TEMPO",
                        format!("{:.1} BPM", a.musical.estimated_bpm),
                        format!("Confidence {:.0}%", a.musical.bpm_confidence * 100.0),
                        ACCENT_PURPLE,
                    ),
                    kpi_card(
                        "MUSICAL KEY & SCALE",
                        a.musical.estimated_key.clone(),
                        format!("Confidence {:.0}%", a.musical.key_confidence * 100.0),
                        ACCENT_RED,
                    ),
                ]
                .spacing(10),
                text("12-TONE CHROMA PROFILE  (Hover notes for energy share)")
                    .size(10.5)
                    .font(FONT_SEMIBOLD)
                    .color(TEXT_MUTED),
                canvas::Canvas::new(ChromaCanvas {
                    chroma: &a.musical.chroma_profile,
                    estimated_key: &a.musical.estimated_key,
                    cache: &self.chart_caches.chroma_mir,
                })
                .width(Length::Fill)
                .height(155),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(Length::FillPortion(1))
        .style(theme::card_style);

        let body = column![spec_card, row![balance_card, mir_card].spacing(14)]
            .spacing(14)
            .width(Length::Fill);

        pro_scrollable(
            ID_SCROLL_SPECTRAL,
            container(body).padding(18).width(Length::Fill),
        )
    }

    // ==================== FAKE LOSSLESS FORENSICS VIEW ====================

    fn view_forensics(&self) -> Element<'_, Message> {
        let Some(a) = &self.current_analysis else {
            return self.view_home();
        };

        let q = &a.quality;
        let (v_lbl, v_col, v_bg) = verdict_style(q.verdict);

        let verdict_card = container(
            column![
                row![
                    icon(IconKind::Forensics, 18.0, v_col),
                    badge(v_lbl, v_col, v_bg),
                    meta_pill(format!("Confidence {:.0}%", q.confidence_pct)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                text(&q.verdict_description)
                    .size(15)
                    .font(FONT_SEMIBOLD)
                    .color(TEXT_PRIMARY),
            ]
            .spacing(10),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::card_accent_style(v_col, v_bg));

        let cutoff_str = q
            .detected_cutoff_hz
            .map(|hz| format!("{:.2} kHz", hz / 1000.0))
            .unwrap_or_else(|| "Full Bandwidth".into());

        let metrics = row![
            kpi_card(
                "DETECTED CUTOFF",
                cutoff_str,
                format!("Nyquist {:.1} kHz", q.nyquist_hz / 1000.0),
                v_col,
            ),
            kpi_card(
                "BANDWIDTH UTILIZED",
                format!("{:.1}%", q.bandwidth_utilized_pct),
                "Active usable spectrum",
                ACCENT_BLUE,
            ),
            kpi_card(
                "SHELF DROP SLOPE",
                format!("{:.1} dB", q.shelf_drop_db),
                "High-freq cliff steepness",
                ACCENT_AMBER,
            ),
            kpi_card(
                "HF ENERGY (>16 kHz)",
                format!("{:.1} dBFS", q.high_freq_energy_dbfs),
                "Upper air-band floor",
                ACCENT_PURPLE,
            ),
        ]
        .spacing(12)
        .width(Length::Fill);

        let spec_card = container(
            column![
                text("SPECTRUM & BRICKWALL CUTOFF AUDIT  (HOVER TO INSPECT HF ROLLOFF)")
                    .size(10.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED),
                canvas::Canvas::new(SpectrumCanvas {
                    bands: &a.spectral.bands_31,
                    cutoff_hz: q.detected_cutoff_hz,
                    cache: &self.chart_caches.spectrum_forensics,
                })
                .width(Length::Fill)
                .height(290),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(Length::Fill)
        .style(theme::card_style);

        let body = column![verdict_card, metrics, spec_card]
            .spacing(14)
            .width(Length::Fill);

        pro_scrollable(
            ID_SCROLL_FORENSICS,
            container(body).padding(18).width(Length::Fill),
        )
    }

    // ==================== BATCH ALBUM SCANNER VIEW ====================

    fn view_batch(&self) -> Element<'_, Message> {
        let Some(batch) = &self.current_batch else {
            return self.view_home();
        };

        let s = &batch.summary;
        let fake_count =
            s.suspected_320k_tracks + s.suspected_192k_tracks + s.suspected_128k_tracks;

        let issues_count = batch
            .tracks
            .iter()
            .filter(|t| match &t.analysis {
                None => true,
                Some(a) => {
                    a.true_peak.inter_sample_clipping_risk
                        || a.time_domain.total_clipped_samples > 0
                        || a.advisor.readiness_score < 75
                        || is_fake_verdict(a.quality.verdict)
                }
            })
            .count();

        let clip_count = batch
            .tracks
            .iter()
            .filter(|t| {
                t.analysis
                    .as_ref()
                    .map(|a| {
                        a.true_peak.true_peak_clipping
                            || a.true_peak.inter_sample_clipping_risk
                            || a.time_domain.total_clipped_samples > 0
                    })
                    .unwrap_or(false)
            })
            .count();

        let summary_row = row![
            kpi_card(
                "TRACKS SCANNED",
                format!("{} / {}", s.succeeded, s.total_files_found),
                format!(
                    "Duration {}  •  Failed: {}",
                    format_mmss(s.total_duration_seconds),
                    s.failed
                ),
                ACCENT_BLUE,
            ),
            kpi_card(
                "ALBUM AVG LUFS",
                format!("{:.1} LUFS", s.average_lufs),
                format!(
                    "Spread {:.1} LU ({:.1}..{:.1})",
                    s.lufs_spread_lu, s.min_lufs, s.max_lufs
                ),
                if s.lufs_spread_lu > 4.0 {
                    ACCENT_AMBER
                } else {
                    ACCENT_GREEN
                },
            ),
            kpi_card(
                "WORST TRUE PEAK",
                format!("{:.2} dBTP", s.worst_true_peak_dbtp),
                format!(
                    "Clipping: {}  •  Hot: {}",
                    s.clipping_tracks, s.true_peak_risk_tracks
                ),
                if s.worst_true_peak_dbtp > 0.0 {
                    ACCENT_RED
                } else if s.worst_true_peak_dbtp > -1.0 {
                    ACCENT_AMBER
                } else {
                    ACCENT_GREEN
                },
            ),
            kpi_card(
                "LOSSLESS AUDIT",
                format!("{} Genuine", s.verified_lossless_tracks),
                format!(
                    "Suspected Fake: {}  •  Lossy: {}",
                    fake_count, s.native_lossy_tracks
                ),
                if fake_count > 0 {
                    ACCENT_RED
                } else {
                    ACCENT_GREEN
                },
            ),
        ]
        .spacing(12)
        .width(Length::Fill);

        let filters = row![
            text("FILTER:").size(10.5).font(FONT_BOLD).color(TEXT_MUTED),
            filter_chip(format!("All ({})", batch.tracks.len()), BatchFilter::All, self.batch_filter),
            filter_chip(format!("Issues ({})", issues_count), BatchFilter::IssuesOnly, self.batch_filter),
            filter_chip(format!("Suspected Fake ({})", fake_count), BatchFilter::FakeLossless, self.batch_filter),
            filter_chip(format!("Clipping / Hot ({})", clip_count), BatchFilter::Clipping, self.batch_filter),
            Space::new().width(Length::Fill),
            text("Click headers to sort  •  Inspect opens full cockpit")
                .size(11)
                .color(TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut indexed: Vec<(usize, &TrackScanResult)> = batch
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| match self.batch_filter {
                BatchFilter::All => true,
                BatchFilter::IssuesOnly => match &t.analysis {
                    None => true,
                    Some(a) => {
                        a.true_peak.inter_sample_clipping_risk
                            || a.time_domain.total_clipped_samples > 0
                            || a.advisor.readiness_score < 75
                            || is_fake_verdict(a.quality.verdict)
                    }
                },
                BatchFilter::FakeLossless => t
                    .analysis
                    .as_ref()
                    .map(|a| is_fake_verdict(a.quality.verdict))
                    .unwrap_or(false),
                BatchFilter::Clipping => t
                    .analysis
                    .as_ref()
                    .map(|a| {
                        a.true_peak.true_peak_clipping
                            || a.true_peak.inter_sample_clipping_risk
                            || a.time_domain.total_clipped_samples > 0
                    })
                    .unwrap_or(false),
            })
            .collect();

        indexed.sort_by(|(_, a), (_, b)| {
            let ord = match self.batch_sort_col {
                BatchSortColumn::Name => a
                    .file_name
                    .to_lowercase()
                    .cmp(&b.file_name.to_lowercase()),
                BatchSortColumn::Lufs => {
                    let va = a.analysis.as_ref().map(|x| x.lufs.integrated_lufs).unwrap_or(-999.0);
                    let vb = b.analysis.as_ref().map(|x| x.lufs.integrated_lufs).unwrap_or(-999.0);
                    va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
                }
                BatchSortColumn::TruePeak => {
                    let va = a
                        .analysis
                        .as_ref()
                        .map(|x| x.true_peak.max_true_peak_dbtp)
                        .unwrap_or(-999.0);
                    let vb = b
                        .analysis
                        .as_ref()
                        .map(|x| x.true_peak.max_true_peak_dbtp)
                        .unwrap_or(-999.0);
                    va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
                }
                BatchSortColumn::DynamicRange => {
                    let va = a
                        .analysis
                        .as_ref()
                        .map(|x| x.time_domain.dynamic_range_db)
                        .unwrap_or(-999.0);
                    let vb = b
                        .analysis
                        .as_ref()
                        .map(|x| x.time_domain.dynamic_range_db)
                        .unwrap_or(-999.0);
                    va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
                }
                BatchSortColumn::Score => {
                    let va = a.analysis.as_ref().map(|x| x.advisor.readiness_score).unwrap_or(0);
                    let vb = b.analysis.as_ref().map(|x| x.advisor.readiness_score).unwrap_or(0);
                    va.cmp(&vb)
                }
            };
            if self.batch_sort_asc {
                ord
            } else {
                ord.reverse()
            }
        });

        let sort_lbl = |base: &str, col: BatchSortColumn| {
            if self.batch_sort_col == col {
                format!("{} {}", base, if self.batch_sort_asc { "▲" } else { "▼" })
            } else {
                base.to_string()
            }
        };

        let header = row![
            button(
                text(sort_lbl("TRACK", BatchSortColumn::Name))
                    .size(10.5)
                    .font(FONT_BOLD)
            )
            .on_press(Message::ToggleBatchSort(BatchSortColumn::Name))
            .style(theme::table_header_button(self.batch_sort_col == BatchSortColumn::Name))
            .width(Length::FillPortion(4)),
            button(
                text(sort_lbl("LUFS", BatchSortColumn::Lufs))
                    .size(10.5)
                    .font(FONT_BOLD)
            )
            .on_press(Message::ToggleBatchSort(BatchSortColumn::Lufs))
            .style(theme::table_header_button(self.batch_sort_col == BatchSortColumn::Lufs))
            .width(Length::FillPortion(1)),
            button(
                text(sort_lbl("TRUE PEAK", BatchSortColumn::TruePeak))
                    .size(10.5)
                    .font(FONT_BOLD)
            )
            .on_press(Message::ToggleBatchSort(BatchSortColumn::TruePeak))
            .style(theme::table_header_button(self.batch_sort_col == BatchSortColumn::TruePeak))
            .width(Length::FillPortion(1)),
            button(
                text(sort_lbl("DR", BatchSortColumn::DynamicRange))
                    .size(10.5)
                    .font(FONT_BOLD)
            )
            .on_press(Message::ToggleBatchSort(BatchSortColumn::DynamicRange))
            .style(theme::table_header_button(
                self.batch_sort_col == BatchSortColumn::DynamicRange
            ))
            .width(Length::FillPortion(1)),
            button(
                text(sort_lbl("SCORE", BatchSortColumn::Score))
                    .size(10.5)
                    .font(FONT_BOLD)
            )
            .on_press(Message::ToggleBatchSort(BatchSortColumn::Score))
            .style(theme::table_header_button(self.batch_sort_col == BatchSortColumn::Score))
            .width(Length::FillPortion(1)),
            text("AUTHENTICITY")
                .size(10.5)
                .font(FONT_BOLD)
                .color(TEXT_MUTED)
                .width(Length::FillPortion(2)),
            text("").width(82),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut table_rows =
            column![header, rule::horizontal(1).style(theme::subtle_rule)].spacing(5);

        for (row_i, (orig_idx, track)) in indexed.into_iter().enumerate() {
            let striped = row_i % 2 == 1;
            let row_elem: Element<'_, Message> = if let Some(a) = &track.analysis {
                let (v_lbl, v_col, v_bg) = verdict_style(a.quality.verdict);
                let tp_col = if a.true_peak.true_peak_clipping {
                    ACCENT_RED
                } else if a.true_peak.inter_sample_clipping_risk {
                    ACCENT_AMBER
                } else {
                    ACCENT_GREEN
                };
                let (score_col, _) = score_colors(a.advisor.readiness_score);

                container(
                    row![
                        text(truncate_str(&track.relative_path, 44))
                            .size(12.5)
                            .font(FONT_SEMIBOLD)
                            .color(TEXT_PRIMARY)
                            .width(Length::FillPortion(4)),
                        text(format!("{:.1}", a.lufs.integrated_lufs))
                            .size(12.5)
                            .font(FONT_MONO_BOLD)
                            .color(ACCENT_BLUE)
                            .width(Length::FillPortion(1)),
                        text(format!("{:.2}", a.true_peak.max_true_peak_dbtp))
                            .size(12.5)
                            .font(FONT_MONO_BOLD)
                            .color(tp_col)
                            .width(Length::FillPortion(1)),
                        text(format!("{:.1} dB", a.time_domain.dynamic_range_db))
                            .size(12)
                            .font(FONT_MONO)
                            .color(TEXT_SECONDARY)
                            .width(Length::FillPortion(1)),
                        text(format!("{}/100", a.advisor.readiness_score))
                            .size(12)
                            .font(FONT_MONO_BOLD)
                            .color(score_col)
                            .width(Length::FillPortion(1)),
                        container(badge(v_lbl, v_col, v_bg)).width(Length::FillPortion(2)),
                        button(text("Inspect →").size(11.5).font(FONT_SEMIBOLD))
                            .on_press(Message::InspectBatchTrack(orig_idx))
                            .style(theme::secondary_button)
                            .padding([4, 9])
                            .width(82),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([7, 10])
                .width(Length::Fill)
                .style(theme::table_row_style(striped))
                .into()
            } else {
                let err_msg = track.error.as_deref().unwrap_or("Decode failed");
                container(
                    row![
                        text(truncate_str(&track.relative_path, 44))
                            .size(12.5)
                            .font(FONT_SEMIBOLD)
                            .color(ACCENT_RED)
                            .width(Length::FillPortion(4)),
                        text(truncate_str(err_msg, 64))
                            .size(11.5)
                            .color(TEXT_MUTED)
                            .width(Length::FillPortion(6)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([7, 10])
                .width(Length::Fill)
                .style(theme::card_accent_style(ACCENT_RED, ACCENT_RED_DIM))
                .into()
            };

            table_rows = table_rows.push(row_elem);
        }

        let table_card = container(table_rows)
            .padding(14)
            .width(Length::Fill)
            .style(theme::card_style);

        let mut body = column![summary_row, filters, table_card]
            .spacing(14)
            .width(Length::Fill);

        if !s.alerts.is_empty() {
            let mut alert_col = column![
                text(format!("FLAGGED ALBUM ALERTS ({})", s.alerts.len()))
                    .size(10.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED)
            ]
            .spacing(6);
            for alert in s.alerts.iter().take(20) {
                let (a_icon, sev_lbl, col, bg) = match alert.severity {
                    AlertSeverity::Error => (IconKind::Critical, "ERROR", ACCENT_RED, ACCENT_RED_DIM),
                    AlertSeverity::Warning => (IconKind::Warning, "WARN", ACCENT_AMBER, ACCENT_AMBER_DIM),
                    AlertSeverity::Info => (IconKind::Tip, "INFO", ACCENT_BLUE, ACCENT_BLUE_DIM),
                };
                alert_col = alert_col.push(
                    container(
                        row![
                            icon(a_icon, 13.0, col),
                            badge(sev_lbl, col, bg),
                            text(format!("[{}] {}", alert.category, alert.track))
                                .size(12)
                                .font(FONT_SEMIBOLD)
                                .color(TEXT_PRIMARY),
                            text(&alert.message).size(12).color(TEXT_SECONDARY),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([6, 10])
                    .width(Length::Fill)
                    .style(theme::card_alt_style),
                );
            }
            body = body.push(
                container(alert_col)
                    .padding(14)
                    .width(Length::Fill)
                    .style(theme::card_style),
            );
        }

        pro_scrollable(
            ID_SCROLL_BATCH,
            container(body).padding(18).width(Length::Fill),
        )
    }

    // ==================== STUDIO PREFERENCES & CONFIGURATION ====================

    fn view_settings(&self) -> Element<'_, Message> {
        let title_header = row![
            column![
                text("STUDIO PREFERENCES & CONFIGURATION")
                    .size(17)
                    .font(FONT_BOLD)
                    .color(TEXT_PRIMARY),
                text("Configure export directories, target loudness standards, limiter ceilings, lossless forensics, and batch multithreading.")
                    .size(12)
                    .color(TEXT_MUTED),
            ]
            .spacing(4)
            .width(Length::Fill),
            row![
                button(
                    row![
                        icon(IconKind::Reset, 13.0, TEXT_PRIMARY),
                        text("Reset Defaults").size(12.5).font(FONT_SEMIBOLD),
                    ]
                    .spacing(7)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::ResetSettingsDefaults)
                .style(theme::secondary_button)
                .padding([8, 16]),
                button(
                    row![
                        icon(IconKind::Pass, 13.0, Color::WHITE),
                        text("Save Settings").size(12.5).font(FONT_SEMIBOLD),
                    ]
                    .spacing(7)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SaveSettings)
                .style(theme::primary_button)
                .padding([8, 18]),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .width(Length::Fill);

        // 1. Output & Export Directory Card
        let out_dir = self.settings.effective_output_dir();
        let path_text = out_dir.to_string_lossy().to_string();
        let is_custom = self.settings.default_output_dir.is_some();

        let mut dir_controls = row![
            button(
                row![
                    icon(IconKind::FolderOpen, 13.0, TEXT_PRIMARY),
                    text("Browse Output Folder...").size(12).font(FONT_SEMIBOLD),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .on_press(Message::PickOutputDir)
            .style(theme::secondary_button)
            .padding([7, 14]),
            button(
                row![
                    icon(IconKind::OpenFile, 13.0, TEXT_PRIMARY),
                    text("Reveal in Explorer").size(12).font(FONT_SEMIBOLD),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .on_press(Message::OpenOutputDirInExplorer)
            .style(theme::secondary_button)
            .padding([7, 14]),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        if is_custom {
            dir_controls = dir_controls.push(
                button(
                    row![
                        icon(IconKind::Reset, 12.0, TEXT_MUTED),
                        text("Reset to Documents").size(12).color(TEXT_MUTED),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::ClearOutputDir)
                .style(theme::table_header_button(false))
                .padding([7, 10]),
            );
        }

        let auto_toggle = |label: &'static str, sub: &'static str, active: bool, msg: Message| {
            let badge_lbl = if active { "ENABLED" } else { "DISABLED" };
            let (badge_fg, badge_bg) = if active {
                (ACCENT_GREEN, ACCENT_GREEN_DIM)
            } else {
                (TEXT_MUTED, theme::BG_CARD_ALT)
            };

            button(
                row![
                    column![
                        text(label).size(12.5).font(FONT_SEMIBOLD).color(TEXT_PRIMARY),
                        text(sub).size(11).color(TEXT_MUTED),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    badge(badge_lbl, badge_fg, badge_bg),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            )
            .on_press(msg)
            .style(theme::setting_card_button(active))
            .padding([10, 14])
            .width(Length::Fill)
        };

        let output_card = container(
            column![
                row![
                    icon(IconKind::FolderOpen, 16.0, ACCENT_BLUE),
                    text("DEFAULT EXPORT & AUDIT DIRECTORY")
                        .size(11)
                        .font(FONT_BOLD)
                        .color(ACCENT_BLUE),
                    Space::new().width(Length::Fill),
                    if is_custom {
                        badge("CUSTOM DIRECTORY", ACCENT_CYAN, theme::BG_CARD_ALT)
                    } else {
                        badge("DOCUMENTS DEFAULT", TEXT_MUTED, theme::BG_CARD_ALT)
                    },
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("All exported JSON mastering reports, batch CSV audits, and PNG dashboard images will be saved to this folder.")
                    .size(11.5)
                    .color(TEXT_SECONDARY),
                container(
                    row![
                        icon(IconKind::FolderOpen, 14.0, ACCENT_CYAN),
                        text(path_text)
                            .size(12)
                            .font(FONT_MONO)
                            .color(TEXT_PRIMARY)
                            .width(Length::Fill),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([10, 14])
                .width(Length::Fill)
                .style(theme::card_alt_style),
                dir_controls,
                rule::horizontal(1).style(theme::subtle_rule),
                text("AUTOMATED REPORT GENERATION")
                    .size(10.5)
                    .font(FONT_BOLD)
                    .color(TEXT_MUTED),
                column![
                    auto_toggle(
                        "Auto-Save JSON Analysis Report",
                        "Automatically write detailed machine-readable <track>_report.json upon single track analysis completion.",
                        self.settings.auto_export_json,
                        Message::ToggleAutoExportJson,
                    ),
                    auto_toggle(
                        "Auto-Save CSV Album Audit",
                        "Automatically generate complete multi-track spreadsheet <album>_audit.csv upon batch scan completion.",
                        self.settings.auto_export_csv,
                        Message::ToggleAutoExportCsv,
                    ),
                    auto_toggle(
                        "Auto-Generate PNG Visual Dashboard",
                        "Automatically render high-resolution 1920×1080 visual mastering dashboard snapshot on track completion.",
                        self.settings.auto_export_png,
                        Message::ToggleAutoExportPng,
                    ),
                ]
                .spacing(8)
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::card_style);

        // 2. Target Loudness Standard Card
        let mut target_tiles = column![].spacing(8).width(Length::Fill);
        for std in TargetStandard::ALL {
            let is_sel = self.settings.target_standard == std;

            let tile = button(
                row![
                    column![
                        row![
                            text(std.name())
                                .size(13)
                                .font(FONT_BOLD)
                                .color(if is_sel { ACCENT_BLUE } else { TEXT_PRIMARY }),
                            Space::new().width(8),
                            meta_pill(std.summary_specs().into()),
                            Space::new().width(Length::Fill),
                            if is_sel {
                                badge("ACTIVE TARGET", ACCENT_BLUE, ACCENT_BLUE_DIM)
                            } else {
                                badge("SELECT", TEXT_MUTED, theme::BG_CARD_ALT)
                            },
                        ]
                        .align_y(Alignment::Center),
                        text(std.description()).size(11.5).color(TEXT_SECONDARY),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            )
            .on_press(Message::SetTargetStandard(std))
            .style(theme::setting_card_button(is_sel))
            .padding([12, 16])
            .width(Length::Fill);

            target_tiles = target_tiles.push(tile);
        }

        let target_card = container(
            column![
                row![
                    icon(IconKind::Mastering, 16.0, ACCENT_AMBER),
                    text("MASTERING TARGET LOUDNESS STANDARD")
                        .size(11)
                        .font(FONT_BOLD)
                        .color(ACCENT_AMBER),
                    Space::new().width(Length::Fill),
                    badge_owned(format!("Target: {:.1} LUFS", self.settings.target_standard.target_lufs()), ACCENT_AMBER, ACCENT_AMBER_DIM),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("Select the target distribution standard. This adjusts normalisation offsets, limiter tolerance, and pass/warning advisory thresholds.")
                    .size(11.5)
                    .color(TEXT_SECONDARY),
                target_tiles,
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::card_style);

        // 3. Limiter Ceiling & Dynamic Range Floors Card
        let mut ceiling_row = row![].spacing(8);
        for &ceil in &TruePeakCeiling::ALL {
            let active = self.settings.true_peak_ceiling == ceil;
            ceiling_row = ceiling_row.push(
                button(
                    text(ceil.label())
                        .size(11.5)
                        .font(if active { FONT_BOLD } else { FONT_SEMIBOLD }),
                )
                .on_press(Message::SetTruePeakCeiling(ceil))
                .style(theme::chip_button(active))
                .padding([7, 12]),
            );
        }

        let dr_options: [f32; 4] = [6.0, 8.0, 10.0, 12.0];
        let mut dr_row = row![].spacing(8);
        for &dr in &dr_options {
            let active = (self.settings.min_dynamic_range_db - dr).abs() < 0.01;
            let label = match dr as u32 {
                6 => "6.0 dB (Hot/Loud)",
                8 => "8.0 dB (Balanced)",
                10 => "10.0 dB (Dynamic)",
                _ => "12.0 dB (Audiophile)",
            };
            dr_row = dr_row.push(
                button(
                    text(label)
                        .size(11.5)
                        .font(if active { FONT_BOLD } else { FONT_SEMIBOLD }),
                )
                .on_press(Message::SetMinDynamicRange(dr))
                .style(theme::chip_button(active))
                .padding([7, 12]),
            );
        }

        let headroom_card = container(
            column![
                row![
                    icon(IconKind::Tip, 16.0, ACCENT_PURPLE),
                    text("HEADROOM LIMITERS & DYNAMIC RANGE FLOORS")
                        .size(11)
                        .font(FONT_BOLD)
                        .color(ACCENT_PURPLE),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("Configure True Peak limiter thresholds and dynamic compression tolerance before advisory warnings trigger.")
                    .size(11.5)
                    .color(TEXT_SECONDARY),
                column![
                    text("TRUE PEAK CEILING THRESHOLD (4× OVERSAMPLED)")
                        .size(10.5)
                        .font(FONT_BOLD)
                        .color(TEXT_MUTED),
                    ceiling_row,
                ]
                .spacing(6),
                rule::horizontal(1).style(theme::subtle_rule),
                column![
                    text("MINIMUM DYNAMIC RANGE WARNING FLOOR")
                        .size(10.5)
                        .font(FONT_BOLD)
                        .color(TEXT_MUTED),
                    dr_row,
                ]
                .spacing(6),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::card_style);

        // 4. Lossless Forensics & Engine Card
        let forensics_mode_row = row![
            button(
                text("Balanced Mode (Recommended)")
                    .size(12)
                    .font(if !self.settings.strict_forensics { FONT_BOLD } else { FONT_SEMIBOLD }),
            )
            .on_press(Message::SetStrictForensics(false))
            .style(theme::chip_button(!self.settings.strict_forensics))
            .padding([8, 14]),
            button(
                text("Strict Mode (Aggressive Cutoff Detection)")
                    .size(12)
                    .font(if self.settings.strict_forensics { FONT_BOLD } else { FONT_SEMIBOLD }),
            )
            .on_press(Message::SetStrictForensics(true))
            .style(theme::chip_button(self.settings.strict_forensics))
            .padding([8, 14]),
        ]
        .spacing(10);

        let dsp_specs = container(
            column![
                text("PURE RUST ZERO-GPU DSP SPECIFICATIONS")
                    .size(10)
                    .font(FONT_MONO_BOLD)
                    .color(TEXT_MUTED),
                row![
                    meta_pill("ITU-R BS.1770-4 K-Filter".into()),
                    meta_pill("4× Polyphase Sinc True Peak".into()),
                    meta_pill("2048-pt FFT".into()),
                    meta_pill("31-Band 1/3 Octave RTA".into()),
                    meta_pill("0 C/FFI".into()),
                    meta_pill("0 GPU".into()),
                ]
                .spacing(6),
            ]
            .spacing(6),
        )
        .padding([10, 14])
        .width(Length::Fill)
        .style(theme::card_alt_style);

        let forensics_card = container(
            column![
                row![
                    icon(IconKind::Forensics, 16.0, ACCENT_GREEN),
                    text("LOSSLESS FORENSICS & DSP ENGINE")
                        .size(11)
                        .font(FONT_BOLD)
                        .color(ACCENT_GREEN),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("Balanced mode flags sharp brickwall roll-offs at 16 kHz (128 kbps), 19 kHz (192 kbps), and 20.5 kHz (320 kbps). Strict mode also detects acoustic roll-offs and muted high frequencies.")
                    .size(11.5)
                    .color(TEXT_SECONDARY),
                forensics_mode_row,
                dsp_specs,
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::card_style);

        // 5. Batch Album Scanner Concurrency Card
        let thread_options: [usize; 5] = [0, 2, 4, 8, 16];
        let mut thread_row = row![].spacing(8);
        for &t in &thread_options {
            let active = self.settings.max_batch_threads == t;
            let label = if t == 0 {
                "Auto (All Cores)".to_string()
            } else {
                format!("{} Threads", t)
            };
            thread_row = thread_row.push(
                button(
                    text(label)
                        .size(11.5)
                        .font(if active { FONT_BOLD } else { FONT_SEMIBOLD }),
                )
                .on_press(Message::SetMaxBatchThreads(t))
                .style(theme::chip_button(active))
                .padding([7, 12]),
            );
        }

        let batch_card = container(
            column![
                row![
                    icon(IconKind::Batch, 16.0, ACCENT_CYAN),
                    text("BATCH SCANNER CONCURRENCY & RECURSION")
                        .size(11)
                        .font(FONT_BOLD)
                        .color(ACCENT_CYAN),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("Configure multi-threaded CPU parallelization for scanning entire albums and disk folders.")
                    .size(11.5)
                    .color(TEXT_SECONDARY),
                column![
                    text("PARALLEL WORKER THREAD COUNT")
                        .size(10.5)
                        .font(FONT_BOLD)
                        .color(TEXT_MUTED),
                    thread_row,
                ]
                .spacing(6),
                rule::horizontal(1).style(theme::subtle_rule),
                row![
                    column![
                        text("Subfolder Scan Recursion")
                            .size(12.5)
                            .font(FONT_SEMIBOLD)
                            .color(TEXT_PRIMARY),
                        text("Traverse and audit nested subdirectories when an album or artist folder is dropped.")
                            .size(11)
                            .color(TEXT_MUTED),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(
                        badge(
                            if self.recursive_scan { "ENABLED" } else { "DISABLED" },
                            if self.recursive_scan { ACCENT_GREEN } else { TEXT_MUTED },
                            if self.recursive_scan { ACCENT_GREEN_DIM } else { theme::BG_CARD_ALT },
                        ),
                    )
                    .on_press(Message::ToggleRecursive)
                    .style(theme::table_header_button(false)),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::card_style);

        // Bottom Persistence Path Well
        let cfg_path = AppSettings::config_path();
        let footer_card = container(
            row![
                icon(IconKind::Settings, 14.0, TEXT_MUTED),
                text(format!("Configuration file: {}", cfg_path.display()))
                    .size(11)
                    .font(FONT_MONO)
                    .color(TEXT_MUTED)
                    .width(Length::Fill),
                button(
                    row![
                        icon(IconKind::Pass, 12.0, Color::WHITE),
                        text("Save Settings").size(12).font(FONT_SEMIBOLD),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SaveSettings)
                .style(theme::primary_button)
                .padding([6, 14]),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .padding([10, 14])
        .width(Length::Fill)
        .style(theme::card_alt_style);

        let content = column![
            title_header,
            output_card,
            target_card,
            headroom_card,
            forensics_card,
            batch_card,
            footer_card,
        ]
        .spacing(16)
        .width(Length::Fill);

        pro_scrollable(
            ID_SCROLL_SETTINGS,
            container(content).padding(20).width(Length::Fill),
        )
    }
}

// ==================== UI HELPER COMPONENTS ====================

fn toolbar_btn<'a>(kind: IconKind, label: &'a str, msg: Message) -> Element<'a, Message> {
    button(
        row![
            icon(kind, 13.5, TEXT_SECONDARY),
            text(label).size(12).font(FONT_SEMIBOLD),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .on_press(msg)
    .style(theme::secondary_button)
    .padding([7, 11])
    .into()
}

fn kpi_card<'a>(
    label: &'a str,
    value: impl Into<String>,
    subtitle: impl Into<String>,
    accent: Color,
) -> Element<'a, Message> {
    let value = value.into();
    let subtitle = subtitle.into();
    container(
        column![
            container(Space::new().width(Length::Fill).height(3))
                .style(theme::accent_strip_style(accent)),
            column![
                text(label).size(9.5).font(FONT_BOLD).color(TEXT_MUTED),
                text(value).size(19.5).font(FONT_MONO_BOLD).color(accent),
                text(subtitle).size(11).color(TEXT_SECONDARY),
            ]
            .spacing(4)
            .padding([9, 12]),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .style(theme::card_style)
    .into()
}

fn kpi_card_with_meter<'a>(
    label: &'a str,
    value: impl Into<String>,
    subtitle: impl Into<String>,
    accent: Color,
    value_norm: f32,
    target_norm: Option<f32>,
    cache: &'a canvas::Cache,
) -> Element<'a, Message> {
    let value = value.into();
    let subtitle = subtitle.into();
    container(
        column![
            container(Space::new().width(Length::Fill).height(3))
                .style(theme::accent_strip_style(accent)),
            column![
                text(label).size(9.5).font(FONT_BOLD).color(TEXT_MUTED),
                text(value).size(18.5).font(FONT_MONO_BOLD).color(accent),
                canvas::Canvas::new(MiniMeterCanvas {
                    value_norm,
                    target_norm,
                    color: accent,
                    cache,
                })
                .width(Length::Fill)
                .height(10),
                text(subtitle).size(10.5).color(TEXT_SECONDARY),
            ]
            .spacing(4)
            .padding([8, 11]),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .style(theme::card_style)
    .into()
}

fn platform_tile<'a>(name: &'a str, target: &'a str, gain_db: f32) -> Element<'a, Message> {
    let (status_str, col, bg) = if gain_db < -3.0 {
        (format!("{:+.1} dB  •  Cut", gain_db), ACCENT_AMBER, ACCENT_AMBER_DIM)
    } else if gain_db < -0.5 {
        (format!("{:+.1} dB  •  Trim", gain_db), ACCENT_BLUE, ACCENT_BLUE_DIM)
    } else if gain_db <= 1.0 {
        (format!("{:+.1} dB  •  Optimal", gain_db), ACCENT_GREEN, ACCENT_GREEN_DIM)
    } else {
        (format!("{:+.1} dB  •  Quiet", gain_db), ACCENT_PURPLE, ACCENT_PURPLE_DIM)
    };

    container(
        column![
            row![
                text(name).size(11.5).font(FONT_BOLD).color(TEXT_PRIMARY),
                Space::new().width(Length::Fill),
                text(target).size(10).font(FONT_MONO).color(TEXT_MUTED),
            ]
            .align_y(Alignment::Center),
            container(text(status_str).size(10.5).font(FONT_MONO_BOLD).color(col))
                .padding([3, 8])
                .style(theme::badge_style(bg, col)),
        ]
        .spacing(5),
    )
    .padding([8, 11])
    .width(Length::Fill)
    .style(theme::card_alt_style)
    .into()
}

fn feature_pill<'a>(title: &'a str, body: &'a str, accent: Color) -> Element<'a, Message> {
    container(
        column![
            container(Space::new().width(Length::Fill).height(3))
                .style(theme::accent_strip_style(accent)),
            column![
                text(title).size(13).font(FONT_BOLD).color(accent),
                text(body).size(11.5).color(TEXT_SECONDARY),
            ]
            .spacing(6)
            .padding(14),
        ],
    )
    .width(Length::Fill)
    .style(theme::card_style)
    .into()
}

fn badge<'a>(label: &'a str, fg: Color, bg: Color) -> Element<'a, Message> {
    container(text(label).size(9.5).font(FONT_BOLD).color(fg))
        .padding([3, 8])
        .style(theme::badge_style(bg, fg))
        .into()
}

fn badge_owned<'a>(label: String, fg: Color, bg: Color) -> Element<'a, Message> {
    container(text(label).size(9.5).font(FONT_MONO_BOLD).color(fg))
        .padding([3, 8])
        .style(theme::badge_style(bg, fg))
        .into()
}

fn meta_pill<'a>(label: String) -> Element<'a, Message> {
    container(text(label).size(10).font(FONT_MONO).color(TEXT_SECONDARY))
        .padding([2, 7])
        .style(theme::meta_pill_style)
        .into()
}

fn metric_line<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![
        text(label).size(11.5).color(TEXT_MUTED),
        Space::new().width(Length::Fill),
        text(value).size(11.5).font(FONT_MONO_BOLD).color(TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn register_bar<'a>(label: &'a str, db: f32, color: Color) -> Element<'a, Message> {
    let norm = ((db + 72.0) / 72.0).clamp(0.0, 1.0);
    column![
        row![
            text(label).size(11.5).font(FONT_SEMIBOLD).color(TEXT_SECONDARY),
            Space::new().width(Length::Fill),
            text(format!("{:.1} dB", db))
                .size(11.5)
                .font(FONT_MONO_BOLD)
                .color(color),
        ],
        progress_bar(0.0..=1.0, norm)
            .girth(8)
            .style(theme::colored_bar_style(color)),
    ]
    .spacing(5)
    .into()
}

fn filter_chip<'a>(
    label: String,
    target: BatchFilter,
    current: BatchFilter,
) -> Element<'a, Message> {
    button(text(label).size(11.5).font(FONT_SEMIBOLD))
        .on_press(Message::SetBatchFilter(target))
        .style(theme::chip_button(target == current))
        .padding([5, 11])
        .into()
}

fn score_colors(score: u8) -> (Color, Color) {
    if score >= 85 {
        (ACCENT_GREEN, ACCENT_GREEN_DIM)
    } else if score >= 70 {
        (ACCENT_BLUE, ACCENT_BLUE_DIM)
    } else if score >= 50 {
        (ACCENT_AMBER, ACCENT_AMBER_DIM)
    } else {
        (ACCENT_RED, ACCENT_RED_DIM)
    }
}

fn verdict_style(v: LosslessVerdict) -> (&'static str, Color, Color) {
    match v {
        LosslessVerdict::GenuineLossless => ("GENUINE LOSSLESS", ACCENT_GREEN, ACCENT_GREEN_DIM),
        LosslessVerdict::SuspectedTranscode320k => ("FAKE ~320k MP3", ACCENT_RED, ACCENT_RED_DIM),
        LosslessVerdict::SuspectedTranscode192k => ("FAKE ~192k MP3", ACCENT_RED, ACCENT_RED_DIM),
        LosslessVerdict::SuspectedTranscode128k => ("FAKE ~128k MP3", ACCENT_RED, ACCENT_RED_DIM),
        LosslessVerdict::BandwidthLimited => ("BANDWIDTH LIMITED", ACCENT_AMBER, ACCENT_AMBER_DIM),
        LosslessVerdict::NativeLossy => ("NATIVE LOSSY", ACCENT_PURPLE, ACCENT_PURPLE_DIM),
    }
}

fn is_fake_verdict(v: LosslessVerdict) -> bool {
    matches!(
        v,
        LosslessVerdict::SuspectedTranscode320k
            | LosslessVerdict::SuspectedTranscode192k
            | LosslessVerdict::SuspectedTranscode128k
    )
}

fn short_codec_label(format_name: &str, codec_name: &str) -> String {
    let f = format_name.to_uppercase();
    let c = codec_name.to_uppercase();
    if c.contains("OPUS") {
        "OPUS".into()
    } else if f.contains("FLAC") || c.contains("FLAC") {
        "FLAC".into()
    } else if f.contains("WAV") || f.contains("RIFF") {
        "WAV".into()
    } else if c.contains("MP3") || f.contains("MP3") {
        "MP3".into()
    } else if c.contains("ALAC") {
        "ALAC".into()
    } else if c.contains("AAC") || f.contains("M4A") {
        "AAC / M4A".into()
    } else if c.contains("VORBIS") {
        "OGG Vorbis".into()
    } else if f.contains("AU") || f.contains("SND") {
        "Sun AU".into()
    } else {
        truncate_str(format_name, 14)
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

pub(crate) fn truncate_filename(name: &str, max_chars: usize) -> String {
    let count = name.chars().count();
    if count <= max_chars {
        return name.to_string();
    }
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();

    let stem = if !ext.is_empty() && name.ends_with(&ext) {
        &name[..name.len() - ext.len()]
    } else {
        name
    };

    let stem_chars: Vec<char> = stem.chars().collect();
    let ext_len = ext.chars().count();
    let avail = max_chars.saturating_sub(ext_len + 1); // 1 char for '…'
    if avail < 6 {
        return truncate_str(name, max_chars);
    }
    let head_len = (avail * 6) / 10;
    let tail_len = avail.saturating_sub(head_len);
    let head: String = stem_chars[..head_len.min(stem_chars.len())].iter().collect();
    let tail: String = stem_chars[stem_chars.len().saturating_sub(tail_len)..].iter().collect();
    format!("{}…{}{}", head, tail, ext)
}

fn stem_of(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layacore")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_filename_short() {
        assert_eq!(truncate_filename("track.flac", 30), "track.flac");
    }

    #[test]
    fn test_truncate_filename_middle() {
        let long_name = "The_Wild_Theme_From_OM_Chapter_1_Udhiram_The_Blood_Wood_Sai_Abhyankkar.flac";
        let truncated = truncate_filename(long_name, 40);
        assert_eq!(truncated.chars().count(), 40);
        assert!(truncated.starts_with("The_Wild_Theme_From"));
        assert!(truncated.ends_with(".flac"));
        assert!(truncated.contains('…'));
    }

    #[test]
    fn test_truncate_filename_no_extension() {
        let name = "very_long_file_without_any_extension_present";
        let truncated = truncate_filename(name, 20);
        assert_eq!(truncated.chars().count(), 20);
        assert!(truncated.contains('…'));
    }
}
