use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};
use tauri::WebviewWindow;

const WEBVIEW_DIRECTORY: &str = "EBWebView";
const RESET_MARKER_NAME: &str = "AniWorldDesktop.reset";
const RESET_MARKER_CONTENTS: &str = "AniWorldDesktop full data reset\n";
const CACHE_DIRECTORY_NAMES: &[&str] = &[
    "Cache",
    "Code Cache",
    "DawnCache",
    "GPUCache",
    "GPUPersistentCache",
    "GrShaderCache",
    "ShaderCache",
    "component_crx_cache",
    "extensions_crx_cache",
];

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserDataKind {
    Cache,
    SiteData,
    All,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub cache_bytes: u64,
    pub browser_data_bytes: u64,
    pub app_data_bytes: u64,
}

pub fn storage_info(data_directory: &Path) -> Result<StorageInfo, String> {
    let browser_directory = data_directory.join(WEBVIEW_DIRECTORY);
    Ok(StorageInfo {
        cache_bytes: cache_size(&browser_directory)?,
        browser_data_bytes: directory_size(&browser_directory)?,
        app_data_bytes: directory_size(data_directory)?,
    })
}

pub fn schedule_full_reset(data_directory: &Path) -> Result<(), String> {
    validate_data_directory(data_directory)?;
    let marker = reset_marker(data_directory)?;
    fs::write(&marker, RESET_MARKER_CONTENTS)
        .map_err(|error| format!("Could not schedule the app data reset: {error}"))
}

pub fn perform_pending_full_reset(data_directory: &Path) -> Result<(), String> {
    let marker = reset_marker(data_directory)?;
    let Ok(contents) = fs::read_to_string(&marker) else {
        return Ok(());
    };
    if contents != RESET_MARKER_CONTENTS {
        return Err(format!(
            "Refusing an invalid reset marker at {}",
            marker.display()
        ));
    }

    validate_data_directory(data_directory)?;
    if data_directory.exists() {
        let mut last_error = None;
        for attempt in 0..10 {
            match fs::remove_dir_all(data_directory) {
                Ok(()) => {
                    last_error = None;
                    break;
                }
                Err(error) if attempt < 9 => {
                    last_error = Some(error);
                    std::thread::sleep(Duration::from_millis(150));
                }
                Err(error) => last_error = Some(error),
            }
        }
        if let Some(error) = last_error {
            return Err(format!("Could not reset app data: {error}"));
        }
    }
    fs::remove_file(&marker)
        .map_err(|error| format!("Could not remove the app data reset marker: {error}"))?;
    Ok(())
}

pub fn clear(window: &WebviewWindow, kind: BrowserDataKind) -> Result<(), String> {
    #[cfg(windows)]
    {
        clear_windows(window, kind)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, kind);
        Err("Browser data management is currently available on Windows only".to_owned())
    }
}

#[cfg(windows)]
fn clear_windows(window: &WebviewWindow, kind: BrowserDataKind) -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .with_webview(move |platform_webview| {
            let error_sender = sender.clone();
            let result = unsafe { start_windows_clear(platform_webview, kind, sender) };
            if let Err(error) = result {
                let _ = error_sender.send(Err(format!("Could not clear browser data: {error}")));
            }
        })
        .map_err(|error| format!("Could not access the browser profile: {error}"))?;

    receiver
        .recv_timeout(Duration::from_secs(30))
        .map_err(|_| "Timed out while clearing browser data".to_owned())?
}

#[cfg(windows)]
unsafe fn start_windows_clear(
    platform_webview: tauri::webview::PlatformWebview,
    kind: BrowserDataKind,
    sender: mpsc::SyncSender<Result<(), String>>,
) -> windows::core::Result<()> {
    use webview2_com::{ClearBrowsingDataCompletedHandler, Microsoft::Web::WebView2::Win32::*};
    use windows::core::Interface;

    let kinds = match kind {
        BrowserDataKind::Cache => COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE,
        BrowserDataKind::SiteData => {
            COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_SITE
                | COREWEBVIEW2_BROWSING_DATA_KINDS_PASSWORD_AUTOSAVE
                | COREWEBVIEW2_BROWSING_DATA_KINDS_GENERAL_AUTOFILL
        }
        BrowserDataKind::All => COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_PROFILE,
    };
    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |status| {
        let result = status.map_err(|error| format!("Browser data could not be cleared: {error}"));
        let _ = sender.send(result);
        Ok(())
    }));
    platform_webview
        .controller()
        .CoreWebView2()?
        .cast::<ICoreWebView2_13>()?
        .Profile()?
        .cast::<ICoreWebView2Profile2>()?
        .ClearBrowsingData(kinds, &handler)
}

fn reset_marker(data_directory: &Path) -> Result<PathBuf, String> {
    validate_data_directory(data_directory)?;
    let parent = data_directory
        .parent()
        .ok_or_else(|| "The app data directory has no parent".to_owned())?;
    Ok(parent.join(RESET_MARKER_NAME))
}

fn validate_data_directory(data_directory: &Path) -> Result<(), String> {
    if data_directory.file_name().and_then(|name| name.to_str()) != Some("AniWorldDesktop") {
        return Err(format!(
            "Refusing to modify an unexpected app data directory: {}",
            data_directory.display()
        ));
    }
    if data_directory.parent().is_none() {
        return Err("Refusing to modify a root directory".to_owned());
    }
    Ok(())
}

fn directory_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }

    let mut total = 0_u64;
    for entry in
        fs::read_dir(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| format!("Could not read a directory entry: {error}"))?;
        total = total.saturating_add(directory_size(&entry.path())?);
    }
    Ok(total)
}

fn cache_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        return Ok(0);
    }

    let mut total = 0_u64;
    for entry in
        fs::read_dir(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| format!("Could not read a directory entry: {error}"))?;
        let entry_path = entry.path();
        let file_name = entry.file_name();
        let is_cache_directory = CACHE_DIRECTORY_NAMES
            .iter()
            .any(|candidate| file_name == *candidate);
        if is_cache_directory {
            total = total.saturating_add(directory_size(&entry_path)?);
        } else if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            total = total.saturating_add(cache_size(&entry_path)?);
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "aniworld-browser-data-{}-{unique}",
                std::process::id()
            ))
            .join("AniWorldDesktop")
    }

    #[test]
    fn storage_info_separates_browser_cache() {
        let directory = test_directory();
        fs::create_dir_all(directory.join("EBWebView/Default/Cache")).unwrap();
        fs::write(
            directory.join("EBWebView/Default/Cache/cache.bin"),
            [0_u8; 8],
        )
        .unwrap();
        fs::write(directory.join("EBWebView/Cookies"), [0_u8; 5]).unwrap();
        fs::write(directory.join("settings.json"), [0_u8; 3]).unwrap();

        let info = storage_info(&directory).unwrap();
        assert_eq!(info.cache_bytes, 8);
        assert_eq!(info.browser_data_bytes, 13);
        assert_eq!(info.app_data_bytes, 16);

        fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    }

    #[test]
    fn pending_reset_requires_the_expected_marker_and_target() {
        let directory = test_directory();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("settings.json"), b"{}").unwrap();

        schedule_full_reset(&directory).unwrap();
        perform_pending_full_reset(&directory).unwrap();

        assert!(!directory.exists());
        assert!(!directory.parent().unwrap().join(RESET_MARKER_NAME).exists());
        fs::remove_dir(directory.parent().unwrap()).unwrap();
    }
}
