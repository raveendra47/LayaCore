use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetStandard {
    SpotifyYouTube,   // -14.0 LUFS, -1.0 dBTP
    AppleMusic,       // -16.0 LUFS, -1.0 dBTP
    BroadcastEbuR128, // -23.0 LUFS, -1.0 dBTP
    AesStreaming,     // -16.0 LUFS, -1.0 dBTP
    ClubCdHot,        // -9.0 LUFS, -0.1 dBTP
}

impl TargetStandard {
    pub const ALL: [TargetStandard; 5] = [
        TargetStandard::SpotifyYouTube,
        TargetStandard::AppleMusic,
        TargetStandard::BroadcastEbuR128,
        TargetStandard::AesStreaming,
        TargetStandard::ClubCdHot,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::SpotifyYouTube => "Spotify / YouTube",
            Self::AppleMusic => "Apple Music",
            Self::BroadcastEbuR128 => "Broadcast EBU R128",
            Self::AesStreaming => "AES TD1004 Streaming",
            Self::ClubCdHot => "Club / CD Hot Master",
        }
    }

    pub fn target_lufs(&self) -> f32 {
        match self {
            Self::SpotifyYouTube => -14.0,
            Self::AppleMusic => -16.0,
            Self::BroadcastEbuR128 => -23.0,
            Self::AesStreaming => -16.0,
            Self::ClubCdHot => -9.0,
        }
    }

    #[allow(dead_code)]
    pub fn ceiling_dbtp(&self) -> f32 {
        match self {
            Self::SpotifyYouTube => -1.0,
            Self::AppleMusic => -1.0,
            Self::BroadcastEbuR128 => -1.0,
            Self::AesStreaming => -1.0,
            Self::ClubCdHot => -0.1,
        }
    }

    pub fn summary_specs(&self) -> &'static str {
        match self {
            Self::SpotifyYouTube => "-14.0 LUFS  •  -1.0 dBTP Ceiling",
            Self::AppleMusic => "-16.0 LUFS  •  -1.0 dBTP Ceiling",
            Self::BroadcastEbuR128 => "-23.0 LUFS  •  -1.0 dBTP (EBU Gated)",
            Self::AesStreaming => "-16.0 LUFS  •  -1.0 dBTP (AES TD1004)",
            Self::ClubCdHot => "-9.0 LUFS  •  -0.1 dBTP (Commercial)",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::SpotifyYouTube => "Target -14.0 LUFS with -1.0 dBTP true peak ceiling (Streaming industry reference).",
            Self::AppleMusic => "Target -16.0 LUFS with -1.0 dBTP true peak ceiling (Apple Sound Check compliance).",
            Self::BroadcastEbuR128 => "Target -23.0 LUFS with ±0.5 LU gating for television, radio, and film broadcast.",
            Self::AesStreaming => "Target -16.0 LUFS recommended by the Audio Engineering Society for web streaming.",
            Self::ClubCdHot => "Aggressive -9.0 LUFS limiter ceiling for high-energy commercial DJ, club, and CD masters.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TruePeakCeiling {
    Minus2_0,
    Minus1_0,
    Minus0_5,
    Minus0_1,
}

impl TruePeakCeiling {
    pub const ALL: [TruePeakCeiling; 4] = [
        TruePeakCeiling::Minus2_0,
        TruePeakCeiling::Minus1_0,
        TruePeakCeiling::Minus0_5,
        TruePeakCeiling::Minus0_1,
    ];

    pub fn dbtp(&self) -> f32 {
        match self {
            Self::Minus2_0 => -2.0,
            Self::Minus1_0 => -1.0,
            Self::Minus0_5 => -0.5,
            Self::Minus0_1 => -0.1,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Minus2_0 => "-2.0 dBTP (Lossy Transcode Safe)",
            Self::Minus1_0 => "-1.0 dBTP (EBU / Streaming Safe)",
            Self::Minus0_5 => "-0.5 dBTP (Balanced Ceiling)",
            Self::Minus0_1 => "-0.1 dBTP (CD / Max Digital Level)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub default_output_dir: Option<PathBuf>,
    pub auto_export_json: bool,
    pub auto_export_csv: bool,
    pub auto_export_png: bool,
    pub target_standard: TargetStandard,
    pub true_peak_ceiling: TruePeakCeiling,
    pub min_dynamic_range_db: f32,
    pub strict_forensics: bool,
    pub default_recursive_scan: bool,
    pub max_batch_threads: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_output_dir: None,
            auto_export_json: false,
            auto_export_csv: false,
            auto_export_png: false,
            target_standard: TargetStandard::SpotifyYouTube,
            true_peak_ceiling: TruePeakCeiling::Minus1_0,
            min_dynamic_range_db: 8.0,
            strict_forensics: false,
            default_recursive_scan: true,
            max_batch_threads: 0,
        }
    }
}

impl AppSettings {
    pub fn config_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let dir = PathBuf::from(appdata).join("LayaCore");
            let _ = std::fs::create_dir_all(&dir);
            dir.join("settings.json")
        } else if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            let dir = PathBuf::from(home).join(".layacore");
            let _ = std::fs::create_dir_all(&dir);
            dir.join("settings.json")
        } else {
            PathBuf::from("layacore_settings.json")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                return settings;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn effective_output_dir(&self) -> PathBuf {
        if let Some(d) = &self.default_output_dir {
            let _ = std::fs::create_dir_all(d);
            return d.clone();
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let p = PathBuf::from(userprofile).join("Documents").join("LayaCore_Reports");
            let _ = std::fs::create_dir_all(&p);
            return p;
        }
        PathBuf::from(".")
    }
}
