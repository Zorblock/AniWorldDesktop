use crate::process_shutdown;
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_updater::UpdaterExt;

const UPDATE_TIMEOUT: Duration = Duration::from_secs(15);
const INSTALL_STATUS_DELAY: Duration = Duration::from_millis(1_200);
const STARTUP_CHECK_ATTEMPTS: usize = 4;
const STARTUP_RETRY_DELAYS: [Duration; STARTUP_CHECK_ATTEMPTS - 1] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUiState {
    phase: UpdatePhase,
    version: Option<String>,
    percentage: Option<u8>,
    message: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum UpdatePhase {
    Hidden,
    Available,
    Downloading,
    Verifying,
    Preparing,
    Installing,
    Error,
}

impl Default for UpdateUiState {
    fn default() -> Self {
        Self {
            phase: UpdatePhase::Hidden,
            version: None,
            percentage: None,
            message: None,
        }
    }
}

pub struct UpdateController {
    state: RwLock<UpdateUiState>,
    operation_running: AtomicBool,
    check_completed: AtomicBool,
}

impl Default for UpdateController {
    fn default() -> Self {
        Self {
            state: RwLock::new(UpdateUiState::default()),
            operation_running: AtomicBool::new(false),
            check_completed: AtomicBool::new(false),
        }
    }
}

impl UpdateController {
    fn acquire_operation(&self) -> Result<UpdateOperationGuard<'_>, String> {
        self.operation_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| UpdateOperationGuard(&self.operation_running))
            .map_err(|_| "An update operation is already running".to_owned())
    }

    fn set_state(&self, app: &AppHandle, state: UpdateUiState) {
        if let Ok(mut current) = self.state.write() {
            *current = state;
        }
        if let Some(window) = app.get_webview_window("main") {
            self.publish_to_window(&window);
        }
    }

    pub fn publish_to_window(&self, window: &WebviewWindow) {
        let state = self
            .state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default();
        if let Ok(state) = serde_json::to_string(&state) {
            let _ = window.eval(format!("window.__aniworldSetUpdateState?.({state});"));
        }
    }

    fn available_version(&self) -> Option<String> {
        self.state
            .read()
            .ok()
            .and_then(|state| state.version.clone())
    }

    fn has_completed_check(&self) -> bool {
        self.check_completed.load(Ordering::Acquire)
    }

    fn publish_error(&self, app: &AppHandle, version: Option<String>, error: &str) {
        self.set_state(
            app,
            UpdateUiState {
                phase: UpdatePhase::Error,
                version,
                percentage: None,
                message: Some(short_error(error)),
            },
        );
    }
}

struct UpdateOperationGuard<'a>(&'a AtomicBool);

impl Drop for UpdateOperationGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub available_version: Option<String>,
}

#[derive(Debug)]
struct UpdateCheckError {
    message: String,
    retryable: bool,
}

impl UpdateCheckError {
    fn operation(message: String) -> Self {
        Self {
            message,
            retryable: false,
        }
    }

    fn updater(error: tauri_plugin_updater::Error) -> Self {
        let retryable = matches!(
            error,
            tauri_plugin_updater::Error::Io(_)
                | tauri_plugin_updater::Error::Reqwest(_)
                | tauri_plugin_updater::Error::Network(_)
                | tauri_plugin_updater::Error::ReleaseNotFound
        );
        Self {
            message: error.to_string(),
            retryable,
        }
    }
}

pub fn check_on_start(app: AppHandle, controller: Arc<UpdateController>) {
    if cfg!(debug_assertions) && std::env::var("ANIWORLD_UPDATE_CHECK").as_deref() != Ok("1") {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let attempts = STARTUP_RETRY_DELAYS
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::once(None));
        for (attempt, retry_delay) in attempts.enumerate() {
            if attempt > 0 && controller.has_completed_check() {
                return;
            }

            match check_for_update(&app, &controller).await {
                Ok(_) => return,
                Err(error) if error.retryable && retry_delay.is_some() => {
                    let delay = retry_delay.expect("retry attempts always have a delay");
                    eprintln!(
                        "Startup update check attempt {} failed: {}. Retrying in {} seconds.",
                        attempt + 1,
                        error.message,
                        delay.as_secs()
                    );
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        std::thread::sleep(delay);
                    })
                    .await;
                }
                Err(error) => {
                    eprintln!(
                        "Startup update check failed after {} attempt{}: {}",
                        attempt + 1,
                        if attempt == 0 { "" } else { "s" },
                        error.message
                    );
                    return;
                }
            }
        }
    });
}

pub async fn check_manually(
    app: AppHandle,
    controller: Arc<UpdateController>,
) -> Result<UpdateCheckResult, String> {
    check_for_update(&app, &controller)
        .await
        .map_err(|error| error.message)
}

pub fn install_requested(app: AppHandle, controller: Arc<UpdateController>) {
    tauri::async_runtime::spawn(async move {
        let version = controller.available_version();
        if let Err(error) = install_available_update(&app, &controller).await {
            eprintln!("Update installation failed: {error}");
            controller.publish_error(&app, version, &error);
        }
    });
}

async fn check_for_update(
    app: &AppHandle,
    controller: &Arc<UpdateController>,
) -> Result<UpdateCheckResult, UpdateCheckError> {
    let _operation = controller
        .acquire_operation()
        .map_err(UpdateCheckError::operation)?;
    let current_version = env!("CARGO_PKG_VERSION").to_owned();
    let updater = app
        .updater_builder()
        .timeout(UPDATE_TIMEOUT)
        .build()
        .map_err(UpdateCheckError::updater)?;
    let update = updater.check().await.map_err(UpdateCheckError::updater)?;
    let available_version = update.map(|update| update.version);
    controller.check_completed.store(true, Ordering::Release);

    let state = match available_version.as_ref() {
        Some(version) => UpdateUiState {
            phase: UpdatePhase::Available,
            version: Some(version.clone()),
            percentage: None,
            message: None,
        },
        None => UpdateUiState::default(),
    };
    controller.set_state(app, state);

    Ok(UpdateCheckResult {
        current_version,
        available_version,
    })
}

async fn install_available_update(
    app: &AppHandle,
    controller: &Arc<UpdateController>,
) -> Result<(), String> {
    let _operation = controller.acquire_operation()?;
    if let Some(version) = controller.available_version() {
        controller.set_state(
            app,
            progress_state(UpdatePhase::Preparing, &version, None, "Preparing download"),
        );
    }
    let updater = app
        .updater_builder()
        .timeout(UPDATE_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        controller.set_state(app, UpdateUiState::default());
        return Ok(());
    };
    let version = update.version.clone();
    let update = update.restart_after_install(true);

    let _update_lock = process_shutdown::acquire_update_lock()
        .map_err(|message| format!("The update could not be started: {message}"))?
        .ok_or_else(|| {
            "Another AniWorld Desktop window is already installing the update".to_owned()
        })?;

    controller.set_state(
        app,
        progress_state(
            UpdatePhase::Downloading,
            &version,
            Some(0),
            "Downloading update",
        ),
    );
    let download_app = app.clone();
    let download_controller = Arc::clone(controller);
    let download_version = version.clone();
    let verify_app = app.clone();
    let verify_controller = Arc::clone(controller);
    let verify_version = version.clone();
    let mut downloaded = 0_u64;
    let mut last_percentage = Some(0_u8);
    let bytes = update
        .download(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let percentage = content_length.filter(|total| *total > 0).map(|total| {
                    downloaded
                        .saturating_mul(100)
                        .saturating_div(total)
                        .min(100) as u8
                });
                if percentage.is_none() || percentage == last_percentage {
                    return;
                }
                last_percentage = percentage;
                download_controller.set_state(
                    &download_app,
                    progress_state(
                        UpdatePhase::Downloading,
                        &download_version,
                        percentage,
                        "Downloading update",
                    ),
                );
            },
            move || {
                verify_controller.set_state(
                    &verify_app,
                    progress_state(
                        UpdatePhase::Verifying,
                        &verify_version,
                        Some(100),
                        "Verifying update",
                    ),
                );
            },
        )
        .await
        .map_err(|error| error.to_string())?;

    controller.set_state(
        app,
        progress_state(
            UpdatePhase::Preparing,
            &version,
            Some(100),
            "Preparing installation",
        ),
    );
    tauri::async_runtime::spawn_blocking(process_shutdown::close_other_instances)
        .await
        .map_err(|error| format!("Could not close other AniWorld Desktop windows: {error}"))?
        .map_err(|message| format!("Could not close other AniWorld Desktop windows: {message}"))?;

    controller.set_state(
        app,
        progress_state(
            UpdatePhase::Installing,
            &version,
            Some(100),
            "Starting installer. AniWorld Desktop will reopen automatically",
        ),
    );
    let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(INSTALL_STATUS_DELAY)).await;
    update.install(bytes).map_err(|error| error.to_string())?;

    #[cfg(not(target_os = "windows"))]
    app.restart();

    Ok(())
}

fn progress_state(
    phase: UpdatePhase,
    version: &str,
    percentage: Option<u8>,
    message: &str,
) -> UpdateUiState {
    UpdateUiState {
        phase,
        version: Some(version.to_owned()),
        percentage,
        message: Some(message.to_owned()),
    }
}

fn short_error(error: &str) -> String {
    error
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_errors_are_compact_enough_for_the_titlebar_tooltip() {
        let error = format!("Download failed\n{}", "x".repeat(300));
        let error = short_error(&error);

        assert!(!error.contains('\n'));
        assert_eq!(error.chars().count(), 160);
    }

    #[test]
    fn startup_retry_only_accepts_transient_update_errors() {
        assert!(UpdateCheckError::updater(tauri_plugin_updater::Error::ReleaseNotFound).retryable);
        assert!(
            UpdateCheckError::updater(tauri_plugin_updater::Error::Network(
                "connection closed".to_owned()
            ))
            .retryable
        );
        assert!(!UpdateCheckError::updater(tauri_plugin_updater::Error::EmptyEndpoints).retryable);
    }

    #[test]
    fn startup_retry_uses_a_short_bounded_backoff() {
        assert_eq!(STARTUP_CHECK_ATTEMPTS, 4);
        assert_eq!(STARTUP_RETRY_DELAYS[0], Duration::from_secs(2));
        assert_eq!(STARTUP_RETRY_DELAYS[2], Duration::from_secs(10));
    }
}
