mod adblock;
mod anime_api;
mod appearance;
mod browser_data;
mod player;
mod presence;
mod process_shutdown;
mod settings;
mod titlebar;
mod updater;
mod webview_bridge;
mod webview_injection;
mod webview_network;

use adblock::AdBlocker;
use presence::{Activity, DiscordPresence};
use serde::Serialize;
use settings::{AppSettings, SettingsStore};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    thread,
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use webview_bridge::{
    CoverHandler, NetworkHandlers, PageContext, PlaybackHandler, UpdateHandler, WindowAction,
    WindowHandler,
};

const ANIWORLD_URL: &str = "https://aniworld.to";
const DISCORD_CLIENT_ID: &str = "1542562842379554826";
const APP_TITLE: &str = concat!("AniWorld Desktop v", env!("CARGO_PKG_VERSION"));

struct SettingsRuntime {
    store: Arc<SettingsStore>,
    presence: Arc<DiscordPresence>,
    updater: Arc<updater::UpdateController>,
    data_directory: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBootstrap {
    settings: AppSettings,
    app_version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSaveResult {
    settings: AppSettings,
    restart_required: bool,
}

#[tauri::command]
fn load_settings(state: tauri::State<'_, SettingsRuntime>) -> SettingsBootstrap {
    SettingsBootstrap {
        settings: state.store.get(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    settings: AppSettings,
    state: tauri::State<'_, SettingsRuntime>,
) -> Result<SettingsSaveResult, String> {
    let previous_language = state.store.get().browser_language;
    let settings = state.store.save(settings)?;
    state.presence.update_settings(settings.discord.clone());
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = appearance::apply(&window, settings.appearance.nyan_cat_scrollbar) {
            eprintln!("Could not apply appearance settings: {error}");
        }
    }
    Ok(SettingsSaveResult {
        restart_required: settings.browser_language != previous_language,
        settings,
    })
}

#[tauri::command]
fn reset_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, SettingsRuntime>,
) -> Result<SettingsSaveResult, String> {
    let previous_language = state.store.get().browser_language;
    let settings = state.store.reset()?;
    state.presence.update_settings(settings.discord.clone());
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = appearance::apply(&window, settings.appearance.nyan_cat_scrollbar) {
            eprintln!("Could not apply default appearance settings: {error}");
        }
    }
    Ok(SettingsSaveResult {
        restart_required: settings.browser_language != previous_language,
        settings,
    })
}

#[tauri::command]
fn set_nyan_cat_scrollbar(
    app: tauri::AppHandle,
    enabled: bool,
    state: tauri::State<'_, SettingsRuntime>,
) -> Result<bool, String> {
    let mut settings = state.store.get();
    settings.appearance.nyan_cat_scrollbar = enabled;
    let settings = state.store.save(settings)?;
    let enabled = settings.appearance.nyan_cat_scrollbar;
    if let Some(window) = app.get_webview_window("main") {
        appearance::apply(&window, enabled)
            .map_err(|error| format!("Could not apply the Nyan Cat scrollbar: {error}"))?;
    }
    Ok(enabled)
}

#[tauri::command]
async fn browser_storage_info(
    state: tauri::State<'_, SettingsRuntime>,
) -> Result<browser_data::StorageInfo, String> {
    let data_directory = state.data_directory.clone();
    tauri::async_runtime::spawn_blocking(move || browser_data::storage_info(&data_directory))
        .await
        .map_err(|error| format!("Could not inspect browser storage: {error}"))?
}

#[tauri::command]
async fn clear_browser_data(
    kind: browser_data::BrowserDataKind,
    app: tauri::AppHandle,
    state: tauri::State<'_, SettingsRuntime>,
) -> Result<browser_data::StorageInfo, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "The main browser window is not available".to_owned())?;
    let data_directory = state.data_directory.clone();
    tauri::async_runtime::spawn_blocking(move || {
        browser_data::clear(&window, kind)?;
        browser_data::storage_info(&data_directory)
    })
    .await
    .map_err(|error| format!("Could not finish browser data cleanup: {error}"))?
}

fn schedule_restart(app: tauri::AppHandle) {
    let _ = thread::Builder::new()
        .name("app-restart".to_owned())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(350));
            app.restart();
        });
}

#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    schedule_restart(app);
}

#[tauri::command]
fn reset_all_app_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, SettingsRuntime>,
) -> Result<(), String> {
    browser_data::schedule_full_reset(&state.data_directory)?;
    schedule_restart(app);
    Ok(())
}

#[tauri::command]
async fn check_for_updates(
    app: tauri::AppHandle,
    state: tauri::State<'_, SettingsRuntime>,
) -> Result<updater::UpdateCheckResult, String> {
    updater::check_manually(app, Arc::clone(&state.updater)).await
}

fn user_data_directory(fallback: &Path) -> PathBuf {
    std::env::var_os("APPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_owned())
        .join("zorblock")
        .join("userData")
        .join("AniWorldDesktop")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            reset_settings,
            set_nyan_cat_scrollbar,
            browser_storage_info,
            clear_browser_data,
            restart_app,
            reset_all_app_data,
            check_for_updates
        ])
        .setup(move |app| {
            let url = ANIWORLD_URL.parse()?;
            let data_directory = user_data_directory(app.path().app_data_dir()?.as_path());
            browser_data::perform_pending_full_reset(&data_directory)
                .map_err(std::io::Error::other)?;
            std::fs::create_dir_all(&data_directory)?;
            let blocker = AdBlocker::load(&data_directory);
            let page_context = PageContext::new(ANIWORLD_URL);
            let settings_store = Arc::new(SettingsStore::load(&data_directory));
            let initial_settings = settings_store.get();
            let update_controller = Arc::new(updater::UpdateController::default());
            let presence = Arc::new(DiscordPresence::start(
                DISCORD_CLIENT_ID,
                initial_settings.discord.clone(),
            ));
            app.manage(SettingsRuntime {
                store: Arc::clone(&settings_store),
                presence: Arc::clone(&presence),
                updater: Arc::clone(&update_controller),
                data_directory: data_directory.clone(),
            });
            let presence_on_title = Arc::clone(&presence);
            let presence_on_cover = Arc::clone(&presence);
            let presence_on_playback = Arc::clone(&presence);
            let cover_cache = Arc::new(RwLock::new(HashMap::<String, String>::new()));
            let cover_cache_on_title = Arc::clone(&cover_cache);
            let cover_cache_on_request = Arc::clone(&cover_cache);
            let cover_lookups = Arc::new(Mutex::new(HashSet::<String>::new()));
            let cover_lookups_on_title = Arc::clone(&cover_lookups);
            let current_activity = Arc::new(Mutex::new(Activity::idle()));
            let activity_on_title = Arc::clone(&current_activity);
            let activity_on_cover = Arc::clone(&current_activity);
            let activity_on_playback = Arc::clone(&current_activity);
            let activity_on_lookup = Arc::clone(&current_activity);
            let cover_cache_on_lookup = Arc::clone(&cover_cache);
            let presence_on_lookup = Arc::clone(&presence);
            let navigation_context = page_context.clone();
            let update_on_page_load = Arc::clone(&update_controller);
            let appearance_on_page_load = Arc::clone(&settings_store);
            let mut initialization_script = webview_injection::initialization_script(
                &blocker.cosmetic_css(),
                initial_settings.appearance.nyan_cat_scrollbar,
            );
            if let Some(language_script) = initial_settings
                .browser_language
                .navigator_override_script()
            {
                initialization_script = format!("{language_script}\n{initialization_script}");
            }
            let cover_handler: CoverHandler = Arc::new(move |page_url, cover_url| {
                let Ok(page_url) = tauri::Url::parse(&page_url) else {
                    return;
                };
                let Some(anime_slug) = Activity::anime_slug_from_url(&page_url) else {
                    return;
                };

                let changed = cover_cache_on_request.write().ok().is_some_and(|mut cache| {
                    if cache
                        .get(&anime_slug)
                        .is_some_and(|cached| anime_api::is_anilist_cover_url(cached))
                    {
                        return false;
                    }
                    cache.insert(anime_slug.clone(), cover_url.clone()).as_deref()
                        != Some(cover_url.as_str())
                });
                if !changed {
                    return;
                }

                if let Ok(mut activity) = activity_on_cover.lock() {
                    if activity.anime_slug() == Some(anime_slug.as_str()) {
                        activity.set_cover_url(cover_url);
                        presence_on_cover.update(activity.clone());
                    }
                }
            });
            let playback_handler: PlaybackHandler =
                Arc::new(move |playing, seeking, position_ms, duration_ms, rate_milli| {
                    if let Ok(mut activity) = activity_on_playback.lock() {
                        if activity.set_playback(
                            playing,
                            seeking,
                            position_ms,
                            duration_ms,
                            rate_milli,
                        ) {
                            presence_on_playback.update(activity.clone());
                        }
                    }
                });
            let update_app = app.handle().clone();
            let update_on_install = Arc::clone(&update_controller);
            let update_handler: UpdateHandler = Arc::new(move || {
                updater::install_requested(update_app.clone(), Arc::clone(&update_on_install));
            });
            let window_app = app.handle().clone();
            let window_handler: WindowHandler = Arc::new(move |action| {
                let app = window_app.clone();
                let action_app = app.clone();
                if let Err(error) = app.run_on_main_thread(move || {
                    let Some(window) = action_app.get_webview_window("main") else {
                        return;
                    };
                    let result = match action {
                        WindowAction::Close => window.close(),
                        WindowAction::Drag => window.start_dragging(),
                        WindowAction::Minimize => window.minimize(),
                        WindowAction::ToggleMaximize => window.is_maximized().and_then(|maximized| {
                            if maximized {
                                window.unmaximize()
                            } else {
                                window.maximize()
                            }
                        }),
                    };
                    if let Err(error) = result {
                        eprintln!("Could not execute the window action: {error}");
                    }
                }) {
                    eprintln!("Could not dispatch the window action: {error}");
                }
            });

            let mut window_builder = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External("about:blank".parse()?),
            )
            .title(APP_TITLE)
            .inner_size(1280.0, 800.0)
            .min_inner_size(900.0, 600.0)
            .maximized(initial_settings.start_maximized)
            .decorations(false)
            .shadow(true)
            .center()
            .data_directory(data_directory.clone())
            .general_autofill_enabled(true)
            .initialization_script_for_all_frames(initialization_script)
            .on_page_load(move |window, _payload| {
                update_on_page_load.publish_to_window(&window);
                let enabled = appearance_on_page_load
                    .get()
                    .appearance
                    .nyan_cat_scrollbar;
                if let Err(error) = appearance::apply(&window, enabled) {
                    eprintln!("Could not synchronize appearance settings: {error}");
                }
            })
            .on_navigation(move |url| {
                if matches!(url.scheme(), "http" | "https") {
                    navigation_context.update(url.as_str());
                    true
                } else {
                    url.scheme() == "about"
                }
            })
            .on_new_window(|_url, _features| tauri::webview::NewWindowResponse::Deny)
            .on_document_title_changed(move |window, title| {
                let app_title = if title.trim().is_empty() {
                    APP_TITLE.to_owned()
                } else {
                    format!("{APP_TITLE} — {title}")
                };
                let _ = window.set_title(&app_title);

                if let Ok(url) = window.url() {
                    let mut activity = Activity::from_url(&url, Some(&title));
                    if let Some(cover_url) = activity.anime_slug().and_then(|anime_slug| {
                        cover_cache_on_title
                            .read()
                            .ok()
                            .and_then(|cache| cache.get(anime_slug).cloned())
                    }) {
                        activity.set_cover_url(cover_url);
                    }
                    if let Ok(mut current) = activity_on_title.lock() {
                        *current = activity.clone();
                    }
                    presence_on_title.update(activity.clone());

                    let lookup = activity
                        .anime_slug()
                        .zip(activity.anime_title())
                        .map(|(slug, title)| (slug.to_owned(), title.to_owned()));
                    if let Some((anime_slug, anime_title)) = lookup {
                        let should_lookup = cover_lookups_on_title
                            .lock()
                            .is_ok_and(|mut lookups| lookups.insert(anime_slug.clone()));
                        if should_lookup {
                            let activity_on_lookup = Arc::clone(&activity_on_lookup);
                            let cover_cache_on_lookup = Arc::clone(&cover_cache_on_lookup);
                            let presence_on_lookup = Arc::clone(&presence_on_lookup);
                            let _ = thread::Builder::new()
                                .name("anilist-cover".to_owned())
                                .spawn(move || match anime_api::fetch_cover_url(&anime_title) {
                                    Ok(Some(cover_url)) => {
                                        if let Ok(mut cache) = cover_cache_on_lookup.write() {
                                            cache.insert(anime_slug.clone(), cover_url.clone());
                                        }
                                        if let Ok(mut current) = activity_on_lookup.lock() {
                                            if current.anime_slug() == Some(anime_slug.as_str()) {
                                                current.set_cover_url(cover_url);
                                                presence_on_lookup.update(current.clone());
                                            }
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(error) => eprintln!(
                                        "Could not load the AniList cover for {anime_title}: {error}"
                                    ),
                                });
                        }
                    }
                }
            });
            if let Some(arguments) = initial_settings.browser_language.browser_arguments() {
                window_builder = window_builder.additional_browser_args(&arguments);
            }
            let window = window_builder.build()?;

            webview_network::install_network_filter(
                &window,
                blocker,
                page_context,
                NetworkHandlers::new(
                    cover_handler,
                    playback_handler,
                    update_handler,
                    window_handler,
                ),
            )?;
            window.navigate(url)?;
            if initial_settings.check_updates_on_start {
                updater::check_on_start(app.handle().clone(), Arc::clone(&update_controller));
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("AniWorld Desktop could not be started");
}
