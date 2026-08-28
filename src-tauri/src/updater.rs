use crate::process_shutdown;
use std::time::Duration;
use tauri::AppHandle;
#[cfg(target_os = "windows")]
use tauri::Manager;
#[cfg(not(target_os = "windows"))]
use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

#[cfg(target_os = "windows")]
use std::sync::mpsc::{self, Sender};

const UPDATE_TIMEOUT: Duration = Duration::from_secs(15);
const INSTALL_STATUS_DELAY: Duration = Duration::from_millis(600);

#[cfg(target_os = "windows")]
enum ProgressCommand {
    Update {
        status: String,
        detail: String,
        percentage: Option<u8>,
    },
    Close,
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct ProgressWindow {
    sender: Sender<ProgressCommand>,
}

#[cfg(not(target_os = "windows"))]
#[derive(Clone)]
struct ProgressWindow(WebviewWindow);

pub fn check_on_start(app: AppHandle) {
    if cfg!(debug_assertions) && std::env::var("ANIWORLD_UPDATE_CHECK").as_deref() != Ok("1") {
        return;
    }

    tauri::async_runtime::spawn(async move {
        if let Err(error) = check_for_update(app).await {
            eprintln!("Update check failed: {error}");
        }
    });
}

async fn check_for_update(app: AppHandle) -> tauri_plugin_updater::Result<()> {
    let updater = app.updater_builder().timeout(UPDATE_TIMEOUT).build()?;
    let Some(update) = updater.check().await? else {
        return Ok(());
    };

    let version = update.version.clone();
    let accepted = app
        .dialog()
        .message(format!(
            "AniWorld Desktop {version} is available.\n\nDownload and install it now?"
        ))
        .title(format!("New Version {version}"))
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::YesNo)
        .blocking_show();

    if !accepted {
        return Ok(());
    }

    let _update_lock = match process_shutdown::acquire_update_lock() {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            app.dialog()
                .message("Another AniWorld Desktop window is already installing this update.")
                .title("Update Already Running")
                .kind(MessageDialogKind::Info)
                .show(|_| {});
            return Ok(());
        }
        Err(message) => {
            app.dialog()
                .message(format!("The update could not be started:\n\n{message}"))
                .title("Update Failed")
                .kind(MessageDialogKind::Error)
                .show(|_| {});
            return Err(std::io::Error::other(message).into());
        }
    };

    let progress_window = create_progress_window(&app, &version)
        .map_err(|message| tauri_plugin_updater::Error::from(std::io::Error::other(message)))?;
    let download_window = progress_window.clone();
    let verification_window = progress_window.clone();
    let mut downloaded = 0_u64;

    let bytes = match update
        .download(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                show_download_progress(&download_window, downloaded, content_length);
            },
            move || {
                set_progress_state(
                    &verification_window,
                    "Verifying update…",
                    "Checking the downloaded update signature",
                    None,
                );
            },
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(error) => {
            show_update_error(&app, &progress_window, &error);
            return Err(error);
        }
    };

    set_progress_state(
        &progress_window,
        "Preparing installation…",
        "Closing other AniWorld Desktop windows",
        Some(100),
    );

    let closed_instances =
        match tauri::async_runtime::spawn_blocking(process_shutdown::close_other_instances).await {
            Ok(Ok(count)) => count,
            Ok(Err(message)) => {
                let error = tauri_plugin_updater::Error::from(std::io::Error::other(message));
                show_update_error(&app, &progress_window, &error);
                return Err(error);
            }
            Err(error) => {
                let error = tauri_plugin_updater::Error::from(std::io::Error::other(format!(
                    "Could not close other AniWorld Desktop windows: {error}"
                )));
                show_update_error(&app, &progress_window, &error);
                return Err(error);
            }
        };

    let install_detail = match closed_instances {
        0 => "AniWorld Desktop will close and restart automatically".to_owned(),
        1 => "Closed 1 other window. AniWorld Desktop will restart automatically".to_owned(),
        count => {
            format!("Closed {count} other windows. AniWorld Desktop will restart automatically")
        }
    };
    set_progress_state(
        &progress_window,
        "Installing update…",
        &install_detail,
        Some(100),
    );
    let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(INSTALL_STATUS_DELAY)).await;

    if let Err(error) = update.install(bytes) {
        show_update_error(&app, &progress_window, &error);
        return Err(error);
    }

    #[cfg(not(target_os = "windows"))]
    app.restart();

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn create_progress_window(app: &AppHandle, version: &str) -> Result<ProgressWindow, String> {
    let version = serde_json::json!(version);
    WebviewWindowBuilder::new(
        app,
        "updater-progress",
        WebviewUrl::App("update.html".into()),
    )
    .title("Updating AniWorld Desktop")
    .inner_size(480.0, 270.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .center()
    .initialization_script(format!(
        r#"
        window.__ANIWORLD_UPDATE_VERSION__ = {version};
        window.__ANIWORLD_UPDATER_STATE__ = {{
          status: "Preparing update…",
          detail: "The app will restart automatically",
          percentage: null
        }};
        window.setUpdaterState = (state) => {{
          window.__ANIWORLD_UPDATER_STATE__ = state;
          window.dispatchEvent(new CustomEvent("aniworld-updater-state", {{ detail: state }}));
        }};
        "#
    ))
    .build()
    .map(ProgressWindow)
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn create_progress_window(app: &AppHandle, version: &str) -> Result<ProgressWindow, String> {
    let (sender, receiver) = mpsc::channel();
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let version = version.to_owned();
    let parent_window = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|handle| handle.0 as isize);

    std::thread::Builder::new()
        .name("aniworld-native-updater".to_owned())
        .spawn(move || native_progress_dialog(version, parent_window, receiver, ready_sender))
        .map_err(|error| format!("Could not start the update progress dialog: {error}"))?;

    ready_receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|error| format!("The update progress dialog did not start: {error}"))??;

    Ok(ProgressWindow { sender })
}

#[cfg(target_os = "windows")]
fn native_progress_dialog(
    version: String,
    parent_window: Option<isize>,
    receiver: mpsc::Receiver<ProgressCommand>,
    ready_sender: mpsc::SyncSender<Result<(), String>>,
) {
    use windows::core::{IUnknown, HSTRING};
    use windows::Win32::{
        Foundation::HWND,
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{
            CLSID_ProgressDialog, IProgressDialog, PROGDLG_MODAL, PROGDLG_NOCANCEL,
            PROGDLG_NOMINIMIZE, PROGDLG_NOTIME,
        },
    };

    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if let Err(error) = initialized.ok() {
        let _ = ready_sender.send(Err(format!(
            "Could not initialize the native update dialog: {error}"
        )));
        return;
    }

    let dialog: IProgressDialog = match unsafe {
        CoCreateInstance(
            &CLSID_ProgressDialog,
            None::<&IUnknown>,
            CLSCTX_INPROC_SERVER,
        )
    } {
        Ok(dialog) => dialog,
        Err(error) => {
            let _ = ready_sender.send(Err(format!(
                "Could not create the native update dialog: {error}"
            )));
            unsafe { CoUninitialize() };
            return;
        }
    };

    let started = unsafe {
        (|| -> windows::core::Result<()> {
            dialog.SetTitle(&HSTRING::from("AniWorld Desktop Update"))?;
            dialog.StartProgressDialog(
                parent_window.map(|handle| HWND(handle as *mut _)),
                None::<&IUnknown>,
                PROGDLG_MODAL | PROGDLG_NOCANCEL | PROGDLG_NOMINIMIZE | PROGDLG_NOTIME,
                None,
            )?;
            dialog.SetLine(1, &HSTRING::from("Preparing update…"), false, None)?;
            dialog.SetLine(
                2,
                &HSTRING::from("The app will restart automatically"),
                false,
                None,
            )?;
            dialog.SetLine(3, &HSTRING::from(format!("Version {version}")), false, None)?;
            dialog.SetProgress(0, 100)
        })()
    };

    if let Err(error) = started {
        let _ = ready_sender.send(Err(format!(
            "Could not show the native update dialog: {error}"
        )));
        let _ = unsafe { dialog.StopProgressDialog() };
        drop(dialog);
        unsafe { CoUninitialize() };
        return;
    }

    let _ = ready_sender.send(Ok(()));
    while let Ok(command) = receiver.recv() {
        match command {
            ProgressCommand::Update {
                status,
                detail,
                percentage,
            } => unsafe {
                let _ = dialog.SetLine(1, &HSTRING::from(status), false, None);
                let _ = dialog.SetLine(2, &HSTRING::from(detail), false, None);
                let _ = dialog.SetProgress(percentage.unwrap_or(0) as u32, 100);
            },
            ProgressCommand::Close => break,
        }
    }

    let _ = unsafe { dialog.StopProgressDialog() };
    drop(dialog);
    unsafe { CoUninitialize() };
}

fn show_download_progress(window: &ProgressWindow, downloaded: u64, total: Option<u64>) {
    let downloaded_mb = downloaded as f64 / 1_048_576.0;
    let (detail, percentage) = match total.filter(|total| *total > 0) {
        Some(total) => {
            let total_mb = total as f64 / 1_048_576.0;
            let percentage = downloaded
                .saturating_mul(100)
                .saturating_div(total)
                .min(100) as u8;
            (
                format!("{downloaded_mb:.1} MB of {total_mb:.1} MB"),
                Some(percentage),
            )
        }
        None => (format!("{downloaded_mb:.1} MB downloaded"), None),
    };

    set_progress_state(window, "Downloading update…", &detail, percentage);
}

#[cfg(target_os = "windows")]
fn set_progress_state(window: &ProgressWindow, status: &str, detail: &str, percentage: Option<u8>) {
    let _ = window.sender.send(ProgressCommand::Update {
        status: status.to_owned(),
        detail: detail.to_owned(),
        percentage,
    });
}

#[cfg(not(target_os = "windows"))]
fn set_progress_state(window: &ProgressWindow, status: &str, detail: &str, percentage: Option<u8>) {
    let state = serde_json::json!({
        "status": status,
        "detail": detail,
        "percentage": percentage,
    });
    let _ = window.0.eval(format!("window.setUpdaterState?.({state});"));
}

#[cfg(target_os = "windows")]
fn close_progress_window(window: &ProgressWindow) {
    let _ = window.sender.send(ProgressCommand::Close);
}

#[cfg(not(target_os = "windows"))]
fn close_progress_window(window: &ProgressWindow) {
    let _ = window.0.close();
}

fn show_update_error(
    app: &AppHandle,
    progress_window: &ProgressWindow,
    error: &tauri_plugin_updater::Error,
) {
    close_progress_window(progress_window);
    app.dialog()
        .message(format!("The update could not be installed:\n\n{error}"))
        .title("Update Failed")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}
