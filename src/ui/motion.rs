//! Short interaction transitions. No timer exists while a control is at rest.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Input::KeyboardAndMouse::IsWindowEnabled, WindowsAndMessaging::*},
};
pub const TIMER: usize = 0x5a43;
static REDUCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// User setting "Reduce motion": every transition snaps to its end state and no timer starts.
pub fn reduced() -> bool {
    REDUCE.load(std::sync::atomic::Ordering::Relaxed)
}
pub fn set_reduced(on: bool) {
    REDUCE.store(on, std::sync::atomic::Ordering::Relaxed);
}
const DURATION: u128 = 150;
const ACTIVE: [u16; 14] = [
    109, 111, 116, 105, 111, 110, 45, 97, 99, 116, 105, 118, 101, 0,
];
#[derive(Clone)]
struct Transition {
    from: [f32; 3],
    target: [f32; 3],
    since: Instant,
}
thread_local! {static STATES:RefCell<BTreeMap<usize,Transition>>=RefCell::new(BTreeMap::new());}
fn eased(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn value(s: &Transition) -> [f32; 3] {
    let t = eased(s.since.elapsed().as_millis() as f32 / DURATION as f32);
    std::array::from_fn(|i| s.from[i] + (s.target[i] - s.from[i]) * t)
}
pub unsafe fn allowed(h: HWND) -> bool {
    let mut enabled = 1i32;
    let read = SystemParametersInfoW(0x1042, 0, &mut enabled as *mut _ as _, 0);
    !reduced()
        && read != 0
        && enabled != 0
        && IsWindowEnabled(h) != 0
        && IsWindowVisible(h) != 0
        && IsIconic(GetAncestor(h, GA_ROOT)) == 0
}
/// Sliding selection pill for the Settings section buttons. Position is in button units
/// (0 = first section). It only moves for about 200 ms after a section change; nothing runs
/// while it is at rest, and with motion off it snaps.
pub const PILL_TIMER: usize = 0x5a45;
const PILL_MS: u128 = 200;
thread_local! {static PILL:RefCell<Option<(f32,usize,Instant)>>=const{RefCell::new(None)};}
/// Set the selected section. Returns true when a slide started (the caller then runs the timer).
pub fn pill_set(target: usize, animate: bool) -> bool {
    PILL.with(|p| {
        let mut p = p.borrow_mut();
        let from = match *p {
            Some((_, t, _)) if t == target => return false,
            Some((_, _, _)) if animate => pill_value(&p),
            _ => target as f32,
        };
        *p = Some((from, target, Instant::now()));
        animate && from != target as f32
    })
}
fn pill_value(p: &Option<(f32, usize, Instant)>) -> f32 {
    match p {
        Some((from, to, since)) => {
            let t = since.elapsed().as_millis() as f32 / PILL_MS as f32;
            from + (*to as f32 - from) * crate::ease::ease_out(t)
        }
        None => 0.,
    }
}
/// Current pill position and whether it is still moving.
pub fn pill() -> (f32, bool) {
    PILL.with(|p| {
        let p = p.borrow();
        let moving = matches!(*p, Some((_, _, since)) if since.elapsed().as_millis() < PILL_MS);
        (pill_value(&p), moving)
    })
}
pub unsafe fn sample(h: HWND, hover: bool, pressed: bool, on: bool) -> [f32; 3] {
    let target = [hover as u8 as f32, pressed as u8 as f32, on as u8 as f32];
    let enabled = allowed(h);
    let mut start = false;
    let out = STATES.with(|all| {
        let mut all = all.borrow_mut();
        let s = all.entry(h as usize).or_insert_with(|| Transition {
            from: target,
            target,
            since: Instant::now(),
        });
        if !enabled {
            *s = Transition {
                from: target,
                target,
                since: Instant::now(),
            };
            return target;
        }
        if s.target != target {
            let from = value(s);
            *s = Transition {
                from,
                target,
                since: Instant::now(),
            };
            start = true;
        }
        value(s)
    });
    if start {
        if SetTimer(h, TIMER, 16, None) != 0 {
            SetPropW(h, ACTIVE.as_ptr(), 1usize as _);
        } else {
            STATES.with(|all| {
                if let Some(s) = all.borrow_mut().get_mut(&(h as usize)) {
                    s.from = s.target;
                    s.since = Instant::now() - std::time::Duration::from_millis(200);
                }
            });
        }
    }
    if !enabled {
        stop(h);
    }
    out
}
unsafe fn stop(h: HWND) {
    KillTimer(h, TIMER);
    RemovePropW(h, ACTIVE.as_ptr());
}
pub unsafe fn tick(h: HWND) {
    let done = STATES.with(|all| {
        all.borrow()
            .get(&(h as usize))
            .map(|s| s.since.elapsed().as_millis() >= DURATION)
            .unwrap_or(true)
    });
    if done || !allowed(h) {
        stop(h);
    }
    if IsWindowVisible(h) != 0 {
        InvalidateRect(h, std::ptr::null(), 0);
    }
}
pub unsafe fn forget(h: HWND) {
    stop(h);
    STATES.with(|s| {
        s.borrow_mut().remove(&(h as usize));
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn easing_bounded_and_monotone() {
        let mut last = 0.;
        for i in 0..=100 {
            let n = eased(i as f32 / 100.);
            assert!((0.0..=1.0).contains(&n) && n >= last);
            last = n;
        }
        assert_eq!(eased(0.), 0.);
        assert_eq!(eased(1.), 1.);
    }
}

/// Settle visual-only transitions before a modal or blocking capture restart.
/// No data/state command is run here.
pub unsafe fn settle_thread() {
    let handles = STATES.with(|all| {
        let mut all = all.borrow_mut();
        all.iter_mut()
            .map(|(h, s)| {
                s.from = s.target;
                s.since = Instant::now() - std::time::Duration::from_millis(200);
                *h
            })
            .collect::<Vec<_>>()
    });
    for h in handles {
        stop(h as HWND);
        InvalidateRect(h as HWND, std::ptr::null(), 0);
    }
}
