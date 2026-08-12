use std::path::Path;

use crate::clipboard::ClipEntry;

fn default_true() -> bool {
    true
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Store {
    #[serde(default)]
    format_on: bool,
    #[serde(default = "default_true")]
    dark_mode: bool,
    #[serde(default)]
    trim_on: bool,
    entries: Vec<ClipEntry>,
}

pub fn default_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("smart_clipboard_data.json")
}

pub fn load(path: &Path) -> Option<(Vec<ClipEntry>, bool, bool, bool)> {
    let text = std::fs::read_to_string(path).ok()?;
    let store: Store = serde_json::from_str(&text).ok()?;
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::with_capacity(store.entries.len());
    for e in store.entries {
        let key = (e.text.clone(), e.html.clone(), e.rtf.clone());
        if seen.insert(key) {
            entries.push(e);
        }
    }
    Some((entries, store.format_on, store.dark_mode, store.trim_on))
}

pub fn save(path: &Path, entries: &[ClipEntry], format_on: bool, dark_mode: bool, trim_on: bool) {
    let store = Store {
        format_on,
        dark_mode,
        trim_on,
        entries: entries.to_vec(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&store) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, json);
    }
}
