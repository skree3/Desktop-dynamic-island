//! Журнал буфера обмена Windows (Win+V): Windows.ApplicationModel.DataTransfer.Clipboard.
//! Чтение журнала Windows разрешает только приложению, окно которого активно,
//! поэтому список запрашивается, когда открыта карточка (окно панели в фокусе).

use serde::Serialize;
use windows::ApplicationModel::DataTransfer::{
    Clipboard, ClipboardHistoryItem, ClipboardHistoryItemsResultStatus, SetHistoryItemAsContentStatus, StandardDataFormats,
};
use windows::Storage::Streams::DataReader;

use super::apps::run_sta;
use crate::core::error::{AppError, AppResult};
use crate::util::image::bytes_data_url;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WinClipItem {
    pub id: String,
    /// text | image | files | other
    pub kind: String,
    pub preview: String,
    pub created_at: i64,
    pub image: Option<String>,
}

fn items() -> AppResult<Vec<ClipboardHistoryItem>> {
    if !Clipboard::IsHistoryEnabled().unwrap_or(false) {
        return Err(AppError::unsupported("Журнал буфера обмена Windows выключен. Включите его: «Параметры → Система → Буфер обмена» или Win+V."));
    }
    let res = Clipboard::GetHistoryItemsAsync()?.get()?;
    match res.Status()? {
        ClipboardHistoryItemsResultStatus::Success => {}
        ClipboardHistoryItemsResultStatus::AccessDenied => {
            return Err(AppError::PermissionDenied("Windows выдаёт журнал буфера обмена только активному окну — щёлкните по панели и обновите.".into()))
        }
        _ => return Err(AppError::unsupported("Журнал буфера обмена Windows недоступен")),
    }
    let list = res.Items()?;
    let mut out = vec![];
    for i in 0..list.Size()? {
        out.push(list.GetAt(i)?);
    }
    Ok(out)
}

fn read_image(item: &ClipboardHistoryItem) -> Option<String> {
    let reference = item.Content().ok()?.GetBitmapAsync().ok()?.get().ok()?;
    let stream = reference.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 3 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    let n = reader.LoadAsync(size as u32).ok()?.get().ok()?;
    let mut buf = vec![0u8; n as usize];
    reader.ReadBytes(&mut buf).ok()?;
    let mime = if buf.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else if buf.starts_with(b"BM") { "image/bmp" } else { "image/jpeg" };
    Some(bytes_data_url(mime, &buf))
}

fn list_inner(with_images: bool) -> AppResult<Vec<WinClipItem>> {
    let text_fmt = StandardDataFormats::Text()?;
    let bmp_fmt = StandardDataFormats::Bitmap()?;
    let files_fmt = StandardDataFormats::StorageItems()?;
    let mut out = vec![];
    for item in items()? {
        let id = item.Id().map(|h| h.to_string()).unwrap_or_default();
        let created_at = item.Timestamp().map(|t| (t.UniversalTime - 116_444_736_000_000_000) / 10_000).unwrap_or(0);
        let Ok(content) = item.Content() else { continue };
        let (kind, preview, image) = if content.Contains(&text_fmt).unwrap_or(false) {
            let text = content.GetTextAsync().and_then(|op| op.get()).map(|h| h.to_string()).unwrap_or_default();
            let preview: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
            ("text", preview, None)
        } else if content.Contains(&bmp_fmt).unwrap_or(false) {
            ("image", "Изображение".to_string(), if with_images { read_image(&item) } else { None })
        } else if content.Contains(&files_fmt).unwrap_or(false) {
            ("files", "Файлы".to_string(), None)
        } else {
            ("other", "Данные".to_string(), None)
        };
        out.push(WinClipItem { id, kind: kind.into(), preview, created_at, image });
    }
    Ok(out)
}

fn find(id: &str) -> AppResult<ClipboardHistoryItem> {
    items()?
        .into_iter()
        .find(|i| i.Id().map(|h| h.to_string() == id).unwrap_or(false))
        .ok_or_else(|| AppError::NotFound("запись уже удалена из журнала Windows".into()))
}

fn copy_inner(id: &str) -> AppResult<()> {
    match Clipboard::SetHistoryItemAsContent(&find(id)?)? {
        SetHistoryItemAsContentStatus::Success => Ok(()),
        SetHistoryItemAsContentStatus::AccessDenied => Err(AppError::PermissionDenied("Windows отклонил копирование: панель должна быть активным окном".into())),
        _ => Err(AppError::system("элемент журнала удалён")),
    }
}

fn delete_inner(id: &str) -> AppResult<()> {
    if Clipboard::DeleteItemFromHistory(&find(id)?)? {
        Ok(())
    } else {
        Err(AppError::PermissionDenied("Windows не разрешил удалить запись".into()))
    }
}

/// Очищает журнал Win+V (закреплённые в Win+V записи Windows сохраняет).
fn clear_inner() -> AppResult<()> {
    if Clipboard::ClearHistory()? {
        Ok(())
    } else {
        Err(AppError::PermissionDenied("Windows не разрешил очистить журнал буфера обмена".into()))
    }
}

// API буфера обмена Windows (OLE) работает только в STA-потоке.
pub fn list(with_images: bool) -> AppResult<Vec<WinClipItem>> {
    run_sta(move || list_inner(with_images))?
}

pub fn copy(id: &str) -> AppResult<()> {
    let id = id.to_string();
    run_sta(move || copy_inner(&id))?
}

pub fn delete(id: &str) -> AppResult<()> {
    let id = id.to_string();
    run_sta(move || delete_inner(&id))?
}

/// Очищает журнал Win+V (закреплённые в Win+V записи Windows сохраняет).
pub fn clear() -> AppResult<()> {
    run_sta(clear_inner)?
}
