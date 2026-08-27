use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

const UPDATE_TIMEOUT: Duration = Duration::from_secs(15);

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

    if let Err(error) = update.download_and_install(|_, _| {}, || {}).await {
        app.dialog()
            .message(format!("The update could not be installed:\n\n{error}"))
            .title("Update Failed")
            .kind(MessageDialogKind::Error)
            .show(|_| {});
        return Err(error);
    }

    #[cfg(not(target_os = "windows"))]
    app.restart();

    Ok(())
}
