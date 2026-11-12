//! App-wide settings in `Application Support/Knotes/settings-v2.json` (next to, not over, the
//! files of Knotes v1). A file instead of user defaults: tests run it with a throwaway HOME.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use knotes_app::SidebarMode;
use serde_json::{Value, json};

pub const SETTINGS_FILE: &str = "settings-v2.json";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AppSettings {
    /// Vault reopened when the app starts without one.
    pub last_vault: Option<PathBuf>,
    /// Folders or tags under "All notes" in the sidebar.
    pub sidebar_mode: SidebarMode,
}

impl AppSettings {
    /// Missing or broken files give the defaults.
    pub fn load(app_data: &Path) -> Self {
        let Ok(text) = fs::read_to_string(app_data.join(SETTINGS_FILE)) else { return Self::default() };
        let Ok(value) = serde_json::from_str::<Value>(&text) else { return Self::default() };
        Self {
            last_vault: value.get("lastVault").and_then(Value::as_str).map(PathBuf::from),
            sidebar_mode: value.get("sidebar").and_then(Value::as_str).and_then(SidebarMode::from_key).unwrap_or_default(),
        }
    }

    /// Written to a temporary file and renamed, so a crash never leaves half a file.
    pub fn save(&self, app_data: &Path) -> io::Result<()> {
        fs::create_dir_all(app_data)?;
        let value = json!({
            "lastVault": self.last_vault.as_ref().map(|p| p.to_string_lossy()),
            "sidebar": self.sidebar_mode.key(),
        });
        let tmp = app_data.join(format!("{SETTINGS_FILE}.tmp"));
        fs::write(&tmp, serde_json::to_string_pretty(&value).map_err(io::Error::other)? + "\n")?;
        fs::rename(&tmp, app_data.join(SETTINGS_FILE))
    }
}
