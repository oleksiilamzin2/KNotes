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
pub const ASK_NOTES: &str = "Запитайте свої нотатки…";
pub const APP: &str = "Knotes";
pub const MENU_FILE: &str = "Файл";
pub const MENU_EDIT: &str = "Редагування";
pub const MENU_VIEW: &str = "Вигляд";
pub const MENU_WINDOW: &str = "Вікно";
pub const ABOUT: &str = "Про Knotes";
pub const HIDE: &str = "Сховати Knotes";
pub const HIDE_OTHERS: &str = "Сховати інші";
pub const SHOW_ALL: &str = "Показати всі";
pub const QUIT: &str = "Завершити Knotes";
pub const OPEN_VAULT: &str = "Відкрити базу…";
pub const CLOSE_WINDOW: &str = "Закрити вікно";
pub const UNDO: &str = "Скасувати";
pub const REDO: &str = "Повторити";
pub const CUT: &str = "Вирізати";
pub const COPY: &str = "Копіювати";
pub const PASTE: &str = "Вставити";
pub const SELECT_ALL: &str = "Виділити все";
pub const TOGGLE_SIDEBAR: &str = "Показати або сховати бічну панель";
pub const TOGGLE_CHAT: &str = "Показати або сховати чат";
pub const FULL_SCREEN: &str = "Повноекранний режим";
pub const MINIMIZE: &str = "Згорнути";
pub const ZOOM: &str = "Масштабувати";
pub const OPEN: &str = "Відкрити";
pub const SAVE: &str = "Зберегти";
/// Folder for notes created from links when the root `.knotes` names none (ADR 0013).
pub const NEW_NOTES_FOLDER: &str = "Вхідні";
/// File name of a new note until its first line names it (ADR 0011).
pub const UNTITLED: &str = "Без назви";
