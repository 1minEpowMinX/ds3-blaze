//! Window icon taken from the icon resource embedded in the executable.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CXSMICON,
    SendMessageW, WM_SETICON,
};

/// Resource ordinal of the icon `build.rs` embeds: winresource gives the
/// application icon ordinal 1.
const ICON_ID: u16 = 1;

/// Sets the executable's icon as the small and the large icon of `window`.
///
/// Each is loaded at the size the system uses for the window's DPI, so the
/// frame of that size in the .ico is shown as drawn. Does nothing when the
/// window has no Win32 handle or the resource does not load.
pub fn apply(window: &impl HasWindowHandle) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as HWND;
    // SAFETY: `hwnd` is the live window eframe just created, and every call
    // below only reads it or the executable's own resources.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let dpi = GetDpiForWindow(hwnd);
        for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let size = GetSystemMetricsForDpi(metric, dpi);
            // The ordinal in place of a name pointer is MAKEINTRESOURCEW.
            let name = usize::from(ICON_ID) as *const u16;
            let icon = LoadImageW(module, name, IMAGE_ICON, size, size, LR_DEFAULTCOLOR);
            // The window keeps using the icon until the process exits, so it
            // is never destroyed.
            if !icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, kind as usize, icon as isize);
            }
        }
    }
}
