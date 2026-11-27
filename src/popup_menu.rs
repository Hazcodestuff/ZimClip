//! App-drawn popup menus with rounded corners.
//!
//! The system menu window cannot be reshaped reliably on current Windows (its frame stays
//! square). So the menu is its own small window: same `HMENU` input as `TrackPopupMenu`, same
//! command result, and the items are still painted by the owner window's existing owner-draw
//! code (WM_MEASUREITEM / WM_DRAWITEM with ODT_MENU), so the look of the items is unchanged.
//!
//! Supported: mouse hover and click, submenus (opened on hover, Right or Enter), Up/Down/Home/End,
//! Enter/Space, Left and Esc, click outside or losing focus to close, disabled items, check marks,
//! separators. Not supported: accelerator mnemonics (nothing in the app uses them in these menus).
use std::cell::RefCell;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODA_DRAWENTIRE, ODS_CHECKED, ODS_DISABLED, ODS_GRAYED, ODS_SELECTED, ODT_MENU},
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::*,
    },
};

use crate::layout_plan::{menu_place as place, menu_stack as stack_heights, menu_submenu_x as submenu_x, MENU_PAD as PAD};
const INSET: i32 = 4;
const RADIUS: i32 = 12;
const SEPARATOR_H: i32 = 12;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

#[derive(Clone)]
struct Item {
    id: u32,
    data: usize,
    separator: bool,
    disabled: bool,
    checked: bool,
    sub: HMENU,
    /// Rect inside the level's window (client coordinates).
    rect: RECT,
}

struct Level {
    hwnd: HWND,
    /// Screen rect of the window.
    screen: RECT,
    items: Vec<Item>,
    hot: Option<usize>,
    /// Index of the item in the parent level that opened this one.
    parent_item: Option<usize>,
}

struct State {
    owner: HWND,
    levels: Vec<Level>,
    result: i32,
    done: bool,
}

thread_local! { static ST: RefCell<Option<State>> = const { RefCell::new(None) }; }

unsafe fn collect(menu: HMENU) -> Vec<Item> {
    let n = GetMenuItemCount(menu).max(0);
    let mut items = Vec::new();
    for i in 0..n {
        let mut mii: MENUITEMINFOW = std::mem::zeroed();
        mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
        mii.fMask = MIIM_ID | MIIM_STATE | MIIM_SUBMENU | MIIM_FTYPE | MIIM_DATA;
        if GetMenuItemInfoW(menu, i as u32, 1, &mut mii) == 0 {
            continue;
        }
        let sub = mii.hSubMenu;
        items.push(Item {
            // Like the system: a submenu item reports its menu handle as the item id.
            id: if sub.is_null() { mii.wID } else { sub as usize as u32 },
            data: mii.dwItemData,
            separator: mii.fType & MFT_SEPARATOR != 0,
            disabled: mii.fState & (MFS_GRAYED | MFS_DISABLED) != 0,
            checked: mii.fState & MFS_CHECKED != 0,
            sub,
            rect: RECT { left: 0, top: 0, right: 0, bottom: 0 },
        });
    }
    items
}

/// Measures the items through the owner's existing WM_MEASUREITEM and lays them out.
/// Returns the window size.
unsafe fn measure(owner: HWND, items: &mut [Item]) -> (i32, i32) {
    let mut width = 160;
    let mut heights = Vec::new();
    for it in items.iter() {
        if it.separator {
            heights.push(SEPARATOR_H);
            continue;
        }
        let mut m: MEASUREITEMSTRUCT = std::mem::zeroed();
        m.CtlType = ODT_MENU;
        m.itemID = it.id;
        m.itemData = it.data;
        let handled = SendMessageW(owner, WM_MEASUREITEM, 0, &mut m as *mut _ as LPARAM);
        let (w, h) = if handled != 0 && m.itemHeight > 0 { (m.itemWidth as i32, m.itemHeight as i32) } else { (220, 30) };
        width = width.max(w.min(420) + 2 * INSET);
        heights.push(h);
    }
    let (spans, height) = stack_heights(&heights);
    for (it, (top, bottom)) in items.iter_mut().zip(spans) {
        it.rect = RECT { left: INSET, top, right: width - INSET, bottom };
    }
    (width, height)
}

unsafe fn work_area(x: i32, y: i32) -> (i32, i32, i32, i32) {
    let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(mon, &mut info) != 0 {
        (info.rcWork.left, info.rcWork.top, info.rcWork.right, info.rcWork.bottom)
    } else {
        (0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
    }
}

unsafe fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let cls = wide("ZimClipPopupMenu");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: GetModuleHandleW(std::ptr::null()),
            lpszClassName: cls.as_ptr(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
    });
}

/// Creates and shows one menu window at the given screen position.
unsafe fn create_level(owner: HWND, items: Vec<Item>, size: (i32, i32), at: (i32, i32), activate: bool) -> Level {
    register();
    let (w, h) = size;
    let (x, y) = at;
    let cls = wide("ZimClipPopupMenu");
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        cls.as_ptr(),
        wide("").as_ptr(),
        WS_POPUP,
        x,
        y,
        w,
        h,
        owner,
        std::ptr::null_mut(),
        GetModuleHandleW(std::ptr::null()),
        std::ptr::null(),
    );
    let region = CreateRoundRectRgn(0, 0, w + 1, h + 1, RADIUS * 2, RADIUS * 2);
    if !region.is_null() && SetWindowRgn(hwnd, region, 0) == 0 {
        DeleteObject(region);
    }
    ShowWindow(hwnd, if activate { SW_SHOW } else { SW_SHOWNOACTIVATE });
    Level { hwnd, screen: RECT { left: x, top: y, right: x + w, bottom: y + h }, items, hot: None, parent_item: None }
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    ST.with(|s| s.borrow_mut().as_mut().map(f))
}

unsafe fn close_to(n: usize) {
    let gone: Vec<HWND> = with(|s| s.levels.drain(n.min(s.levels.len())..).map(|l| l.hwnd).collect()).unwrap_or_default();
    for h in gone {
        DestroyWindow(h);
    }
}

unsafe fn open_sub(level: usize, index: usize) {
    close_to(level + 1);
    let Some((owner, sub, parent, item_top)) = with(|s| {
        let l = &s.levels[level];
        (s.owner, l.items[index].sub, l.screen, l.items[index].rect.top)
    }) else {
        return;
    };
    if sub.is_null() {
        return;
    }
    let mut items = collect(sub);
    if items.is_empty() {
        return;
    }
    let (w, h) = measure(owner, &mut items);
    let work = work_area(parent.right, parent.top + item_top);
    let x = submenu_x(parent.left, parent.right, w, work.2);
    let at = place(x, parent.top + item_top - PAD, w, h, work);
    let first = items.iter().position(selectable);
    let mut lv = create_level(owner, items, (w, h), at, false);
    lv.hot = first;
    lv.parent_item = Some(index);
    with(|s| s.levels.push(lv));
}

fn selectable(it: &Item) -> bool {
    !it.separator && !it.disabled
}

unsafe fn set_hot(level: usize, hot: Option<usize>) {
    let hwnd = with(|s| {
        let l = s.levels.get_mut(level)?;
        if l.hot == hot {
            None
        } else {
            l.hot = hot;
            Some(l.hwnd)
        }
    })
    .flatten();
    if let Some(h) = hwnd {
        InvalidateRect(h, std::ptr::null(), 0);
    }
}

/// Mouse position (screen) -> (level, item). The deepest level under the point wins.
fn hit(pt: POINT) -> Option<(usize, Option<usize>)> {
    with(|s| {
        for (li, l) in s.levels.iter().enumerate().rev() {
            let r = l.screen;
            if pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom {
                let (cx, cy) = (pt.x - r.left, pt.y - r.top);
                let item = l.items.iter().position(|i| cx >= i.rect.left && cx < i.rect.right && cy >= i.rect.top && cy < i.rect.bottom);
                return Some((li, item));
            }
        }
        None
    })
    .flatten()
}

fn finish(result: i32) {
    with(|s| {
        s.result = result;
        s.done = true;
    });
}

unsafe fn activate(level: usize, index: usize) {
    let Some(it) = with(|s| s.levels.get(level).and_then(|l| l.items.get(index).cloned())).flatten() else { return };
    if !selectable(&it) {
        return;
    }
    if !it.sub.is_null() {
        open_sub(level, index);
    } else {
        finish(it.id as i32);
    }
}

unsafe fn key(vk: u32) {
    let Some((top, hot, items)) = with(|s| {
        let n = s.levels.len() - 1;
        (n, s.levels[n].hot, s.levels[n].items.clone())
    }) else {
        return;
    };
    let count = items.len() as i32;
    let step = |from: Option<usize>, dir: i32| -> Option<usize> {
        let mut i = match from {
            Some(i) => i as i32,
            None => {
                if dir > 0 {
                    -1
                } else {
                    count
                }
            }
        };
        for _ in 0..count {
            i = (i + dir + count) % count;
            if selectable(&items[i as usize]) {
                return Some(i as usize);
            }
        }
        from
    };
    match vk {
        v if v == VK_DOWN as u32 => set_hot(top, step(hot, 1)),
        v if v == VK_UP as u32 => set_hot(top, step(hot, -1)),
        v if v == VK_HOME as u32 => set_hot(top, step(None, 1)),
        v if v == VK_END as u32 => set_hot(top, step(None, -1)),
        v if v == VK_RETURN as u32 || v == VK_SPACE as u32 => {
            if let Some(h) = hot {
                activate(top, h);
            }
        }
        v if v == VK_RIGHT as u32 => {
            if let Some(h) = hot {
                if !items[h].sub.is_null() {
                    activate(top, h);
                }
            }
        }
        v if v == VK_LEFT as u32 => {
            if top > 0 {
                close_to(top);
            }
        }
        v if v == VK_ESCAPE as u32 => {
            if top > 0 {
                close_to(top);
            } else {
                finish(0);
            }
        }
        _ => {}
    }
}

unsafe fn mouse_move(pt: POINT) {
    let Some((li, item)) = hit(pt) else { return };
    // Pointing into a shallower menu closes the deeper ones, unless the pointer is on the item that opened them.
    let opener = with(|s| s.levels.get(li + 1).and_then(|l| l.parent_item)).flatten();
    if with(|s| s.levels.len() > li + 1).unwrap_or(false) && (item.is_none() || item != opener) {
        close_to(li + 1);
    }
    let hot = item.filter(|i| with(|s| selectable(&s.levels[li].items[*i])).unwrap_or(false));
    set_hot(li, hot);
    if let Some(i) = hot {
        let (is_sub, is_open) = with(|s| (!s.levels[li].items[i].sub.is_null(), s.levels.len() > li + 1)).unwrap_or((false, false));
        if is_sub && !is_open {
            open_sub(li, i);
        }
    }
}

unsafe fn screen_point(h: HWND, l: LPARAM) -> POINT {
    let mut pt = POINT { x: l as i16 as i32, y: (l >> 16) as i16 as i32 };
    ClientToScreen(h, &mut pt);
    pt
}

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => 1,
        WM_SETCURSOR => {
            SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
            1
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_PAINT => {
            paint(h);
            0
        }
        WM_MOUSEMOVE => {
            mouse_move(screen_point(h, l));
            0
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            if hit(screen_point(h, l)).is_none() {
                finish(0);
            }
            0
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            if let Some((li, Some(i))) = hit(screen_point(h, l)) {
                activate(li, i);
            }
            0
        }
        // Losing focus, losing the mouse capture or a cancel request all end the menu.
        WM_ACTIVATE if (w & 0xffff) as u32 == WA_INACTIVE => {
            finish(0);
            0
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => {
            finish(0);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

unsafe fn paint(h: HWND) {
    let snapshot = with(|s| {
        let l = s.levels.iter().find(|l| l.hwnd == h)?;
        Some((s.owner, l.items.clone(), l.hot, l.screen.right - l.screen.left, l.screen.bottom - l.screen.top))
    })
    .flatten();
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(h, &mut ps);
    let Some((owner, items, hot, w, ht)) = snapshot else {
        EndPaint(h, &ps);
        return;
    };
    let mem = CreateCompatibleDC(dc);
    let bmp = CreateCompatibleBitmap(dc, w, ht);
    let old = SelectObject(mem, bmp as _);
    let panel = crate::ui::panel();
    let border = crate::ui::border();
    let full = RECT { left: 0, top: 0, right: w, bottom: ht };
    let b = CreateSolidBrush(panel);
    FillRect(mem, &full, b);
    DeleteObject(b as _);
    crate::ui::style::rect(mem, full, panel, border, RADIUS);
    for (i, it) in items.iter().enumerate() {
        if it.separator {
            let pen = CreatePen(PS_SOLID, 1, border);
            let op = SelectObject(mem, pen as _);
            let y = (it.rect.top + it.rect.bottom) / 2;
            MoveToEx(mem, it.rect.left + 12, y, std::ptr::null_mut());
            LineTo(mem, it.rect.right - 12, y);
            SelectObject(mem, op);
            DeleteObject(pen as _);
            continue;
        }
        let mut state = 0u32;
        if it.disabled {
            state |= ODS_GRAYED | ODS_DISABLED;
        } else if hot == Some(i) {
            state |= ODS_SELECTED;
        }
        if it.checked {
            state |= ODS_CHECKED;
        }
        let d = DRAWITEMSTRUCT {
            CtlType: ODT_MENU,
            CtlID: 0,
            itemID: it.id,
            itemAction: ODA_DRAWENTIRE,
            itemState: state,
            hwndItem: std::ptr::null_mut(),
            hDC: mem,
            rcItem: it.rect,
            itemData: it.data,
        };
        SendMessageW(owner, WM_DRAWITEM, 0, &d as *const _ as LPARAM);
        // The owner's handler excludes a submenu item from the clip (for the system arrow);
        // start every item from a clean clip.
        SelectClipRgn(mem, std::ptr::null_mut());
    }
    BitBlt(dc, 0, 0, w, ht, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old);
    DeleteObject(bmp as _);
    DeleteDC(mem);
    EndPaint(h, &ps);
}

/// Same arguments and result as `TrackPopupMenu` with `TPM_RETURNCMD`: the chosen command id, or 0.
pub unsafe fn track(menu: HMENU, _flags: u32, x: i32, y: i32, owner: HWND) -> i32 {
    let mut items = collect(menu);
    if items.is_empty() {
        return 0;
    }
    let (w, h) = measure(owner, &mut items);
    let at = place(x, y, w, h, work_area(x, y));
    let first = items.iter().position(selectable);
    let mut root = create_level(owner, items, (w, h), at, true);
    root.hot = first;
    let root_hwnd = root.hwnd;
    ST.with(|s| *s.borrow_mut() = Some(State { owner, levels: vec![root], result: 0, done: false }));
    SetForegroundWindow(root_hwnd);
    SetFocus(root_hwnd);
    SetCapture(root_hwnd);
    let mut stash: Vec<MSG> = Vec::new();
    let mut quit: Option<i32> = None;
    loop {
        if with(|s| s.done).unwrap_or(true) {
            break;
        }
        let mut msg: MSG = std::mem::zeroed();
        let r = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
        if r <= 0 {
            quit = Some(msg.wParam as i32);
            break;
        }
        if msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN {
            key(msg.wParam as u32);
            continue;
        }
        if msg.hwnd.is_null() {
            // Thread messages (the app's hotkeys) are handled after the menu closes.
            stash.push(msg);
            continue;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let result = with(|s| s.result).unwrap_or(0);
    // Mark done first so closing the windows cannot end the menu a second way.
    ReleaseCapture();
    close_to(0);
    ST.with(|s| *s.borrow_mut() = None);
    for m in stash {
        PostThreadMessageW(windows_sys::Win32::System::Threading::GetCurrentThreadId(), m.message, m.wParam, m.lParam);
    }
    if let Some(code) = quit {
        PostQuitMessage(code);
    }
    result
}
