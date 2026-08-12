#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod storage;

use std::path::PathBuf;
use std::time::Duration;

use clipboard::{current_sequence_number, ClipEntry, ClipboardIO};
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW, GetWindowThreadProcessId,
    PostMessageW, SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE, GUITHREADINFO, WM_PASTE,
    WS_EX_NOACTIVATE,
};

const VK_CONTROL: u16 = 0x11;
const VK_V: u16 = 0x56;

fn send_ctrl_v() {
    let key = |vk: u16, scan: u16, up: bool| INPUT {
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
    };
    let inputs = [
        key(VK_CONTROL, 0x1D, false),
        key(VK_V, 0x2F, false),
        key(VK_V, 0x2F, true),
        key(VK_CONTROL, 0x1D, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn main() -> eframe::Result {
    let data_path = storage::default_path();
    let (entries, format_on) = storage::load(&data_path).unwrap_or((Vec::new(), true));

    let mut viewport = egui::ViewportBuilder::default()
        .with_always_on_top()
        .with_title("Smart Clipboard")
        .with_inner_size([380.0, 560.0])
        .with_min_inner_size([280.0, 200.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Smart Clipboard",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let mut app = SmartClipboardApp::new(data_path, entries, format_on, app_window_hwnd(cc));
            app.ensure_no_activate();
            Ok(Box::new(app))
        }),
    )
}

fn app_window_hwnd(cc: &eframe::CreationContext<'_>) -> HWND {
    match cc.window_handle() {
        Ok(handle) => match handle.as_raw() {
            RawWindowHandle::Win32(w) => HWND(w.hwnd.get() as *mut std::ffi::c_void),
            _ => HWND(std::ptr::null_mut()),
        },
        Err(_) => HWND(std::ptr::null_mut()),
    }
}

struct SmartClipboardApp {
    entries: Vec<ClipEntry>,
    format_on: bool,
    data_path: PathBuf,
    io: ClipboardIO,
    last_seq: u32,
    suppress_until_seq: u32,
    paste_target: Option<HWND>,
    app_hwnd: HWND,
    needs_save: bool,
}

impl SmartClipboardApp {
    fn new(
        data_path: PathBuf,
        entries: Vec<ClipEntry>,
        format_on: bool,
        app_hwnd: HWND,
    ) -> Self {
        Self {
            entries,
            format_on,
            data_path,
            io: ClipboardIO::new(),
            last_seq: 0,
            suppress_until_seq: 0,
            paste_target: unsafe { Some(GetForegroundWindow()) },
            app_hwnd,
            needs_save: false,
        }
    }

    fn ensure_no_activate(&self) {
        unsafe {
            if self.app_hwnd.0.is_null() {
                return;
            }
            let ex = GetWindowLongPtrW(self.app_hwnd, GWL_EXSTYLE);
            let no_act = WS_EX_NOACTIVATE.0 as isize;
            if ex & no_act == 0 {
                let _ = SetWindowLongPtrW(self.app_hwnd, GWL_EXSTYLE, ex | no_act);
            }
        }
    }

    fn poll_clipboard(&mut self) {
        let seq = current_sequence_number();
        if seq == self.last_seq || seq == self.suppress_until_seq {
            self.last_seq = seq;
            return;
        }
        self.last_seq = seq;
        if let Some(entry) = self.io.read() {
            if !entry.text.trim().is_empty() {
                self.add_entry(entry);
            }
        }
    }

    fn add_entry(&mut self, entry: ClipEntry) {
        let key = (entry.text.clone(), entry.html.clone(), entry.rtf.clone());
        if let Some(pos) = self.entries.iter().position(|e| {
            e.text == key.0 && e.html == key.1 && e.rtf == key.2
        }) {
            let mut existing = self.entries.remove(pos);
            existing.time = entry.time;
            self.entries.insert(0, existing);
        } else {
            self.entries.insert(0, entry);
        }
        self.needs_save = true;
    }

    fn do_paste(&mut self, idx: usize) {
        let Some(entry) = self.entries.get(idx).cloned() else {
            return;
        };
        self.io.write(&entry, self.format_on);
        self.suppress_until_seq = current_sequence_number();

        let Some(target) = self.paste_target else {
            return;
        };
        if target.0.is_null() {
            return;
        }

        std::thread::sleep(Duration::from_millis(60));

        unsafe {
            if GetForegroundWindow() == target {
                std::thread::sleep(Duration::from_millis(30));
                send_ctrl_v();
            } else {
                self.paste_focus_fallback(target);
            }
            let _ = SetForegroundWindow(target);
        }
    }

    fn paste_focus_fallback(&self, target: HWND) {
        unsafe {
            let our_tid = GetCurrentThreadId();
            let target_tid = GetWindowThreadProcessId(target, None);
            let attached = our_tid != target_tid
                && AttachThreadInput(our_tid, target_tid, true).as_bool();

            let mut ok = SetForegroundWindow(target);
            let deadline = std::time::Instant::now() + Duration::from_millis(1500);
            while GetForegroundWindow() != target && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
                if !ok.as_bool() {
                    ok = SetForegroundWindow(target);
                }
            }

            if GetForegroundWindow() == target {
                std::thread::sleep(Duration::from_millis(40));
                send_ctrl_v();
            } else {
                let mut info = GUITHREADINFO::default();
                info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
                let focus_hwnd = if GetGUIThreadInfo(target_tid, &mut info).is_ok() {
                    info.hwndFocus
                } else {
                    HWND(std::ptr::null_mut())
                };
                let dest = if focus_hwnd.0.is_null() { target } else { focus_hwnd };
                let _ = PostMessageW(dest, WM_PASTE, WPARAM(0), LPARAM(0));
            }

            if attached {
                let _ = AttachThreadInput(our_tid, target_tid, false);
            }
        }
    }

    fn persist(&self) {
        storage::save(&self.data_path, &self.entries, self.format_on);
    }
}

impl eframe::App for SmartClipboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));
        self.ensure_no_activate();
        self.poll_clipboard();

        let focused = ctx.input(|i| i.raw.focused);
        if !focused {
            self.paste_target = unsafe { Some(GetForegroundWindow()) };
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.strong("Smart Clipboard");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = ui
                        .button("Clear All")
                        .on_hover_text("Remove all saved clipboard entries");
                    if resp.clicked() {
                        self.entries.clear();
                        self.needs_save = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Format");
                let sw = ui.toggle_value(&mut self.format_on, "");
                let (color, status) = if self.format_on {
                    (egui::Color32::from_rgb(120, 210, 120), "ON")
                } else {
                    (ui.visuals().weak_text_color(), "OFF")
                };
                ui.colored_label(color, status);
                sw.on_hover_text(
                    "When ON, text is pasted with its formatting.\nWhen OFF, it is pasted as plain text.",
                );
            });
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.entries.is_empty() {
                        ui.add_space(24.0);
                        ui.centered_and_justified(|ui| {
                            ui.weak("Nothing copied yet.\nCopy any text anywhere and it will show up here.")
                        });
                        return;
                    }

                    let mut remove: Option<usize> = None;
                    let mut paste: Option<usize> = None;

                    for (i, entry) in self.entries.iter().enumerate() {
                        let preview = preview_text(&entry.text);
                        let frame = egui::Frame::group(ui.style());
                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                let avail = (ui.available_width() - 40.0).max(60.0);
                                let btn = egui::Button::new(
                                    egui::RichText::new(preview).color(ui.visuals().text_color()),
                                )
                                .wrap()
                                .min_size(egui::vec2(avail, 0.0));
                                let resp = ui.add(btn);
                                if resp.on_hover_text(
                                    "Click to paste into the last focused window",
                                ).clicked()
                                {
                                    paste = Some(i);
                                }
                                let x = ui
                                    .small_button("X")
                                    .on_hover_text("Remove this entry");
                                if x.clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }

                    if let Some(i) = remove {
                        self.entries.remove(i);
                        self.needs_save = true;
                    }
                    if let Some(i) = paste {
                        self.do_paste(i);
                    }
                });
        });

        if self.needs_save {
            self.persist();
            self.needs_save = false;
        }
    }
}

fn preview_text(text: &str) -> String {
    let normalized: String = text
        .chars()
        .map(|c| if c == '\r' { '\n' } else { c })
        .collect();
    let trimmed = normalized.trim();
    let mut out = String::new();
    for c in trimmed.chars().take(200) {
        out.push(c);
    }
    if trimmed.chars().count() > 200 {
        out.push_str("...");
    }
    out
}
