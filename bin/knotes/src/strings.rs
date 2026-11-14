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
pub const AI: &str = "ШІ";
pub const TABS: &str = "Вкладки";
pub const CLOSE_TAB: &str = "Закрити вкладку";
pub const CLOSE_OTHER_TABS: &str = "Закрити інші вкладки";
pub const OPEN_IN_NEW_TAB: &str = "Відкрити в новій вкладці";
pub const OUTLINE: &str = "Зміст";
pub const CONNECTIONS: &str = "Зв'язки";
pub const LINKS_HERE: &str = "Посилання сюди";
pub const UNLINKED_MENTIONS: &str = "Згадки без посилань";
pub const OUTGOING_LINKS: &str = "Вихідні";
pub const NOT_THERE: &str = "немає";
pub const MAKE_LINK: &str = "Зробити посиланням";

/// `Ще 12…`: the notes of a connections section not listed yet.
pub fn more_notes(n: usize) -> String {
    format!("Ще {n}…")
}
pub const NO_HEADINGS: &str = "Немає заголовків";
pub const NO_DOCUMENT: &str = "Немає відкритого документа";
pub const FIND_SEARCHING: &str = "Пошук…";
pub const NO_PDF_OUTLINE: &str = "У цьому PDF немає змісту";
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
pub const TOGGLE_PANEL: &str = "Показати або сховати панель";
pub const FULL_SCREEN: &str = "Повноекранний режим";
pub const ZOOM_IN: &str = "Збільшити";
pub const ZOOM_OUT: &str = "Зменшити";
pub const ZOOM_ACTUAL: &str = "Справжній розмір";
pub const MINIMIZE: &str = "Згорнути";
pub const ZOOM: &str = "Масштабувати";
pub const OPEN: &str = "Відкрити";
pub const SAVE: &str = "Зберегти";
/// Folder for notes created from links when the root `.knotes.json` names none (ADR 0013).
pub const NEW_NOTES_FOLDER: &str = "Вхідні";
/// File name of a new note until its first line names it (ADR 0011).
pub const UNTITLED: &str = "Без назви";
/// Sidebar row of notes without any tag.
pub const UNTAGGED: &str = "Без тегів";
/// Placeholder of the search field over the note list.
pub const SEARCH: &str = "Пошук";
/// Search modes over the note list (Obsidian's `file:` and `content:`).
pub const SEARCH_FILES: &str = "Файли";
pub const SEARCH_CONTENT: &str = "Вміст";
/// Asked before a rename changes links in other notes (ADR 0019).
pub const RENAME_UPDATE: &str = "Оновити";
pub const RENAME_KEEP: &str = "Не оновлювати";
pub const CANCEL: &str = "Скасувати";
pub const RENAME_KEEP_EXPLAINED: &str = "Без оновлення ці посилання перестануть вести до нотатки.";

/// "Оновити 3 посилання у 2 нотатках?"
pub fn rename_question(links: usize, notes: usize) -> String {
    let (l, n) = (links as u64, notes as u64);
    let links_word = knotes_app::plural_uk(l, "посилання", "посилання", "посилань");
    let notes_word = knotes_app::plural_uk(n, "нотатці", "нотатках", "нотатках");
    format!("Оновити {links} {links_word} у {notes} {notes_word}?")
}
/// The note list's context menu (ADR 0019).
pub const RENAME_NOTE: &str = "Перейменувати";
pub const MOVE_NOTE: &str = "Перемістити в…";
pub const MOVE: &str = "Перемістити";
/// The vault's root folder in the folder picker.
pub const ROOT_FOLDER: &str = "/";

/// "Перемістити «Note» в теку:"
pub fn move_question(note: &str) -> String {
    format!("Перемістити «{note}» в теку:")
}
/// The open note's name in the toolbar.
pub const NOTE_TITLE: &str = "Назва нотатки";
/// The notice of a conflict (ADR 0011).
pub const CONFLICT_TITLE: &str = "Нотатку змінили ззовні";
pub const CONFLICT_OK: &str = "Гаразд";
pub const CONFLICT_OPEN_COPY: &str = "Відкрити копію";

/// What happened to each version.
pub fn conflict_details(note: &str, copy: &str) -> String {
    format!("Ваші зміни лишилися в «{note}», а версію з диска збережено поруч як «{copy}».")
}
/// Edit menu: the find bar over the note, and the search over the note list.
pub const FIND: &str = "Знайти…";
pub const FIND_NEXT: &str = "Знайти далі";
pub const FIND_PREVIOUS: &str = "Знайти попереднє";
pub const SEARCH_NOTES: &str = "Пошук у нотатках";
pub const ASK_AI: &str = "Запитати ШІ";
/// The find bar over the note.
pub const FIND_PLACEHOLDER: &str = "Знайти в нотатці";
pub const FIND_DONE: &str = "Готово";
pub const FIND_NONE: &str = "Немає";

/// "3 з 12"
pub fn find_count(current: usize, total: usize) -> String {
    format!("{current} з {total}")
}

/// A table row's menu (ADR 0021).
pub const TABLE_ROW_ABOVE: &str = "Вставити рядок вище";
pub const TABLE_ROW_BELOW: &str = "Вставити рядок нижче";
pub const TABLE_ROW_DELETE: &str = "Видалити рядок";
/// A table column's menu.
pub const TABLE_COLUMN_LEFT: &str = "Вставити колонку ліворуч";
pub const TABLE_COLUMN_RIGHT: &str = "Вставити колонку праворуч";
pub const TABLE_COLUMN_DELETE: &str = "Видалити колонку";
pub const TABLE_ALIGN_LEFT: &str = "Вирівняти ліворуч";
pub const TABLE_ALIGN_CENTER: &str = "Вирівняти по центру";
pub const TABLE_ALIGN_RIGHT: &str = "Вирівняти праворуч";
pub const TABLE_SORT_UP: &str = "Сортувати за зростанням";
pub const TABLE_SORT_DOWN: &str = "Сортувати за спаданням";

/// Under a PDF's preview (ADR 0022): "сторінка 3 з 12".
pub fn pdf_page(page: usize, pages: usize) -> String {
    format!("сторінка {page} з {pages}")
}

// The chat (ADR 0027). OpenAI asks for "Continue with ChatGPT" as the sign-in button.
pub const CHAT_SIGN_IN: &str = "Continue with ChatGPT";
pub const CHAT_EXPLAINED: &str = "Відповідає модель OpenAI на вашому плані ChatGPT. Фрагменти нотаток, які вона знаходить, надсилаються в OpenAI разом із розмовою.";
pub const CHAT_WAITING: &str = "Завершіть вхід у браузері…";
pub const CHAT_CANCEL: &str = "Скасувати";
pub const CHAT_NEW: &str = "Нова розмова";
pub const CHAT_UNTITLED: &str = "Без назви";
pub const CHAT_DELETE: &str = "Видалити розмову";
pub const CHAT_SIGN_OUT: &str = "Вийти з ChatGPT";
pub const CHAT_PLAN: &str = "Використовується план ChatGPT";
pub const CHAT_MANAGE: &str = "Керувати";
pub const CHAT_USAGE_URL: &str = "https://chatgpt.com/settings/usage";
pub const CHAT_WELCOME: &str = "Ви використовуєте свій план ChatGPT";
pub const CHAT_WELCOME_EXPLAINED: &str = "Запити чату в Knotes витрачають використання вашого плану ChatGPT. Переглянути й обмежити його можна в налаштуваннях ChatGPT.";
pub const CHAT_GOT_IT: &str = "Зрозуміло";
pub const CHAT_THINKING: &str = "Думає…";
pub const CHAT_COMPACTING: &str = "Стискаю розмову…";
pub const CHAT_COMPACTED: &str = "— вище розмову стиснуто: ШІ пам'ятає її коротко —";
pub const CHAT_STOPPED: &str = "Зупинено.";
pub const CHAT_USAGE_LIMIT: &str = "Ліміт використання плану ChatGPT вичерпано.";
pub const CHAT_MANAGE_USAGE: &str = "Керувати використанням";
pub const CHAT_UNAVAILABLE: &str = "ChatGPT зараз недоступний, спробуйте пізніше.";
pub const CHAT_SIGN_IN_AGAIN: &str = "Потрібно увійти знову.";
pub const CHAT_NOT_ELIGIBLE: &str = "План ChatGPT недоступний для цього акаунта";
pub const CHAT_CUT: &str = "Відповідь обірвалася.";
pub const CHAT_DECLINED: &str = "Вхід скасовано в браузері.";
pub const CHAT_PLAN_OFF: &str = "Використання плану ChatGPT не дозволено. Увійдіть знову й дозвольте його.";

/// How full a conversation is: its size and where it is summarized, in thousands of tokens.
pub fn chat_context(tokens: u64, limit: u64) -> String {
    format!("Розмова: {} тис. з {} тис. токенів", tokens.div_ceil(1000), limit / 1000)
}

/// What a tool call shows in the conversation.
pub fn chat_tool(name: &str, args: &serde_json::Value) -> String {
    let arg = |key: &str| args.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    match name {
        "search" => format!("Пошук: {}", arg("query")),
        "find_by_title" => format!("Пошук назви: {}", arg("query")),
        "read_note" => format!("Читаю {}", arg("path")),
        "read_pdf" => match args.get("from_page").and_then(|v| v.as_u64()) {
            Some(page) => format!("Читаю {}, с. {page}", arg("path")),
            None => format!("Читаю {}", arg("path")),
        },
        "outline" => format!("Зміст {}", arg("path")),
        "links" => format!("Зв'язки {}", arg("path")),
        "list_notes" if !arg("tag").is_empty() => format!("Нотатки з тегом #{}", arg("tag").trim_start_matches('#')),
        "list_notes" if !arg("folder").is_empty() => format!("Нотатки теки {}", arg("folder")),
        "list_notes" => "Список нотаток".to_string(),
        "list_tags" => "Теги".to_string(),
        other => other.to_string(),
    }
}
