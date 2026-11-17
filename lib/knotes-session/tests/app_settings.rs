use std::path::PathBuf;

use knotes_app::PanelMode;
use knotes_session::AppSettings;

#[test]
fn settings_round_trip_through_the_app_data_folder() {
    let data = tempfile::tempdir().unwrap();
    assert_eq!(AppSettings::load(data.path()), AppSettings::default());
    assert_eq!(AppSettings::default().panel_mode, PanelMode::Ai);
    let settings = AppSettings {
        last_vault: Some(PathBuf::from("/Users/me/Notes")),
        panel_mode: PanelMode::Outline,
        window_frame: Some([40, 60, 1200, 800]),
        chat_model: Some("gpt-5.5".into()),
        chat_effort: Some("low".into()),
        meaning_model: Some("off".into()),
        device: Some("mac-a1b2c3".into()),
    };
    settings.save(data.path()).unwrap();
    assert_eq!(AppSettings::load(data.path()), settings);
}

#[test]
fn a_broken_file_means_defaults() {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join(knotes_session::SETTINGS_FILE), "{not json").unwrap();
    assert_eq!(AppSettings::load(data.path()), AppSettings::default());
}

#[test]
fn this_mac_s_name_for_its_review_log_is_made_once_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let name = AppSettings::device_name(dir.path());
    assert!(name.starts_with("mac-") && name.len() == 10 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{name}");
    assert_eq!(AppSettings::device_name(dir.path()), name, "the same the next time");
    assert_eq!(AppSettings::load(dir.path()).device.as_deref(), Some(name.as_str()));
}
