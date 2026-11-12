//! В Linux нет счётчика изменений буфера обмена, сопоставимого с Windows:
//! модуль буфера обмена сравнивает хеш содержимого. Пометки менеджеров паролей
//! (x-kde-passwordManagerHint) arboard не предоставляет — работает только
//! эвристический фильтр и список исключений по активному окну (X11).

pub fn sequence() -> Option<u64> {
    None
}

pub fn is_sensitive() -> bool {
    false
}

pub fn owner_app() -> Option<String> {
    super::foreground::current().map(|f| f.exe_name.to_lowercase()).filter(|s| !s.is_empty())
}
