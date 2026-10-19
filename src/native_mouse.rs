use crate::flyout::MousePresses;
use std::cell::Cell;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HHOOK, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN,
    WM_LBUTTONUP,
};

thread_local! {
    static PRESSES: Cell<MousePresses> = Cell::new(MousePresses::default());
}

/// Counts left-button gestures on the UI thread; it stores no pointer coordinates.
pub struct MouseTracker(HHOOK);

impl MouseTracker {
    pub fn new() -> windows::core::Result<Self> {
        // SAFETY: The callback lives in this executable and runs on this thread's message loop.
        let module = unsafe { GetModuleHandleW(None) }?;
        let hook =
            unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(track_mouse), Some(module.into()), 0) }?;
        PRESSES.with(|state| state.set(MousePresses::default()));
        Ok(Self(hook))
    }

    pub fn pressed_id(&self) -> Option<u64> {
        PRESSES.with(|state| state.get().pressed_id())
    }
    pub fn last_id(&self) -> u64 {
        PRESSES.with(|state| state.get().last_id())
    }
}

impl Drop for MouseTracker {
    fn drop(&mut self) {
        // SAFETY: This object owns the hook and releases it on the UI thread.
        let _ = unsafe { UnhookWindowsHookEx(self.0) };
    }
}

unsafe extern "system" fn track_mouse(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code >= 0 && matches!(message.0 as u32, WM_LBUTTONDOWN | WM_LBUTTONUP) {
        PRESSES.with(|state| {
            let mut presses = state.get();
            if message.0 as u32 == WM_LBUTTONDOWN {
                presses.button_down();
            } else {
                presses.button_up();
            }
            state.set(presses);
        });
    }
    // SAFETY: Always forward the original event without suppressing or changing it.
    unsafe { CallNextHookEx(None, code, message, data) }
}
