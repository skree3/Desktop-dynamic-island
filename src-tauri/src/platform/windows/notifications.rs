//! Чтение уведомлений других приложений: Windows.UI.Notifications.Management.UserNotificationListener.
//!
//! Ограничение Windows: API доступен только приложениям с идентификатором пакета
//! (MSIX / sparse package) и capability `userNotificationListener`. Для обычной
//! (unpackaged) сборки система возвращает ошибку или Denied — в этом случае мы
//! честно сообщаем, что функция недоступна, и не имитируем её работу.
//! Внутренние уведомления (таймеры, плагины) работают в любом случае.

use std::collections::HashSet;
use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};
use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds};

use super::ensure_com;
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{ListenerStatus, SystemNotification};

pub struct Listener {
    inner: UserNotificationListener,
    seen: HashSet<u32>,
    icons: std::collections::HashMap<String, Option<String>>,
}

unsafe impl Send for Listener {}

const BACKEND: &str = "Windows UserNotificationListener";

pub fn open() -> Result<Listener, ListenerStatus> {
    ensure_com();
    let fail = |msg: String| ListenerStatus { available: false, active: false, backend: BACKEND.into(), message: msg };
    let listener = UserNotificationListener::Current().map_err(|e| {
        fail(format!(
            "Windows не предоставляет доступ к уведомлениям других приложений для этой сборки ({}). Требуется установка в виде MSIX-пакета с разрешением userNotificationListener.",
            e.message()
        ))
    })?;
    let status = listener
        .RequestAccessAsync()
        .and_then(|op| op.get())
        .map_err(|e| fail(format!("Запрос доступа к уведомлениям отклонён системой: {}. Требуется идентификатор пакета (MSIX).", e.message())))?;
    match status {
        UserNotificationListenerAccessStatus::Allowed => Ok(Listener { inner: listener, seen: HashSet::new(), icons: Default::default() }),
        UserNotificationListenerAccessStatus::Denied => Err(fail(
            "Доступ к уведомлениям запрещён. Разрешите его в «Параметры → Конфиденциальность → Уведомления».".into(),
        )),
        _ => Err(fail("Доступ к уведомлениям не предоставлен".into())),
    }
}

impl Listener {
    pub fn status(&self) -> ListenerStatus {
        ListenerStatus { available: true, active: true, backend: BACKEND.into(), message: "Получение уведомлений Windows активно".into() }
    }

    /// Возвращает уведомления, появившиеся с прошлого вызова.
    pub fn poll(&mut self, first_run: bool) -> AppResult<Vec<SystemNotification>> {
        ensure_com();
        let list = self.inner.GetNotificationsAsync(NotificationKinds::Toast)?.get()?;
        let mut fresh = vec![];
        let mut current = HashSet::new();
        for i in 0..list.Size()? {
            let n = list.GetAt(i)?;
            let id = n.Id()?;
            current.insert(id);
            if self.seen.contains(&id) || first_run {
                continue;
            }
            let (app_name, app_id, icon) = match n.AppInfo() {
                Ok(info) => {
                    let id = info.AppUserModelId().map(|h| h.to_string()).ok();
                    let key = id.clone().unwrap_or_default();
                    let icon = self.icons.entry(key).or_insert_with(|| app_logo(&info)).clone();
                    (info.DisplayInfo().and_then(|d| d.DisplayName()).map(|h| h.to_string()).unwrap_or_default(), id, icon)
                }
                Err(_) => (String::new(), None, None),
            };
            let mut texts: Vec<String> = vec![];
            if let Ok(binding) = n
                .Notification()
                .and_then(|x| x.Visual())
                .and_then(|v| v.GetBinding(&KnownNotificationBindings::ToastGeneric()?))
            {
                if let Ok(elems) = binding.GetTextElements() {
                    for j in 0..elems.Size().unwrap_or(0) {
                        if let Ok(t) = elems.GetAt(j).and_then(|e| e.Text()) {
                            texts.push(t.to_string());
                        }
                    }
                }
            }
            let ts = n.CreationTime().map(|t| (t.UniversalTime - 116_444_736_000_000_000) / 10_000).unwrap_or(0);
            fresh.push(SystemNotification {
                key: id.to_string(),
                app_name: if app_name.is_empty() { "Приложение".into() } else { app_name },
                app_id,
                title: texts.first().cloned().unwrap_or_default(),
                body: texts.iter().skip(1).cloned().collect::<Vec<_>>().join("\n"),
                icon,
                timestamp: ts,
            });
        }
        self.seen = current;
        Ok(fresh)
    }

    pub fn remove(&self, key: &str) -> AppResult<()> {
        let id: u32 = key.parse().map_err(|_| AppError::invalid("id уведомления"))?;
        self.inner.RemoveNotification(id)?;
        Ok(())
    }
}

/// Иконка приложения-отправителя (Telegram, Discord и т.д.) как data URL.
fn app_logo(info: &windows::ApplicationModel::AppInfo) -> Option<String> {
    let reference = info.DisplayInfo().ok()?.GetLogo(windows::Foundation::Size { Width: 48.0, Height: 48.0 }).ok()?;
    let stream = reference.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 512 * 1024 {
        return None;
    }
    let reader = windows::Storage::Streams::DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    let n = reader.LoadAsync(size as u32).ok()?.get().ok()?;
    let mut buf = vec![0u8; n as usize];
    reader.ReadBytes(&mut buf).ok()?;
    let mime = if buf.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" };
    Some(crate::util::image::bytes_data_url(mime, &buf))
}
