use crate::i18n::{LanguageChoice, Strings};
use crate::model::HealthEvalOptions;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    NativeLight,
    ModernDark,
    #[default]
    CrystalBlue,
    SakuraAoi,
    SakuraNight,
    CyberMidnight,
}

impl ThemeKind {
    pub const ALL: &'static [ThemeKind] = &[
        ThemeKind::NativeLight,
        ThemeKind::ModernDark,
        ThemeKind::CrystalBlue,
        ThemeKind::SakuraAoi,
        ThemeKind::SakuraNight,
        ThemeKind::CyberMidnight,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            ThemeKind::NativeLight => "Native Light (QDiskInfo)",
            ThemeKind::ModernDark => "Modern Dark",
            ThemeKind::CrystalBlue => "Crystal Classic Blue",
            ThemeKind::SakuraAoi => "Sakura Aoi (Stylized Light)",
            ThemeKind::SakuraNight => "Sakura Night (Stylized Dark)",
            ThemeKind::CyberMidnight => "Cyber Midnight",
        }
    }

    pub fn localized_name(self, tr: &Strings) -> &'static str {
        match self {
            ThemeKind::NativeLight => tr.theme_native_light,
            ThemeKind::ModernDark => tr.theme_modern_dark,
            ThemeKind::CrystalBlue => tr.theme_crystal_blue,
            ThemeKind::SakuraAoi => tr.theme_sakura_aoi,
            ThemeKind::SakuraNight => tr.theme_sakura_night,
            ThemeKind::CyberMidnight => tr.theme_cyber_midnight,
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(
            self,
            ThemeKind::ModernDark | ThemeKind::SakuraNight | ThemeKind::CyberMidnight
        )
    }

    pub fn has_stylized_artwork(self) -> bool {
        matches!(self, ThemeKind::SakuraAoi | ThemeKind::SakuraNight)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub language: LanguageChoice,
    pub theme: ThemeKind,
    pub ignore_c4: bool,
    pub hex_raw: bool,
    pub use_fahrenheit: bool,
    pub cyclic_navigation: bool,
    pub use_gb_instead_of_tb: bool,
    pub hide_serial_by_default: bool,
    pub ssd_wear_warning_percent: u8,
    pub temp_warning_ata: i32,
    pub temp_critical_ata: i32,
    pub temp_warning_nvme: i32,
    pub temp_critical_nvme: i32,
    pub auto_refresh_minutes: u32,
    pub show_theme_artwork: bool,
    pub custom_background_path: Option<String>,
    pub panel_opacity: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: LanguageChoice::Auto,
            theme: ThemeKind::CrystalBlue,
            ignore_c4: true,
            hex_raw: true,
            use_fahrenheit: false,
            cyclic_navigation: false,
            use_gb_instead_of_tb: false,
            hide_serial_by_default: true,
            ssd_wear_warning_percent: 10,
            temp_warning_ata: 55,
            temp_critical_ata: 60,
            temp_warning_nvme: 65,
            temp_critical_nvme: 70,
            auto_refresh_minutes: 0,
            show_theme_artwork: true,
            custom_background_path: None,
            panel_opacity: 0.90,
        }
    }
}

impl AppConfig {
    pub fn eval_options(&self) -> HealthEvalOptions {
        HealthEvalOptions {
            ignore_c4: self.ignore_c4,
            ssd_wear_warning_threshold_percent: self.ssd_wear_warning_percent,
            temp_warning_ata: self.temp_warning_ata,
            temp_critical_ata: self.temp_critical_ata,
            temp_warning_nvme: self.temp_warning_nvme,
            temp_critical_nvme: self.temp_critical_nvme,
        }
    }

    pub fn set_ata_temp_thresholds(&mut self, warning: i32, critical: i32) {
        self.temp_warning_ata = warning.clamp(20, 120);
        self.temp_critical_ata = critical.clamp(self.temp_warning_ata, 130);
    }

    pub fn set_nvme_temp_thresholds(&mut self, warning: i32, critical: i32) {
        self.temp_warning_nvme = warning.clamp(20, 120);
        self.temp_critical_nvme = critical.clamp(self.temp_warning_nvme, 130);
    }

    pub fn reset_thresholds(&mut self) {
        let def = Self::default();
        self.ssd_wear_warning_percent = def.ssd_wear_warning_percent;
        self.temp_warning_ata = def.temp_warning_ata;
        self.temp_critical_ata = def.temp_critical_ata;
        self.temp_warning_nvme = def.temp_warning_nvme;
        self.temp_critical_nvme = def.temp_critical_nvme;
    }

    pub fn config_path() -> Option<PathBuf> {
        if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
            let p = PathBuf::from(xdg).join("rdiskinfo").join("config.json");
            return Some(p);
        }
        env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("rdiskinfo")
                .join("config.json")
        })
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str::<Self>(&raw).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
        self.save();
    }
}
