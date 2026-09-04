use std::sync::{Arc, RwLock};

pub type CoverHandler = Arc<dyn Fn(String, String) + Send + Sync + 'static>;
pub type PlaybackHandler = Arc<dyn Fn(bool, bool, u64, u64, u32) + Send + Sync + 'static>;
pub type UpdateHandler = Arc<dyn Fn() + Send + Sync + 'static>;
pub type WindowHandler = Arc<dyn Fn(WindowAction) + Send + Sync + 'static>;

#[derive(Clone)]
pub struct NetworkHandlers {
    pub(crate) cover: CoverHandler,
    pub(crate) playback: PlaybackHandler,
    pub(crate) update: UpdateHandler,
    pub(crate) window: WindowHandler,
}

impl NetworkHandlers {
    pub fn new(
        cover: CoverHandler,
        playback: PlaybackHandler,
        update: UpdateHandler,
        window: WindowHandler,
    ) -> Self {
        Self {
            cover,
            playback,
            update,
            window,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowAction {
    Close,
    Drag,
    EnterFullscreen,
    ExitFullscreen,
    Minimize,
    ToggleMaximize,
}

#[derive(Clone)]
pub struct PageContext(Arc<RwLock<String>>);

impl PageContext {
    pub fn new(initial_url: &str) -> Self {
        Self(Arc::new(RwLock::new(initial_url.to_owned())))
    }

    pub fn update(&self, url: &str) {
        if let Ok(mut current_url) = self.0.write() {
            *current_url = url.to_owned();
        }
    }

    pub fn current_url(&self) -> String {
        self.0
            .read()
            .map(|url| url.clone())
            .unwrap_or_else(|_| "https://aniworld.to/".to_owned())
    }
}

pub(crate) fn cover_update(request_url: &str) -> Option<String> {
    let url = tauri::Url::parse(request_url).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("aniworld-rpc.invalid")
        || url.path() != "/cover"
    {
        return None;
    }

    url.query_pairs()
        .find_map(|(key, value)| (key == "url").then(|| value.into_owned()))
}

pub(crate) fn playback_update(request_url: &str) -> Option<(bool, bool, u64, u64, u32)> {
    let url = tauri::Url::parse(request_url).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("aniworld-rpc.invalid")
        || url.path() != "/playback"
    {
        return None;
    }

    let values: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let playing = values.get("playing")?.as_str() == "1";
    let seeking = values.get("seeking").is_some_and(|value| value == "1");
    let position_ms = values.get("position_ms")?.parse().ok()?;
    let duration_ms = values.get("duration_ms")?.parse().ok()?;
    let rate_milli = values.get("rate_milli")?.parse().ok()?;

    if (60_000..=12 * 60 * 60 * 1_000).contains(&duration_ms)
        && position_ms <= duration_ms
        && (250..=4_000).contains(&rate_milli)
    {
        Some((playing, seeking, position_ms, duration_ms, rate_milli))
    } else {
        None
    }
}

pub(crate) fn update_install_request(request_url: &str) -> bool {
    tauri::Url::parse(request_url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("aniworld-rpc.invalid")
            && url.path() == "/update/install"
    })
}

pub(crate) fn window_action_request(request_url: &str) -> Option<WindowAction> {
    let url = tauri::Url::parse(request_url).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("aniworld-rpc.invalid")
        || url.path() != "/window"
    {
        return None;
    }

    match url
        .query_pairs()
        .find_map(|(key, value)| (key == "action").then(|| value.into_owned()))?
        .as_str()
    {
        "close" => Some(WindowAction::Close),
        "drag" => Some(WindowAction::Drag),
        "enter-fullscreen" => Some(WindowAction::EnterFullscreen),
        "exit-fullscreen" => Some(WindowAction::ExitFullscreen),
        "minimize" => Some(WindowAction::Minimize),
        "toggle-maximize" => Some(WindowAction::ToggleMaximize),
        _ => None,
    }
}

pub(crate) fn is_aniworld_page(page_url: &str) -> bool {
    tauri::Url::parse(page_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "aniworld.to" || host.ends_with(".aniworld.to"))
}

pub(crate) fn is_aniworld_episode_page(page_url: &str) -> bool {
    let Ok(url) = tauri::Url::parse(page_url) else {
        return false;
    };
    is_aniworld_page(page_url)
        && url.path_segments().is_some_and(|segments| {
            let segments: Vec<_> = segments.collect();
            segments
                .iter()
                .any(|segment| segment.starts_with("staffel-"))
                && segments
                    .iter()
                    .any(|segment| segment.starts_with("episode-"))
        })
}

pub(crate) fn anime_cover_url(request_url: &str, page_url: &str) -> Option<String> {
    let page_url = tauri::Url::parse(page_url).ok()?;
    if page_url.host_str() != Some("aniworld.to") {
        return None;
    }

    let page_segments: Vec<_> = page_url.path_segments()?.collect();
    let stream_index = page_segments
        .windows(2)
        .position(|pair| pair == ["anime", "stream"])?;
    let anime_slug = page_segments.get(stream_index + 2)?;

    let cover_url = tauri::Url::parse(request_url).ok()?;
    if cover_url.scheme() != "https" || cover_url.host_str() != Some("aniworld.to") {
        return None;
    }

    let filename = cover_url.path_segments()?.next_back()?;
    let expected_prefix = format!("{anime_slug}-stream-cover-");
    if filename.starts_with(&expected_prefix) && filename.contains("_220x330.") {
        Some(cover_url.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_current_anime_cover_request() {
        let page =
            "https://aniworld.to/anime/stream/skeleton-knight-in-another-world/staffel-2/episode-1";
        let cover = "https://aniworld.to/public/img/cover/skeleton-knight-in-another-world-stream-cover-wljf0Pi2tmNCGkT5AUOCNXLPiEsYwnyq_220x330.jpg";

        assert_eq!(anime_cover_url(cover, page).as_deref(), Some(cover));
        assert_eq!(
            anime_cover_url(
                "https://aniworld.to/public/img/cover/another-anime-stream-cover-id_220x330.jpg",
                page
            ),
            None
        );
    }

    #[test]
    fn parses_playback_updates() {
        assert_eq!(
            playback_update(
                "https://aniworld-rpc.invalid/playback?playing=1&seeking=1&position_ms=30000&duration_ms=120000&rate_milli=1000"
            ),
            Some((true, true, 30_000, 120_000, 1_000))
        );
        assert_eq!(
            playback_update(
                "https://aniworld-rpc.invalid/playback?playing=1&position_ms=130000&duration_ms=120000&rate_milli=1000"
            ),
            None
        );
    }

    #[test]
    fn recognizes_only_the_internal_update_route() {
        assert!(update_install_request(
            "https://aniworld-rpc.invalid/update/install"
        ));
        assert!(!update_install_request(
            "https://aniworld-rpc.invalid/update/check"
        ));
        assert!(!update_install_request(
            "https://example.com/update/install"
        ));
    }

    #[test]
    fn parses_only_supported_window_actions() {
        assert_eq!(
            window_action_request("https://aniworld-rpc.invalid/window?action=minimize"),
            Some(WindowAction::Minimize)
        );
        assert_eq!(
            window_action_request("https://aniworld-rpc.invalid/window?action=toggle-maximize"),
            Some(WindowAction::ToggleMaximize)
        );
        assert_eq!(
            window_action_request("https://aniworld-rpc.invalid/window?action=drag"),
            Some(WindowAction::Drag)
        );
        assert_eq!(
            window_action_request("https://aniworld-rpc.invalid/window?action=close"),
            Some(WindowAction::Close)
        );
        assert_eq!(
            window_action_request("https://aniworld-rpc.invalid/window?action=enter-fullscreen"),
            Some(WindowAction::EnterFullscreen)
        );
        assert_eq!(
            window_action_request("https://aniworld-rpc.invalid/window?action=exit-fullscreen"),
            Some(WindowAction::ExitFullscreen)
        );
        assert_eq!(
            window_action_request("https://aniworld-rpc.invalid/window?action=unsupported"),
            None
        );
        assert_eq!(
            window_action_request("https://example.com/window?action=close"),
            None
        );
    }

    #[test]
    fn parses_dom_cover_updates() {
        let cover = "https://aniworld.to/public/img/cover/serial-experiments-lain-stream-cover-VrUstkIXsXoHFWITlqWOOh8egVAtK3QA_220x330.jpg";
        let bridge_url = "https://aniworld-rpc.invalid/cover?url=https%3A%2F%2Faniworld.to%2Fpublic%2Fimg%2Fcover%2Fserial-experiments-lain-stream-cover-VrUstkIXsXoHFWITlqWOOh8egVAtK3QA_220x330.jpg";

        assert_eq!(cover_update(bridge_url).as_deref(), Some(cover));
    }
}
