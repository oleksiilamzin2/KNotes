//! App-wide settings in `Application Support/Knotes/settings-v2.json` (next to, not over, the
//! files of Knotes v1). A file instead of user defaults: tests run it with a throwaway HOME.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use knotes_app::PanelMode;
use serde_json::{Value, json};

pub const SETTINGS_FILE: &str = "settings-v2.json";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AppSettings {
    /// Vault reopened when the app starts without one.
    pub last_vault: Option<PathBuf>,
    /// The AI or the outline in the right panel.
    pub panel_mode: PanelMode,
    /// Where the window was and how big (x, y, width, height in screen points), to open it
    /// there again.
    pub window_frame: Option<[i64; 4]>,
    /// The model the chat asks (its slug), as last chosen.
    pub chat_model: Option<String>,
    /// How hard the chat's model reasons (`reasoning.effort`), as last chosen; none, the model's
    /// own default.
    pub chat_effort: Option<String>,
    /// The model search by meaning runs on (`MeaningModel::key`), `off`, or none: the default.
    pub meaning_model: Option<String>,
    /// This Mac's name for its review log in a vault (ADR 0037): letters and digits, made once.
    pub device: Option<String>,
}

impl AppSettings {
    /// Missing or broken files give the defaults.
    pub fn load(app_data: &Path) -> Self {
        let Ok(text) = fs::read_to_string(app_data.join(SETTINGS_FILE)) else { return Self::default() };
        let Ok(value) = serde_json::from_str::<Value>(&text) else { return Self::default() };
        Self {
            last_vault: value.get("lastVault").and_then(Value::as_str).map(PathBuf::from),
            panel_mode: value.get("panel").and_then(Value::as_str).and_then(PanelMode::from_key).unwrap_or_default(),
            window_frame: value.get("windowFrame").and_then(Value::as_array).and_then(|v| {
                let n: Vec<i64> = v.iter().filter_map(Value::as_i64).collect();
                <[i64; 4]>::try_from(n).ok()
            }),
            chat_model: value.get("chatModel").and_then(Value::as_str).map(str::to_string),
            chat_effort: value.get("chatEffort").and_then(Value::as_str).map(str::to_string),
            meaning_model: value.get("meaningModel").and_then(Value::as_str).map(str::to_string),
            device: value.get("device").and_then(Value::as_str).filter(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).map(str::to_string),
        }
    }

    /// This Mac's name for its review logs, made (and kept in the settings) the first time.
    pub fn device_name(app_data: &Path) -> String {
        let mut settings = Self::load(app_data);
        if let Some(device) = &settings.device {
            return device.clone();
        }
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64) ^ u64::from(std::process::id());
        let device = format!("mac-{}", &knotes_cards::card_id(seed)[2..]);
        settings.device = Some(device.clone());
        if let Err(e) = settings.save(app_data) {
            eprintln!("knotes: cannot save settings: {e}");
        }
        device
    }

    /// Written to a temporary file and renamed, so a crash never leaves half a file.
    pub fn save(&self, app_data: &Path) -> io::Result<()> {
        fs::create_dir_all(app_data)?;
        let value = json!({
            "lastVault": self.last_vault.as_ref().map(|p| p.to_string_lossy()),
            "panel": self.panel_mode.key(),
            "windowFrame": self.window_frame,
            "chatModel": self.chat_model,
            "chatEffort": self.chat_effort,
            "meaningModel": self.meaning_model,
            "device": self.device,
        });
        let tmp = app_data.join(format!("{SETTINGS_FILE}.tmp"));
        fs::write(&tmp, serde_json::to_string_pretty(&value).map_err(io::Error::other)? + "\n")?;
        fs::rename(&tmp, app_data.join(SETTINGS_FILE))
    }
}
