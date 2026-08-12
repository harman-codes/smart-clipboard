mod clipboard {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/clipboard.rs"));
}

use clipboard::{ClipEntry, ClipboardIO};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

fn key(vk: u16, scan: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn main() {
    let io = ClipboardIO::new();
    io.write(
        &ClipEntry {
            text: "PASTE TEST CONTENT 12345".to_string(),
            html: None,
            rtf: None,
            time: 0,
        },
        true,
    );
    std::thread::sleep(std::time::Duration::from_millis(150));
    unsafe {
        let target = GetForegroundWindow();
        println!("target hwnd: {:?}", target.0);
        let ok = SetForegroundWindow(target);
        println!("SetForegroundWindow ok: {}", ok.as_bool());
        std::thread::sleep(std::time::Duration::from_millis(120));
        let inputs = [
            key(0x11, 0x1D, false),
            key(0x56, 0x2F, false),
            key(0x56, 0x2F, true),
            key(0x11, 0x1D, true),
        ];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        println!("SendInput ctrl+v sent: {}", sent);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}
