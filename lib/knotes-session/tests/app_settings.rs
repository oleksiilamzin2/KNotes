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
        vaults: vec![PathBuf::from("/Users/me/Notes"), PathBuf::from("/Users/me/Work")],
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

#[test]
fn the_vaults_opened_are_kept_the_last_first_until_one_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (std::path::Path::new("/v/a"), std::path::Path::new("/v/b"));
    let mut settings = AppSettings::default();
    settings.remember_vault(a);
    settings.remember_vault(b);
    settings.remember_vault(a);
    assert_eq!((settings.vaults.clone(), settings.last_vault.as_deref()), (vec![a.to_path_buf(), b.to_path_buf()], Some(a)), "once each, the last opened first");
    settings.save(dir.path()).unwrap();
    assert_eq!(AppSettings::load(dir.path()), settings);
    settings.forget_vault(a);
    assert_eq!((settings.vaults.clone(), settings.last_vault.as_deref()), (vec![b.to_path_buf()], Some(b)), "the next launch opens one still listed");
    settings.forget_vault(b);
    assert_eq!((settings.vaults.len(), settings.last_vault.as_deref()), (0, None));
    // Settings written before the list was kept: the last vault is the one known.
    std::fs::write(dir.path().join(knotes_session::SETTINGS_FILE), r#"{"lastVault": "/v/old"}"#).unwrap();
    assert_eq!(AppSettings::load(dir.path()).vaults, [std::path::PathBuf::from("/v/old")]);
}
