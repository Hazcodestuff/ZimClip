use std::path::Path;
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Registry::*},
    UI::{Shell::*, WindowsAndMessaging::*},
};
pub const CALLBACK: u32 = WM_APP + 61;
pub const OPEN: usize = 61;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn command(path: &Path) -> String {
    format!("\"{}\" --background", path.display())
}
pub fn startup_enabled() -> bool {
    unsafe {
        let mut h = std::ptr::null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut h,
        ) != 0
        {
            return false;
        }
        let mut kind = 0;
        let mut bytes = 4096u32;
        let mut value = vec![0u16; 2048];
        let code = RegQueryValueExW(
            h,
            wide("ZimClip").as_ptr(),
            std::ptr::null(),
            &mut kind,
            value.as_mut_ptr() as _,
            &mut bytes,
        );
        RegCloseKey(h);
        if code != 0 || kind != REG_SZ {
            return false;
        }
        let end = value.iter().position(|x| *x == 0).unwrap_or(value.len());
        let actual = String::from_utf16_lossy(&value[..end]);
        std::env::current_exe().is_ok_and(|p| actual == command(&p))
    }
}
pub fn startup(enable: bool) -> Result<(), String> {
    unsafe {
        let mut h = std::ptr::null_mut();
        let code = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut h,
            std::ptr::null_mut(),
        );
        if code != 0 {
            return Err(format!("Cannot open Windows startup settings ({code})."));
        }
        let result = if enable {
            let path = std::env::current_exe().map_err(|e| e.to_string())?;
            let val = wide(&command(&path));
            RegSetValueExW(
                h,
                wide("ZimClip").as_ptr(),
                0,
                REG_SZ,
                val.as_ptr() as _,
                (val.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(h, wide("ZimClip").as_ptr())
        };
        RegCloseKey(h);
        if result != 0 && !(result == ERROR_FILE_NOT_FOUND && !enable) {
            return Err(format!("Cannot change Windows startup ({result})."));
        }
        Ok(())
    }
}
unsafe fn data(h: HWND) -> NOTIFYICONDATAW {
    let mut n: NOTIFYICONDATAW = std::mem::zeroed();
    n.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = h;
    n.uID = 1;
    n
}
pub unsafe fn install(h: HWND) -> bool {
    let mut n = data(h);
    n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    n.uCallbackMessage = CALLBACK;
    n.hIcon = SendMessageW(h, WM_GETICON, ICON_SMALL as usize, 0) as _;
    if n.hIcon.is_null() {
        n.hIcon = LoadIconW(GetModuleHandleW(std::ptr::null()), 1usize as _);
    }
    let title = wide(&format!("{} - replay running", crate::identity::name()));
    for (i, c) in title.iter().take(127).enumerate() {
        n.szTip[i] = *c;
    }
    let ok = Shell_NotifyIconW(NIM_ADD, &n) != 0;
    SetPropW(h, wide("tray-ready").as_ptr(), ok as usize as _);
    ok
}
pub unsafe fn refresh_identity(h: HWND) {
    if GetPropW(h, wide("tray-ready").as_ptr()).is_null() {
        return;
    }
    let mut n = data(h);
    n.uFlags = NIF_ICON | NIF_TIP;
    n.hIcon = SendMessageW(h, WM_GETICON, ICON_SMALL as usize, 0) as _;
    let text = wide(&format!("{} - replay running", crate::identity::name()));
    for (i, c) in text.iter().take(127).enumerate() {
        n.szTip[i] = *c;
    }
    Shell_NotifyIconW(NIM_MODIFY, &n);
}
pub unsafe fn remove(h: HWND) {
    Shell_NotifyIconW(NIM_DELETE, &data(h));
}
pub unsafe fn reveal(h: HWND) {
    ShowWindow(h, SW_RESTORE);
    SetForegroundWindow(h);
}
pub unsafe fn handle(h: HWND, m: u32, l: isize) -> Option<isize> {
    let restart = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
    if m == restart {
        if !install(h) {
            reveal(h);
        }
        return Some(0);
    }
    if m == WM_MEASUREITEM {
        let d = &mut *(l as *mut windows_sys::Win32::UI::Controls::MEASUREITEMSTRUCT);
        if d.CtlType == windows_sys::Win32::UI::Controls::ODT_MENU {
            d.itemWidth = 300;
            d.itemHeight = if d.itemID == 63 { 12 } else { 38 };
            return Some(1);
        }
    }
    if m == WM_DRAWITEM {
        use windows_sys::Win32::{Graphics::Gdi::*, UI::Controls::*};
        let d = &*(l as *const DRAWITEMSTRUCT);
        if d.CtlType == ODT_MENU {
            let b = CreateSolidBrush(if d.itemState & ODS_SELECTED != 0 {
                crate::ui::border()
            } else {
                crate::ui::panel()
            });
            FillRect(d.hDC, &d.rcItem, b);
            DeleteObject(b);
            if d.itemID == 63 {
                let pen = CreatePen(PS_SOLID, 1, crate::ui::border());
                let old = SelectObject(d.hDC, pen);
                let y = (d.rcItem.top + d.rcItem.bottom) / 2;
                MoveToEx(d.hDC, d.rcItem.left + 16, y, std::ptr::null_mut());
                LineTo(d.hDC, d.rcItem.right - 16, y);
                SelectObject(d.hDC, old);
                DeleteObject(pen);
                return Some(1);
            }
            let mut r = d.rcItem;
            r.left += 32;
            if d.itemState & ODS_CHECKED != 0 {
                let pen = CreatePen(PS_SOLID, 2, crate::ui::accent());
                let old = SelectObject(d.hDC, pen);
                let x = d.rcItem.left + 12;
                let y = (d.rcItem.top + d.rcItem.bottom) / 2;
                MoveToEx(d.hDC, x, y, std::ptr::null_mut());
                LineTo(d.hDC, x + 4, y + 4);
                LineTo(d.hDC, x + 11, y - 5);
                SelectObject(d.hDC, old);
                DeleteObject(pen);
            }
            r.right -= 18;
            let key = GetPropW(h, wide("tray-key").as_ptr()) as u32;
            let paused = !GetPropW(h, wide("tray-paused").as_ptr()).is_null();
            let rec = !GetPropW(h, wide("tray-recording").as_ptr()).is_null();
            let text = match d.itemID {
                1 => format!("Save replay       {}", crate::shortcuts::label(key)),
                10 => if paused {
                    "Resume replay"
                } else {
                    "Pause replay"
                }
                .into(),
                301 => "Open clip".into(),
                302 => "Select clip".into(),
                303 => "Delete clip".into(),
                304 => "Open clips folder".into(),
                305 => "Manage tags...".into(),
                500..=29999 => {
                    if d.itemData != 0 {
                        let mut n = 0;
                        let p = d.itemData as *const u16;
                        while n < 128 && *p.add(n) != 0 {
                            n += 1;
                        }
                        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
                    } else {
                        String::new()
                    }
                }
                3 => if rec {
                    "Exit (stop recording first)"
                } else {
                    "Exit"
                }
                .into(),
                2 => "Open clips".into(),
                61 => "Open app".into(),
                _ if d.itemData != 0 => {
                    let mut n = 0;
                    let p = d.itemData as *const u16;
                    while n < 128 && *p.add(n) != 0 {
                        n += 1;
                    }
                    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
                }
                _ => if rec {
                    "Exit (stop recording first)"
                } else {
                    "Exit"
                }
                .into(),
            };
            let old = SelectObject(
                d.hDC,
                SendMessageW(GetDlgItem(h, 14), WM_GETFONT, 0, 0) as _,
            );
            SetTextColor(
                d.hDC,
                if crate::nav::item_dimmed(d.itemState) {
                    0x888888
                } else {
                    crate::ui::text_color()
                },
            );
            SetBkMode(d.hDC, TRANSPARENT as _);
            DrawTextW(
                d.hDC,
                wide(&text).as_ptr(),
                -1,
                &mut r,
                DT_SINGLELINE | DT_VCENTER,
            );
            SelectObject(d.hDC, old);
            // Submenu item: the system draws its own arrow after this handler returns, in the
            // system colours. Draw a themed chevron and exclude the item from the clip so the
            // system arrow cannot paint over it (documented practice for owner-drawn menus).
            if d.itemID > 29999 && windows_sys::Win32::UI::WindowsAndMessaging::IsMenu(d.itemID as usize as _) != 0 {
                let pen = CreatePen(PS_SOLID, 2, crate::ui::text_color());
                let oldpen = SelectObject(d.hDC, pen);
                let x = d.rcItem.right - 20;
                let y = (d.rcItem.top + d.rcItem.bottom) / 2;
                MoveToEx(d.hDC, x, y - 5, std::ptr::null_mut());
                LineTo(d.hDC, x + 5, y);
                LineTo(d.hDC, x, y + 5);
                SelectObject(d.hDC, oldpen);
                DeleteObject(pen);
                ExcludeClipRect(d.hDC, d.rcItem.left, d.rcItem.top, d.rcItem.right, d.rcItem.bottom);
            }
            return Some(1);
        }
    }
    if m != CALLBACK {
        return None;
    }
    if l as u32 == WM_LBUTTONUP || l as u32 == WM_LBUTTONDBLCLK {
        PostMessageW(h, WM_COMMAND, OPEN, 0);
    }
    if l as u32 == WM_RBUTTONUP || l as u32 == WM_CONTEXTMENU {
        let menu = CreatePopupMenu();
        let paused = !GetPropW(h, wide("tray-paused").as_ptr()).is_null();
        let rec = !GetPropW(h, wide("tray-recording").as_ptr()).is_null();
        // Owner-draw labels are resolved from IDs in WM_DRAWITEM, never from
        // pointers to temporary strings whose storage ends before menu paint.
        for (id, flags) in [
            (1, if paused { MF_GRAYED } else { 0 }),
            (10, if rec { MF_GRAYED } else { 0 }),
            (2, 0),
            (OPEN, 0),
            (63, MF_DISABLED),
            (3, 0),
        ] {
            AppendMenuW(menu, MF_OWNERDRAW | flags, id, std::ptr::null());
        }
        let mut pt = POINT::default();
        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
        SetForegroundWindow(h);
        let cmd = crate::menu_round::track(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, h);
        if cmd > 0 {
            PostMessageW(h, WM_COMMAND, cmd as usize, 0);
        }
        PostMessageW(h, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
    Some(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_command_is_quoted() {
        assert_eq!(
            command(Path::new("C:\\Apps With Spaces\\ZimClip.exe")),
            "\"C:\\Apps With Spaces\\ZimClip.exe\" --background"
        );
    }
}
