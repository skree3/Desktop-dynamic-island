//! Приём файлов, перетащенных на остров (OLE drag-and-drop).
//!
//! Почему не через WebView: курсор над островом указывает на окно Chromium из
//! процесса msedgewebview2, а OLE ищет цель сброса только у него и его родителей
//! в том же процессе. Встроенный обработчик wry снимает (межпроцессный
//! RevokeDragDrop срабатывает) цели Chromium, поставить свою туда не может, и
//! файлы молча отклоняются.
//!
//! Поэтому, пока над островом идёт перетаскивание, начатое за его пределами,
//! поверх острова показывается невидимое окно нашего процесса со своей целью
//! сброса. Отпустили кнопку — окно скрывается.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use windows::core::{implement, w, Ref, Result};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, POINTL, RECT, WPARAM};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetCursorPos, GetSystemMetrics, GetWindowRect, IsWindowVisible, RegisterClassW, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, SM_SWAPBUTTON, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_MOUSEACTIVATE,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

pub enum DropEvent {
    Enter(Vec<PathBuf>),
    Leave,
    Drop(Vec<PathBuf>),
}

/// Обработчик возвращает, принимаем ли перетаскивание (для Enter).
pub type Handler = Box<dyn Fn(DropEvent) -> bool>;

#[implement(IDropTarget)]
struct Target {
    handler: Handler,
    accepted: Cell<bool>,
}

fn paths_of(data: &Ref<'_, IDataObject>) -> Vec<PathBuf> {
    let Some(data) = data.as_ref() else { return vec![] };
    let fmt = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let mut out = vec![];
    unsafe {
        let Ok(mut medium) = data.GetData(&fmt) else { return out };
        let hdrop = HDROP(medium.u.hGlobal.0 as _);
        let count = DragQueryFileW(hdrop, u32::MAX, None);
        for i in 0..count {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            out.push(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
        }
        ReleaseStgMedium(&mut medium);
    }
    out
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, data: Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let paths = paths_of(&data);
        let ok = !paths.is_empty() && (self.handler)(DropEvent::Enter(paths));
        self.accepted.set(ok);
        unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = if self.accepted.get() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        if self.accepted.replace(false) {
            (self.handler)(DropEvent::Leave);
        }
        Ok(())
    }

    fn Drop(&self, data: Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let paths = paths_of(&data);
        let ok = self.accepted.replace(false) && !paths.is_empty();
        if ok {
            (self.handler)(DropEvent::Drop(paths));
        }
        unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }
}

unsafe extern "system" fn catcher_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

struct Catcher {
    hwnd: HWND,
    _target: IDropTarget,
}

thread_local! {
    // Окно-приёмник живёт в главном (UI) потоке: там цикл сообщений и OLE.
    static CATCHER: RefCell<Option<Catcher>> = const { RefCell::new(None) };
}

/// Создаёт окно-приёмник. Вызывать в главном потоке один раз.
pub fn init(handler: Handler) -> bool {
    CATCHER.with(|c| {
        if c.borrow().is_some() {
            return true;
        }
        unsafe {
            let _ = OleInitialize(None);
            let class = w!("DesktopIslandDropCatcher");
            let wc = WNDCLASSW { lpfnWndProc: Some(catcher_proc), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            let Ok(hwnd) = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            ) else {
                return false;
            };
            // Почти прозрачное (альфа 1 из 255): не видно, но курсор и OLE его «видят».
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA);
            let target: IDropTarget = Target { handler, accepted: Cell::new(false) }.into();
            if RegisterDragDrop(hwnd, &target).is_err() {
                return false;
            }
            *c.borrow_mut() = Some(Catcher { hwnd, _target: target });
            true
        }
    })
}

/// Показывает приёмник поверх прямоугольника или прячет его (`None`). Главный поток.
pub fn place(rect: Option<RECT>) {
    CATCHER.with(|c| {
        let Some(c) = c.borrow().as_ref().map(|c| c.hwnd) else { return };
        unsafe {
            match rect {
                Some(r) => {
                    let _ = SetWindowPos(c, Some(HWND_TOPMOST), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                }
                None => {
                    let _ = ShowWindow(c, SW_HIDE);
                }
            }
        }
    });
}

/// Нажата ли основная кнопка мыши (с учётом «левши»).
pub fn primary_button_down() -> bool {
    unsafe {
        let vk = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
        (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0
    }
}

pub fn cursor() -> Option<POINT> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok().map(|_| p) }
}

/// Прямоугольник видимого окна (физические пиксели).
pub fn window_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(hwnd, &mut r).ok().map(|_| r)
    }
}

pub fn contains(r: &RECT, p: &POINT, margin: i32) -> bool {
    p.x >= r.left - margin && p.x < r.right + margin && p.y >= r.top - margin && p.y < r.bottom + margin
}
