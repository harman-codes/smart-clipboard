use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ClipEntry {
    pub text: String,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub time: i64,
}

pub struct ClipboardIO {
    html_format: u32,
    rtf_format: u32,
}

impl ClipboardIO {
    pub fn new() -> Self {
        let html_format = Self::register("HTML Format");
        let rtf_format = Self::register("Rich Text Format");
        Self {
            html_format,
            rtf_format,
        }
    }

    fn register(name: &str) -> u32 {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { RegisterClipboardFormatW(windows::core::PCWSTR(wide.as_ptr())) }
    }

    pub fn read(&self) -> Option<ClipEntry> {
        if unsafe { OpenClipboard(None) }.is_err() {
            return None;
        }
        let result = self.read_locked();
        let _ = unsafe { CloseClipboard() };
        result
    }

    fn read_locked(&self) -> Option<ClipEntry> {
        let text = unsafe {
            if !IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() {
                return None;
            }
            let h = GetClipboardData(CF_UNICODETEXT).ok()?;
            if h.0.is_null() {
                return None;
            }
            let hg = HGLOBAL(h.0);
            let ptr = GlobalLock(hg) as *const u16;
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(hg);
            text
        };

        if text.trim().is_empty() {
            return None;
        }

        let html = self
            .read_bytes(self.html_format)
            .map(|b| String::from_utf8_lossy(&b).split('\0').next().unwrap_or("").to_string());
        let rtf = self.read_bytes(self.rtf_format).map(|b| STANDARD.encode(b));

        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        Some(ClipEntry { text, html, rtf, time })
    }

    fn read_bytes(&self, format: u32) -> Option<Vec<u8>> {
        unsafe {
            if format == 0 || !IsClipboardFormatAvailable(format).is_ok() {
                return None;
            }
            let h = GetClipboardData(format).ok()?;
            if h.0.is_null() {
                return None;
            }
            let hg = HGLOBAL(h.0);
            let size = GlobalSize(hg);
            if size == 0 {
                return None;
            }
            let ptr = GlobalLock(hg) as *const u8;
            let data = std::slice::from_raw_parts(ptr, size);
            let nul = data.iter().position(|&b| b == 0).unwrap_or(size);
            let bytes = data[..nul].to_vec();
            let _ = GlobalUnlock(hg);
            if bytes.is_empty() { None } else { Some(bytes) }
        }
    }

    pub fn write(&self, entry: &ClipEntry, format_on: bool) {
        let mut pending: Vec<HGLOBAL> = Vec::new();
        unsafe {
            if OpenClipboard(None).is_ok() {
                if EmptyClipboard().is_ok() {
                    let utf16: Vec<u16> = entry
                        .text
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    let bytes =
                        std::slice::from_raw_parts(utf16.as_ptr() as *const u8, utf16.len() * 2);
                    if let Ok(h) = make_hglobal_bytes(bytes) {
                        set_data(CF_UNICODETEXT, h, &mut pending);
                    }

                    if format_on {
                        if let Some(html) = &entry.html {
                            let mut bytes = html.as_bytes().to_vec();
                            bytes.push(0);
                            if let Ok(h) = make_hglobal_bytes(&bytes) {
                                set_data(self.html_format, h, &mut pending);
                            }
                        }
                        if let Some(rtf_b64) = &entry.rtf {
                            if let Ok(decoded) = STANDARD.decode(rtf_b64) {
                                let mut bytes = decoded;
                                bytes.push(0);
                                if let Ok(h) = make_hglobal_bytes(&bytes) {
                                    set_data(self.rtf_format, h, &mut pending);
                                }
                            }
                        }
                    }
                }
                let _ = CloseClipboard();
            }
        }
        for h in pending {
            let _ = unsafe { GlobalFree(h) };
        }
    }
}

fn set_data(format: u32, h: HGLOBAL, pending: &mut Vec<HGLOBAL>) {
    pending.push(h);
    if unsafe { SetClipboardData(format, HANDLE(h.0)) }.is_ok() {
        pending.pop();
    }
}

fn make_hglobal_bytes(bytes: &[u8]) -> windows::core::Result<HGLOBAL> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let ptr = GlobalLock(h) as *mut u8;
        if !bytes.is_empty() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        }
        let _ = GlobalUnlock(h);
        Ok(h)
    }
}

pub fn current_sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}
