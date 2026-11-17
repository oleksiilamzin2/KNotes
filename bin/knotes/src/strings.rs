//! User-visible strings. Ukrainian for now; localization (uk/en from macOS) comes later.

pub const ALL_NOTES: &str = "Усі нотатки";
pub const FOLDERS: &str = "Теки";
pub const TAGS: &str = "Теги";
/// Cards (ADR 0037): making one from a note, and a deck's review.
pub const MAKE_CARD: &str = "Зробити картку";
pub const CARD_QUESTION: &str = "Нова картка";
pub const CARD_DECK: &str = "Колода (наприклад: мови/англійська)";
pub const CARD_ASKS: &str = "Питання (виділене стане відповіддю)";
pub const CARD_NOTHING_SELECTED: &str = "Нічого не виділено";
pub const CARD_NOTHING_SELECTED_DETAILS: &str = "Виділіть у нотатці текст, який має стати карткою.";
pub const CARD_NOT_MADE: &str = "Не вдалося зробити картку";
pub const CARD_NOT_MADE_DETAILS: &str = "Потрібні колода (літери, цифри, «/») і питання.";
pub const CARD_MOVED: &str = "картку в нотатці змінено; почніть повторення знову";
pub const DECK_SETTINGS: &str = "Налаштування колоди…";
pub const DECK_SETTINGS_DETAILS: &str = "Порожнє поле — як у колоди вище. Запам'ятовування: з якою ймовірністю ви маєте згадати картку, коли вона приходить на повторення (70–97 %); що вище, то частіше повтори. Діє на цю колоду та вкладені.";
pub const DECK_RETENTION: &str = "Запам'ятовування, %";
pub const DECK_NEW_A_DAY: &str = "Нових карток на день";
pub const DECK_SETTINGS_NOT_SAVED: &str = "Не вдалося зберегти налаштування колоди";
/// The settings dialog's title: «Колода «англійська»».
pub fn deck_settings_of(deck: &str) -> String {
    format!("Колода «{deck}»")
}

pub const REVIEW_START: &str = "Почати";
pub const REVIEW_SHOW_ANSWER: &str = "Показати відповідь";
pub const REVIEW_OPEN_NOTE: &str = "Відкрити нотатку";
pub const REVIEW_NOTHING: &str = "У цій колоді зараз нема чого повторювати.";
pub const REVIEW_AGAIN_IN: &str = "10 хв";
pub const GRADES: [&str; 4] = ["Знову", "Важко", "Добре", "Легко"];
/// "12 карток до повторення", "200+ карток до повторення".
pub fn review_waiting(cards: usize, more: bool) -> String {
    let word = knotes_app::plural_uk(cards as u64, "картка", "картки", "карток");
    format!("{cards}{} {word} до повторення", if more { "+" } else { "" })
}
/// "мови/англійська · повторено 2, лишилось 5".
pub fn review_progress(deck: &str, done: usize, left: usize) -> String {
    format!("{deck} · повторено {done}, лишилось {left}")
}
pub fn review_done(reviewed: usize) -> String {
    format!("Готово. Повторено {reviewed} {}.", knotes_app::plural_uk(reviewed as u64, "картку", "картки", "карток"))
}
/// When a card comes back: "3 д", "2 міс", "1,5 р".
pub fn review_interval(days: f64) -> String {
    if days < 30.0 {
        format!("{days:.0} д")
    } else if days < 365.0 {
        format!("{:.0} міс", days / 30.0)
    } else {
        format!("{:.1} р", days / 365.0).replace('.', ",")
    }
}
pub fn review_failed(note: &str) -> String {
    format!("Не вдалося записати повторення картки з «{note}»")
}
/// The sidebar's fourth mode: the decks of the notes' cards (ADR 0037).
pub const CARDS: &str = "Картки";
/// The space switcher at the sidebar's foot (ADR 0030).
pub const SPACE: &str = "Простір";
pub const WHOLE_VAULT: &str = "Уся база";
/// The part of a list, and of the sidebar, outside the window's space.
pub const REST_OF_VAULT: &str = "Решта бази";
pub const SPACE_SETTINGS: &str = "Налаштувати простори…";
/// A folder's menu in the sidebar: its space.
pub const NO_SPACE: &str = "Без простору";
pub const EVERY_SPACE: &str = "У всіх просторах";
pub const NEW_SPACE: &str = "Новий простір…";
pub const NEW_SPACE_QUESTION: &str = "Назва нового простору";
pub const CREATE: &str = "Створити";
/// The recently deleted (ADR 0032).
pub const RECENTLY_DELETED: &str = "Нещодавно видалені";
pub const RESTORE: &str = "Відновити";
pub const DELETE_FOREVER: &str = "Видалити назавжди";
pub const EMPTY_DELETED: &str = "Очистити нещодавно видалені…";

/// The settings' «Видалені» tab.
pub const SETTINGS_TRASH: &str = "Видалені";
pub const TRASH_KEEP: &str = "Зберігати видалене";
pub const TRASH_DAYS: &str = "днів";
pub const TRASH_NOTE: &str = "Видалені нотатки, PDF і теки лежать у «Нещодавно видалених» (у прихованій теці .knotes/trash самої бази) і звідти відновлюються. Коли мине цей строк, вони видаляються назавжди.";

/// "Нещодавно видалені бази «Дім»".
pub fn trash_of(vault: &str) -> String {
    format!("Нещодавно видалені бази «{vault}»")
}

/// Where a deleted note was: "з теки «Work»", or the vault's root.
pub fn deleted_from(folder: Option<&str>) -> String {
    match folder {
        Some(folder) => format!("з теки «{folder}»"),
        None => "з кореня бази".to_owned(),
    }
}

/// A deleted folder: "тека · 3 нотатки".
pub fn deleted_folder(notes: usize) -> String {
    format!("тека · {notes} {}", knotes_app::plural_uk(notes as u64, "нотатка", "нотатки", "нотаток"))
}

/// A deleted folder chosen in their list: "Тека «Work» · 3 нотатки. Відновіть її, щоб відкрити."
pub fn deleted_folder_shown(name: &str, notes: usize) -> String {
    format!("Тека «{name}» · {notes} {}. Відновіть її, щоб відкрити.", knotes_app::plural_uk(notes as u64, "нотатка", "нотатки", "нотаток"))
}

/// "Видалити теку «Work» і 3 нотатки?"
pub fn delete_folder_question(name: &str, notes: usize) -> String {
    format!("Видалити теку «{name}» і {notes} {}?", knotes_app::plural_uk(notes as u64, "нотатка", "нотатки", "нотаток"))
}

/// "Їх можна буде відновити з «Нещодавно видалених» протягом 30 днів."
pub fn delete_kept_note(days: u32) -> String {
    format!("Їх можна буде відновити з «Нещодавно видалених» протягом {days} {}.", knotes_app::plural_uk(u64::from(days), "день", "дні", "днів"))
}

/// "Видалити назавжди 3 записи? Цього не можна скасувати."
pub fn empty_deleted_question(entries: usize) -> String {
    format!("Видалити назавжди {entries} {}? Цього не можна скасувати.", knotes_app::plural_uk(entries as u64, "запис", "записи", "записів"))
}

/// "2 записи": the recently deleted list's count.
pub fn entries_count(n: usize) -> String {
    format!("{n} {}", knotes_app::plural_uk(n as u64, "запис", "записи", "записів"))
}

/// A new folder's name, and the commands that make one.
pub const NEW_FOLDER: &str = "Нова тека";
pub const NEW_SUBFOLDER: &str = "Нова підтека";
/// The settings' «Простори» tab (ADR 0030).
pub const SETTINGS_SPACES: &str = "Простори";
pub const SPACES_NOTE: &str = "Простори бази зберігаються в ній самій (у файлі .knotes.json у її корені) і їдуть разом з нею. Теку додають до простору з її меню в бічній панелі.";
pub const SPACES_NO_VAULT: &str = "Відкрийте базу, щоб налаштувати її простори.";
pub const ADD_SPACE: &str = "Додати простір";
pub const DELETE: &str = "Видалити";
/// A note's, a file's or a folder's menu (ADR 0036).
pub const KEEP_FROM_AI: &str = "Сховати від ШІ";
pub const SHOW_TO_AI: &str = "Показати ШІ";
/// The mark of what the AI never sees, as a tooltip and to VoiceOver.
pub const KEPT_FROM_AI: &str = "Сховано від ШІ";
/// The date under a document's name: when its file was last changed.
pub const NOTE_CHANGED: &str = "Змінено";
/// Why a note read only cannot be kept from the AI or shown to it (its tag cannot be written).
pub const READ_ONLY_TAG: &str = "Нотатку відкрито лише для читання, тож тег #no-ai у неї не записати.";
pub const NEW_SPACE_NAME: &str = "Новий простір";
pub const DELETE_SPACE_NOTE: &str = "Його теки й нотатки лишаться на місці, лише вийдуть із простору.";

/// "Простори бази «Дім»".
pub fn spaces_of(vault: &str) -> String {
    format!("Простори бази «{vault}»")
}

/// "Видалити простір «Робота»?"
pub fn delete_space_question(name: &str) -> String {
    format!("Видалити простір «{name}»?")
}

/// A space's colour as the settings name it.
pub fn color_name(color: knotes_spaces::SpaceColor) -> &'static str {
    use knotes_spaces::SpaceColor;
    match color {
        SpaceColor::Blue => "Синій",
        SpaceColor::Purple => "Фіолетовий",
        SpaceColor::Pink => "Рожевий",
        SpaceColor::Red => "Червоний",
        SpaceColor::Orange => "Помаранчевий",
        SpaceColor::Yellow => "Жовтий",
        SpaceColor::Green => "Зелений",
        SpaceColor::Teal => "Бірюзовий",
        SpaceColor::Gray => "Сірий",
        SpaceColor::Custom(..) => OWN_COLOR,
    }
}

/// A space's colour of one's own, and the item choosing one in the colour panel.
pub const OWN_COLOR: &str = "Свій колір";
pub const CHOOSE_COLOR: &str = "Свій колір…";
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
/// An embedded note's button (ADR 0034).
pub const OPEN_EMBED: &str = "Відкрити нотатку";
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
pub const SETTINGS: &str = "Налаштування…";
pub const SETTINGS_SEARCH: &str = "Пошук";
pub const SETTINGS_MEANING: &str = "Пошук за змістом:";
pub const SETTINGS_MEANING_E5: &str = "e5-large-instruct — точніший";
pub const SETTINGS_MEANING_GEMMA: &str = "EmbeddingGemma 2 — легший";
pub const SETTINGS_MEANING_OFF: &str = "Вимкнено";
pub const SETTINGS_CACHE: &str = "Збережені вектори:";
pub const SETTINGS_CACHE_NONE: &str = "немає";
pub const SETTINGS_CLEAR: &str = "Очистити";
pub const SETTINGS_REGENERATE: &str = "Перегенерувати";
pub const SETTINGS_VECTORS_READY: &str = "Вектори актуальні";
pub const SETTINGS_MEANING_IS_OFF: &str = "Пошук за змістом вимкнено";
/// The files search by meaning needs, not there.
pub const MEANING_HELPER_MISSING: &str = "поруч із застосунком немає knotes-embed";
pub fn meaning_model_missing(file: &str) -> String {
    format!("немає файлу моделі «{file}» (його завантажує scripts/setup.sh)")
}
/// Search by meaning's model failing (it does not start, will not load, or dies): why.
pub fn settings_meaning_failing(why: &str) -> String {
    format!("Пошук за змістом не працює: {why}")
}

/// The vectors being made: notes done of all, and blocks a second.
pub fn settings_making(done: usize, total: usize, per_second: f64) -> String {
    format!("Генерація векторів: {done} з {total} нотаток · {per_second:.0} блоків/с")
}
pub const SETTINGS_CACHE_E5: &str = "e5-large-instruct";
pub const SETTINGS_CACHE_GEMMA: &str = "EmbeddingGemma 2";

/// A size on disk in bytes, as the settings show it: "152 КБ", "1,4 МБ".
pub fn file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    let (value, unit) = if b < KB * KB { (b / KB, "КБ") } else if b < KB * KB * KB { (b / KB / KB, "МБ") } else { (b / KB / KB / KB, "ГБ") };
    let text = if value < 10.0 && unit != "КБ" { format!("{value:.1}") } else { format!("{:.0}", value.max(1.0)) };
    format!("{} {unit}", text.replace('.', ","))
}

pub const SETTINGS_MEANING_NOTE: &str = "Модель працює лише на цьому Mac: нотатки нікуди не надсилаються. Після перемикання вектори нотаток перераховуються у фоні, а повернення до вже обробленої моделі миттєве.";
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
pub const SEARCH_MEANING: &str = "За змістом";
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

/// A note's edits could not be written.
pub fn save_failed(note: &str) -> String {
    format!("Не вдалося зберегти «{note}»")
}

/// Said by autosave, once: the edits are not lost yet.
pub fn save_failed_details(error: &str) -> String {
    format!("{error}. Зміни лишаються у вікні, Knotes спробує зберегти їх ще раз.")
}

/// Asked before leaving a note whose edits could not be written.
pub fn unsaved_details(error: &str) -> String {
    format!("{error}. Якщо піти з нотатки зараз, незбережені зміни буде втрачено.")
}
pub const STAY: &str = "Залишитися";
pub const DISCARD_CHANGES: &str = "Відкинути зміни";

/// A note not in UTF-8, shown read only.
pub fn not_utf8(note: &str) -> String {
    format!("«{note}» збережено не в UTF-8")
}
pub const NOT_UTF8_DETAILS: &str =
    "Knotes показує її лише для читання, щоб не зіпсувати файл. Щоб редагувати її тут, збережіть її в UTF-8 в іншому редакторі.";

/// A note that could not be read, shown read only and empty.
pub fn unreadable(note: &str) -> String {
    format!("Не вдалося прочитати «{note}»")
}
pub fn unreadable_details(error: &str) -> String {
    format!("{error}. Нотатку відкрито лише для читання, щоб набране не замінило файл.")
}

/// The note open went from the disk with edits not written yet.
pub const GONE_TITLE: &str = "Нотатку видалили або перенесли поза Knotes";
pub fn gone_details(copy: &str) -> String {
    format!("Незбережені зміни збережено поруч як «{copy}».")
}

/// File operations that failed; a sheet says why (`failed_files` for several).
pub fn delete_failed(name: &str) -> String {
    format!("Не вдалося видалити «{name}»")
}
pub fn restore_failed(name: &str) -> String {
    format!("Не вдалося відновити «{name}»")
}
pub const PURGE_FAILED: &str = "Не вдалося видалити назавжди";
pub fn rename_failed(name: &str) -> String {
    format!("Не вдалося перейменувати «{name}»")
}
pub fn move_failed(name: &str) -> String {
    format!("Не вдалося перемістити «{name}»")
}
pub const MOVES_FAILED: &str = "Не вдалося перемістити частину файлів";
pub const LINKS_FAILED: &str = "Не вдалося оновити всі посилання";
pub fn links_unknown(name: &str) -> String {
    format!("Не вдалося знайти посилання на «{name}»")
}
pub fn links_unknown_details(error: &str) -> String {
    format!("{error}. Нічого не перейменовано, щоб посилання не зламалися.")
}
pub fn name_failed(name: &str) -> String {
    format!("Не вдалося назвати нотатку «{name}»")
}
pub const NEW_NOTE_FAILED: &str = "Не вдалося створити нотатку";
pub const NEW_FOLDER_FAILED: &str = "Не вдалося створити теку";
pub fn create_failed(name: &str) -> String {
    format!("Не вдалося створити «{name}»")
}
pub fn attach_failed(name: &str) -> String {
    format!("Не вдалося додати «{name}»")
}
pub fn mention_failed(name: &str) -> String {
    format!("Не вдалося зробити посилання в «{name}»")
}
pub fn folder_space_failed(name: &str) -> String {
    format!("Не вдалося змінити простір теки «{name}»")
}
/// A file kept from the AI not moved: its new folder's rules cannot say it is kept.
pub fn ai_not_moved(name: &str) -> String {
    format!("«{name}» не переміщено, щоб його не побачив ШІ")
}
pub fn ai_rules_unwritable(file: &str, error: &str) -> String {
    format!("Його сховано від ШІ, а «{file}» там, куди він іде, не вдається змінити: {error}")
}
/// Moved, but what kept it from the AI could not go with it.
pub fn ai_not_carried(names: &str) -> String {
    format!("«{names}» більше не сховано від ШІ")
}
/// "Не вдалося сховати «Щоденник» від ШІ", or show it.
pub fn ai_failed(name: &str, keep: bool) -> String {
    if keep { format!("Не вдалося сховати «{name}» від ШІ") } else { format!("Не вдалося показати «{name}» ШІ") }
}
pub const SPACES_FAILED: &str = "Не вдалося зберегти простори";
pub const TRASH_DAYS_FAILED: &str = "Не вдалося зберегти, скільки днів зберігати видалене";
pub fn open_failed(name: &str) -> String {
    format!("Не вдалося відкрити «{name}»")
}
/// Why a PDF does not open (ADR 0022).
pub const PDF_LOCKED: &str = "Цей PDF захищено паролем, а такі Knotes не відкриває. Відкрийте його в Перегляді.";
pub const PDF_DAMAGED: &str = "Файл пошкоджений або це не PDF.";
pub const VAULT_BUSY: &str = "Цю базу вже відкрито в іншому вікні або в іншому запущеному Knotes.";
/// Why, and the files it was about: "Permission denied. «a», «b»".
pub fn failed_files(error: &str, names: &[&str]) -> String {
    if names.is_empty() {
        return error.to_owned();
    }
    let names: Vec<String> = names.iter().map(|n| format!("«{n}»")).collect();
    format!("{error}. {}", names.join(", "))
}

/// Indexing failed: what is shown is what was indexed before.
pub const INDEX_FAILED: &str = "Не вдалося оновити індекс бази";
pub fn index_failed_details(error: &str) -> String {
    format!("{error}. Список і пошук показують те, що було проіндексовано раніше; Knotes спробує ще раз із наступною зміною.")
}
/// The vault cannot be watched: changes made outside are not seen.
pub const NOT_WATCHED: &str = "Knotes не бачить змін, зроблених поза ним";
pub fn not_watched_details(error: &str) -> String {
    format!("{error}. Базу проіндексовано, але зміни з інших програм з'являться, лише коли ви відкриєте її знову.")
}

/// A list option not saved: the folder's rules file does not parse, or cannot be written.
pub const LIST_OPTION_NOT_SAVED: &str = "Налаштування списку не збережено";
pub fn rules_file_error(file: &str, error: &str) -> String {
    format!("Файл «{file}»: {error}. Виправте його, і налаштування можна буде змінити.")
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
/// The find bar's «Aa».
pub const FIND_IGNORE_CASE: &str = "Ігнорувати регістр";
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

pub const CHAT_EFFORT_TIP: &str = "Скільки модель міркує перед відповіддю: довше — ретельніше, але повільніше й більше з ліміту плану";

/// What the menu calls a reasoning effort; one it does not know, as it is.
pub fn chat_effort(effort: &str) -> String {
    match effort {
        "none" => "Без роздумів",
        "minimal" => "Мінімальні роздуми",
        "low" => "Короткі роздуми",
        "medium" => "Звичайні роздуми",
        "high" => "Довгі роздуми",
        "xhigh" => "Найдовші роздуми",
        "max" => "Максимальні роздуми",
        "ultra" => "Ультра",
        other => other,
    }
    .to_string()
}

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
        "related" => format!("Пов'язане з {}", arg("path")),
        "list_tasks" if !arg("folder").is_empty() => format!("Справи в теці {}", arg("folder")),
        "list_tasks" if arg("state") == "done" => "Виконані справи".to_string(),
        "list_tasks" => "Незакриті справи".to_string(),
        "list_notes" if !arg("tag").is_empty() => format!("Нотатки з тегом #{}", arg("tag").trim_start_matches('#')),
        "list_notes" if !arg("folder").is_empty() => format!("Нотатки теки {}", arg("folder")),
        "list_notes" => "Список нотаток".to_string(),
        "list_tags" => "Теги".to_string(),
        other => other.to_string(),
    }
}
