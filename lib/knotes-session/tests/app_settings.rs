use std::path::PathBuf;

use knotes_app::SidebarMode;
use knotes_session::AppSettings;

#[test]
fn settings_round_trip_through_the_app_data_folder() {
    let data = tempfile::tempdir().unwrap();
    assert_eq!(AppSettings::load(data.path()), AppSettings::default());
    assert_eq!(AppSettings::default().sidebar_mode, SidebarMode::Folders);
    let settings = AppSettings { last_vault: Some(PathBuf::from("/Users/me/Notes")), sidebar_mode: SidebarMode::Tags };
    settings.save(data.path()).unwrap();
    assert_eq!(AppSettings::load(data.path()), settings);
}

#[test]
fn a_broken_file_means_defaults() {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join(knotes_session::SETTINGS_FILE), "{not json").unwrap();
    assert_eq!(AppSettings::load(data.path()), AppSettings::default());
}
