//! User-visible strings. Ukrainian for now; localization (uk/en from macOS) comes later.

pub const ALL_NOTES: &str = "Усі нотатки";
pub const FOLDERS: &str = "Теки";
pub const TAGS: &str = "Теги";
pub const TODAY: &str = "Сьогодні";
pub const YESTERDAY: &str = "Вчора";
pub const PREVIOUS_7_DAYS: &str = "Попередні 7 днів";
pub const PREVIOUS_30_DAYS: &str = "Попередні 30 днів";

/// "3 нотатки", "50 000 нотаток": number in the macOS region format, Ukrainian plural.
pub fn notes_count(n: usize, formatted: &str) -> String {
    format!("{formatted} {}", knotes_app::plural_uk(n as u64, "нотатка", "нотатки", "нотаток"))
}
pub const NEW_NOTE: &str = "Нова нотатка";
pub const BACK: &str = "Назад";
pub const FORWARD: &str = "Вперед";
pub const CHAT: &str = "Чат";
pub const LIST_OPTIONS: &str = "Параметри списку";
pub const SORT_BY: &str = "Сортувати за";
pub const SORT_MODIFIED: &str = "Датою зміни";
pub const SORT_TITLE: &str = "Назвою";
pub const GROUP_BY_DATE: &str = "Групувати за датою";
pub const INCLUDE_SUBFOLDERS: &str = "Нотатки з підтек";
