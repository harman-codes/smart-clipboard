mod clipboard {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/clipboard.rs"));
}

use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOVSCROLL, ES_MULTILINE,
    EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, SW_SHOW,
    SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetCursorPos, SetForegroundWindow, SetWindowPos,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_GETTEXT,
    WNDCLASS_STYLES, WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WindowFromPoint,
};
use windows::core::{PCSTR, PCWSTR};

const TARGET_TITLE: &str = "PROBE_TARGET";
const TARGET_CLASS: &str = "FocusProbeWnd";

static EDIT_HWND: AtomicIsize = AtomicIsize::new(0);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn window_title(hwnd: HWND) -> String {
    if hwnd.0.is_null() {
        return String::new();
    }
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    unsafe {
        GetWindowTextW(hwnd, &mut buf);
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

fn real_set_focus(hwnd: HWND) {
    unsafe {
        type F = unsafe extern "system" fn(HWND) -> HWND;
        let user32 = LoadLibraryW(PCWSTR(wide("user32.dll").as_ptr())).unwrap_or_default();
        if let Some(addr) = GetProcAddress(user32, PCSTR(b"SetFocus\0".as_ptr())) {
            let f: F = std::mem::transmute(addr);
            f(hwnd);
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = DefWindowProcW(hwnd, msg, wparam, lparam);
    if msg == WM_ACTIVATE && wparam.0 != 0 {
        let edit = EDIT_HWND.load(Ordering::Relaxed);
        if edit != 0 {
            real_set_focus(HWND(edit as *mut std::ffi::c_void));
        }
    }
    result
}

fn mouse_down(x: i32, y: i32) {
    unsafe {
        let _ = SetCursorPos(x, y);
        let down = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_LEFTDOWN,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
    }
}

fn mouse_up() {
    unsafe {
        let up = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_LEFTUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
    }
}

fn mouse_click(x: i32, y: i32) {
    mouse_down(x, y);
    std::thread::sleep(Duration::from_millis(40));
    mouse_up();
}

fn pump_msgs(ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(ms);
    let mut msg = MSG::default();
    unsafe {
        while Instant::now() < deadline {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn force_foreground(hwnd: HWND) {
    unsafe {
        let fg = GetForegroundWindow();
        let fg_tid = GetWindowThreadProcessId(fg, None);
        let my_tid = GetCurrentThreadId();
        let attached = my_tid != fg_tid && AttachThreadInput(my_tid, fg_tid, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        std::thread::sleep(Duration::from_millis(250));
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(my_tid, fg_tid, false);
        }
    }
}

fn edit_text(edit: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(edit);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let _ = SendMessageW(
            edit,
            WM_GETTEXT,
            WPARAM(buf.len() as usize),
            LPARAM(buf.as_mut_ptr() as isize),
        );
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

fn find_app_window() -> HWND {
    use windows::Win32::Foundation::BOOL;
    static FOUND: AtomicIsize = AtomicIsize::new(0);
    unsafe extern "system" fn cb(hwnd: HWND, _l: LPARAM) -> BOOL {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return BOOL(1);
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        if title == "Smart Clipboard" {
            FOUND.store(hwnd.0 as isize, Ordering::Relaxed);
            return BOOL(0);
        }
        BOOL(1)
    }
    unsafe {
        EnumWindows(Some(cb), LPARAM(0));
    }
    let raw = FOUND.load(Ordering::Relaxed) as isize;
    HWND(raw as *mut std::ffi::c_void)
}

fn main() {
    let io = clipboard::ClipboardIO::new();
    io.write(
        &clipboard::ClipEntry {
            text: "FOCUS_PROBE_CONTENT".to_string(),
            html: None,
            rtf: None,
            time: 0,
        },
        true,
    );

    let exe = concat!(env!("CARGO_MANIFEST_DIR"), "/Smart Clipboard.exe");
    let mut app = std::process::Command::new(exe).spawn().expect("launch app");
    std::thread::sleep(Duration::from_millis(2500));

    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let class_name = wide(TARGET_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: WNDCLASS_STYLES(0),
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: Default::default(),
            hCursor: Default::default(),
            hbrBackground: Default::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hIconSm: Default::default(),
        };
        let _ = RegisterClassExW(&wc);

        let title = wide(TARGET_TITLE);
        let target = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0),
            100,
            100,
            420,
            300,
            HWND(std::ptr::null_mut()),
            None,
            hinst,
            None,
        )
        .expect("create target window");

        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(wide("Edit").as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32),
            10,
            10,
            380,
            240,
            target,
            None,
            hinst,
            None,
        )
        .expect("create edit control");

        EDIT_HWND.store(edit.0 as isize, Ordering::Relaxed);
        ShowWindow(target, SW_SHOW);
        force_foreground(target);
        real_set_focus(edit);
        std::thread::sleep(Duration::from_millis(1200));

        let app_hwnd = find_app_window();
        if app_hwnd.0.is_null() {
            println!("FAIL: app window not found");
            let _ = DestroyWindow(target);
            let _ = app.kill();
            return;
        }

        let _ = SetWindowPos(
            app_hwnd,
            None,
            700,
            150,
            380,
            560,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        std::thread::sleep(Duration::from_millis(500));

        let mut rc = RECT::default();
        let _ = GetWindowRect(app_hwnd, &mut rc);
        let cx = (rc.left + rc.right) / 2;
        let click_y = rc.top + 118;

        mouse_click(cx, click_y);
        pump_msgs(2500);

        let fg = GetForegroundWindow();
        let text = edit_text(edit);
        let focused_ok = fg.0 == target.0;
        let pasted = text.contains("FOCUS_PROBE_CONTENT");
        println!(
            "after 1st click -> fg_is_target={} fg_is_app={} pasted={} text='{}'",
            focused_ok,
            fg.0 == app_hwnd.0,
            pasted,
            text
        );
        let ok1 = focused_ok && pasted;

        mouse_click(cx, click_y);
        pump_msgs(2500);
        let fg2 = GetForegroundWindow();
        let text2 = edit_text(edit);
        let pasted2 = text2.matches("FOCUS_PROBE_CONTENT").count();
        let focused_ok2 = fg2.0 == target.0;
        println!(
            "after 2nd click -> fg_is_target={} fg_is_app={} paste_count={} text='{}'",
            focused_ok2,
            fg2.0 == app_hwnd.0,
            pasted2,
            text2
        );
        let ok2 = focused_ok2 && pasted2 >= 1;

        println!("RESULT 1: {}", if ok1 { "PASS" } else { "FAIL" });
        println!("RESULT 2: {}", if ok2 { "PASS" } else { "FAIL" });

        let before = {
            let mut r = RECT::default();
            let _ = GetWindowRect(app_hwnd, &mut r);
            r
        };
        mouse_down((before.left + before.right) / 2, before.top + 15);
        std::thread::sleep(Duration::from_millis(100));
        SetCursorPos(before.left + 60, before.top + 55);
        std::thread::sleep(Duration::from_millis(200));
        mouse_up();
        pump_msgs(800);
        let after = {
            let mut r = RECT::default();
            let _ = GetWindowRect(app_hwnd, &mut r);
            r
        };
        let moved = after.left != before.left || after.top != before.top;
        println!(
            "drag test: before=({},{}) after=({},{}) -> {}",
            before.left,
            before.top,
            after.left,
            after.top,
            if moved { "MOVED" } else { "NOT MOVED" }
        );

        let _ = DestroyWindow(target);
    }

    let _ = std::process::Command::new("taskkill")
        .args(["/IM", "Smart Clipboard.exe", "/F"])
        .output();
    let _ = app.kill();
}
