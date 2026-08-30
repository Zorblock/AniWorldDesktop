use adblock::{
    lists::{FilterSet, ParseOptions},
    request::Request,
    Engine,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};
use tauri::WebviewWindow;

pub type CoverHandler = Arc<dyn Fn(String, String) + Send + Sync + 'static>;
pub type PlaybackHandler = Arc<dyn Fn(bool, bool, u64, u64, u32) + Send + Sync + 'static>;
pub type SettingsHandler = Arc<dyn Fn() + Send + Sync + 'static>;

const EASYLIST_URL: &str = "https://easylist.to/easylist/easylist.txt";
const CACHE_MAX_AGE: Duration = Duration::from_secs(4 * 24 * 60 * 60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(6);
const MAX_FILTER_LIST_SIZE: usize = 10 * 1024 * 1024;

const FALLBACK_FILTERS: &str = r#"[Adblock Plus 2.0]
! Title: AniWorld Desktop fallback filters
||2mdn.net^
||adnxs.com^
||adsterra.com^
||amazon-adsystem.com^
||doubleclick.net^
||exoclick.com^
||googleadservices.com^
||googlesyndication.com^
||juicyads.com^
||onclickads.net^
||outbrain.com^
||popads.net^
||popcash.net^
||propellerads.com^
||scorecardresearch.com^
||taboola.com^
||trafficjunky.net^
/ads.js$script,third-party
/popunder.js$script
"#;

const COSMETIC_FALLBACK_CSS: &str = r#"
.adsbygoogle,
.ad-banner,
.ad-container,
.advertisement,
[id^="google_ads_"],
[id^="ad-container"],
[class~="advertisement"],
iframe[src*="doubleclick.net"],
iframe[src*="googlesyndication.com"] {
  display: none !important;
}
"#;

const EMBED_PLAYER_CSS: &str = r#"
body.video-embed-mode {
  width: 100% !important;
  height: 100vh !important;
  min-height: 0 !important;
  margin: 0 !important;
  padding: 0 !important;
  overflow: hidden !important;
  background: #000 !important;
}
body.video-embed-mode > :not(#root),
body.video-embed-mode #root > :not(.video-embed-page),
body.video-embed-mode .video-embed-page > :not(.video-page__player),
body.video-embed-mode .video-page__player > :not(.video-page__player-frame) {
  display: none !important;
}
body.video-embed-mode #root,
body.video-embed-mode .video-embed-page,
body.video-embed-mode .video-page__player,
body.video-embed-mode .video-page__player-frame {
  width: 100% !important;
  height: 100% !important;
  min-height: 0 !important;
  max-width: none !important;
  margin: 0 !important;
  padding: 0 !important;
}
body.video-embed-mode .video-page__player,
body.video-embed-mode .video-page__player-frame {
  position: fixed !important;
  inset: 0 !important;
}
"#;

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

pub struct AdBlocker {
    engine: Engine,
}

impl AdBlocker {
    pub fn load(user_data_directory: &Path) -> Arc<Self> {
        let list = load_filter_list(user_data_directory);
        let mut filter_set = FilterSet::new(false);
        filter_set.add_filter_list(FALLBACK_FILTERS.to_owned(), ParseOptions::default());
        filter_set.add_filter_list(list, ParseOptions::default());

        Arc::new(Self {
            engine: Engine::new_with_filter_set(filter_set),
        })
    }

    pub fn blocks(&self, url: &str, source_url: &str, request_type: &str, method: &str) -> bool {
        Request::new(url, source_url, request_type, method)
            .map(|request| self.engine.check_network_request(&request).should_block())
            .unwrap_or(false)
    }

    pub fn initialization_script(&self) -> String {
        let mut selectors: Vec<_> = self
            .engine
            .url_cosmetic_resources("https://aniworld.to/")
            .hide_selectors
            .into_iter()
            .collect();
        selectors.sort_unstable();

        let mut css = String::from(COSMETIC_FALLBACK_CSS);
        for selector in selectors {
            css.push_str(&selector);
            css.push_str(" { display: none !important; }\n");
        }

        let css_json = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".to_owned());
        let embed_player_css_json =
            serde_json::to_string(EMBED_PLAYER_CSS).unwrap_or_else(|_| "\"\"".to_owned());
        format!(
            r#"
(() => {{
  Object.defineProperty(window, "open", {{
    configurable: false,
    writable: false,
    value: () => null
  }});

  const host = location.hostname.toLowerCase();
  const isAniWorld = host === "aniworld.to" || host.endsWith(".aniworld.to");
  const isFilemoon = host === "filemoon.to" || host.endsWith(".filemoon.to");
  const isTopFrame = window.top === window;

  if (isFilemoon && location.pathname.startsWith("/d/")) {{
    const embedUrl = new URL(location.href);
    embedUrl.pathname = location.pathname.replace(/^\/d\//, "/e/");
    location.replace(embedUrl.href);
    return;
  }}

  const stopPopupLink = (event) => {{
    const target = event.target;
    const anchor = target instanceof Element ? target.closest('a[target="_blank"]') : null;
    if (!anchor) {{
      return;
    }}

    const hosterItem = anchor.matches("a.watchEpisode")
      ? anchor.closest("li[data-link-target]")
      : null;
    const isInlineHosterLink =
      isTopFrame &&
      isAniWorld &&
      hosterItem?.dataset.externalEmbed === "false";
    if (isInlineHosterLink) {{
      event.preventDefault();
      const playerContainer = document.querySelector(".inSiteWebStream");
      const player = playerContainer?.querySelector("iframe");
      const linkTarget = hosterItem.dataset.linkTarget;
      if (player && linkTarget) {{
        playerContainer.style.display = "inline-block";
        player.style.display = "inline-block";
        document.querySelectorAll(".fakePlayer").forEach((element) => {{
          element.style.display = "none";
        }});
        player.src = linkTarget;
      }}
      return;
    }}

    const isTrustedAniWorldLink =
      isTopFrame &&
      isAniWorld &&
      anchor.matches("a.watchEpisode, a.originalLinkTarget");

    event.preventDefault();
    event.stopImmediatePropagation();

    if (isTrustedAniWorldLink) {{
      location.assign(anchor.href);
    }}
  }};

  document.addEventListener("click", stopPopupLink, true);
  document.addEventListener("auxclick", stopPopupLink, true);

  const playbackMessageType = "aniworld-desktop-playback";
  const forwardPlayback = (playback) => {{
    const params = new URLSearchParams({{
      playing: playback.playing ? "1" : "0",
      seeking: playback.seeking ? "1" : "0",
      position_ms: String(playback.positionMs),
      duration_ms: String(playback.durationMs),
      rate_milli: String(playback.rateMilli)
    }});
    fetch(`https://aniworld-rpc.invalid/playback?${{params}}`, {{
      cache: "no-store",
      credentials: "omit",
      mode: "no-cors"
    }}).catch(() => {{}});
  }};

  if (isTopFrame) {{
    window.addEventListener("message", (event) => {{
      if (event.data?.type === playbackMessageType) {{
        forwardPlayback(event.data);
      }}
    }});
  }}

  const sendPlayback = (playback) => {{
    const message = {{ type: playbackMessageType, ...playback }};
    if (isTopFrame) {{
      forwardPlayback(message);
    }} else {{
      window.top.postMessage(message, "*");
    }}
  }};

  let lastPlaybackState = "";
  let lastPlaybackSentAt = 0;
  let pendingPlaybackReport;
  const pauseGraceUntil = new WeakMap();
  const reportVideo = (video, force = false) => {{
    const duration = Number(video.duration);
    const position = Number(video.currentTime);
    const rate = Number(video.playbackRate);
    if (!Number.isFinite(duration) || duration < 60 || duration > 43200 ||
        !Number.isFinite(position) || !Number.isFinite(rate)) {{
      return;
    }}

    const now = Date.now();
    const playing = !video.paused && !video.ended && video.readyState >= 2;
    if (!playing && !video.seeking && !video.ended && now < (pauseGraceUntil.get(video) || 0)) {{
      return;
    }}

    const playback = {{
      playing,
      seeking: video.seeking,
      positionMs: Math.round(Math.max(0, Math.min(duration, position)) * 1000),
      durationMs: Math.round(duration * 1000),
      rateMilli: Math.round(Math.max(0.25, Math.min(4, rate)) * 1000)
    }};
    const stableState = `${{playback.playing}}|${{playback.seeking}}|${{playback.durationMs}}|${{playback.rateMilli}}`;
    if (!force && stableState === lastPlaybackState && now - lastPlaybackSentAt < 10000) {{
      return;
    }}
    lastPlaybackState = stableState;
    lastPlaybackSentAt = now;
    sendPlayback(playback);
  }};

  const scheduleVideoReport = (video, delay = 150) => {{
    clearTimeout(pendingPlaybackReport);
    pendingPlaybackReport = setTimeout(() => reportVideo(video, true), delay);
  }};

  const trackedVideos = new WeakSet();
  const attachVideo = (video) => {{
    if (trackedVideos.has(video)) {{
      return;
    }}
    trackedVideos.add(video);
    ["ended", "loadedmetadata", "durationchange", "ratechange"]
      .forEach((eventName) => video.addEventListener(eventName, () => scheduleVideoReport(video)));
    video.addEventListener("play", () => {{
      pauseGraceUntil.delete(video);
      scheduleVideoReport(video);
    }});
    video.addEventListener("seeking", () => reportVideo(video, true));
    video.addEventListener("seeked", () => {{
      pauseGraceUntil.set(video, Date.now() + 700);
      scheduleVideoReport(video, 750);
    }});
    video.addEventListener("pause", () => {{
      pauseGraceUntil.set(video, Date.now() + 700);
      scheduleVideoReport(video, 750);
    }});
    video.addEventListener("timeupdate", () => reportVideo(video));
    reportVideo(video, true);
  }};

  const attachVideos = () => document.querySelectorAll("video").forEach(attachVideo);
  attachVideos();
  const observeVideos = () => {{
    if (document.documentElement) {{
      new MutationObserver(attachVideos).observe(
        document.documentElement,
        {{ childList: true, subtree: true }}
      );
    }}
  }};
  if (document.documentElement) {{
    observeVideos();
  }} else {{
    document.addEventListener("DOMContentLoaded", observeVideos, {{ once: true }});
  }}
  setInterval(() => {{
    attachVideos();
    document.querySelectorAll("video").forEach((video) => reportVideo(video));
  }}, 5000);

  const isInlineIframeAd = (element) => {{
    if (!(element instanceof HTMLIFrameElement)) {{
      return false;
    }}

    const style = element.style;
    return element.getAttribute("scrolling")?.toLowerCase() === "no" &&
      style.height === "250px" &&
      style.width === "100%" &&
      style.marginBottom === "10px" &&
      style.borderRadius === "10px" &&
      style.getPropertyPriority("display") === "important";
  }};

  const removeInlineIframeAds = (root) => {{
    if (isInlineIframeAd(root)) {{
      root.remove();
      return;
    }}
    root.querySelectorAll?.("iframe").forEach((iframe) => {{
      if (isInlineIframeAd(iframe)) {{
        iframe.remove();
      }}
    }});
  }};

  const observeInlineIframeAds = () => {{
    removeInlineIframeAds(document);
    if (!document.documentElement) {{
      return;
    }}
    new MutationObserver((mutations) => {{
      mutations.forEach((mutation) => {{
        if (mutation.type === "attributes") {{
          removeInlineIframeAds(mutation.target);
        }} else {{
          mutation.addedNodes.forEach(removeInlineIframeAds);
        }}
      }});
    }}).observe(document.documentElement, {{
      attributes: true,
      attributeFilter: ["scrolling", "style"],
      childList: true,
      subtree: true
    }});
  }};

  if (document.documentElement) {{
    observeInlineIframeAds();
  }} else {{
    document.addEventListener("DOMContentLoaded", observeInlineIframeAds, {{ once: true }});
  }}

  const installEmbedPlayerFilters = () => {{
    if (document.querySelector("style[data-aniworld-embed-player]")) {{
      return;
    }}
    const style = document.createElement("style");
    style.dataset.aniworldEmbedPlayer = "true";
    style.textContent = {embed_player_css_json};
    (document.head || document.documentElement).appendChild(style);
  }};

  if (document.documentElement) {{
    installEmbedPlayerFilters();
  }} else {{
    document.addEventListener("DOMContentLoaded", installEmbedPlayerFilters, {{ once: true }});
  }}

  if (!isAniWorld || !isTopFrame) {{
    return;
  }}

  const openSettings = () => {{
    fetch("https://aniworld-rpc.invalid/settings/open", {{
      cache: "no-store",
      credentials: "omit",
      mode: "no-cors"
    }}).catch(() => {{}});
  }};

  const installSettingsButton = () => {{
    if (document.querySelector("button[data-aniworld-settings-button]")) {{
      return;
    }}

    const style = document.createElement("style");
    style.dataset.aniworldSettingsButton = "true";
    style.textContent = `
      button[data-aniworld-settings-button] {{
        position: fixed !important;
        top: 12px !important;
        left: 12px !important;
        z-index: 2147483647 !important;
        display: grid !important;
        place-items: center !important;
        width: 42px !important;
        height: 42px !important;
        margin: 0 !important;
        padding: 0 !important;
        border: 1px solid rgba(255, 255, 255, 0.16) !important;
        border-radius: 12px !important;
        color: #f7f8ff !important;
        background: rgba(15, 20, 31, 0.9) !important;
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.28) !important;
        backdrop-filter: blur(14px) !important;
        cursor: pointer !important;
        transition: background 120ms ease, border-color 120ms ease, transform 120ms ease !important;
      }}
      button[data-aniworld-settings-button]:hover {{
        border-color: rgba(113, 132, 255, 0.7) !important;
        background: rgba(35, 43, 67, 0.96) !important;
      }}
      button[data-aniworld-settings-button]:active {{
        transform: scale(0.95) !important;
      }}
      button[data-aniworld-settings-button]:focus-visible {{
        outline: 2px solid #7184ff !important;
        outline-offset: 2px !important;
      }}
      button[data-aniworld-settings-button] svg {{
        width: 21px !important;
        height: 21px !important;
        fill: currentColor !important;
        pointer-events: none !important;
      }}
    `;

    const button = document.createElement("button");
    button.type = "button";
    button.dataset.aniworldSettingsButton = "true";
    button.title = "Settings";
    button.setAttribute("aria-label", "Open settings");
    button.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19.43 12.98c.04-.32.07-.65.07-.98s-.03-.66-.08-.98l2.11-1.65a.5.5 0 0 0 .12-.64l-2-3.46a.5.5 0 0 0-.61-.22l-2.49 1a7.3 7.3 0 0 0-1.69-.98L14.5 2.42A.49.49 0 0 0 14 2h-4a.49.49 0 0 0-.49.42l-.38 2.65c-.61.25-1.17.59-1.69.98l-2.49-1a.49.49 0 0 0-.61.22l-2 3.46a.49.49 0 0 0 .12.64l2.11 1.65c-.04.32-.08.67-.08.98s.03.66.08.98l-2.11 1.65a.5.5 0 0 0-.12.64l2 3.46a.5.5 0 0 0 .61.22l2.49-1c.52.4 1.08.73 1.69.98l.38 2.65c.04.24.24.42.49.42h4c.25 0 .46-.18.49-.42l.38-2.65c.61-.25 1.17-.58 1.69-.98l2.49 1c.23.08.49 0 .61-.22l2-3.46a.5.5 0 0 0-.12-.64l-2.11-1.65ZM12 15.5A3.5 3.5 0 1 1 12 8a3.5 3.5 0 0 1 0 7.5Z"/></svg>`;
    button.addEventListener("click", (event) => {{
      event.preventDefault();
      event.stopImmediatePropagation();
      openSettings();
    }});
    (document.body || document.documentElement).appendChild(style);
    (document.body || document.documentElement).appendChild(button);
  }};

  if (document.body) {{
    installSettingsButton();
  }} else {{
    document.addEventListener("DOMContentLoaded", installSettingsButton, {{ once: true }});
  }}

  let lastCoverUrl = "";
  const reportAnimeCover = () => {{
    const image = document.querySelector(".seriesCoverBox img[itemprop='image'], .seriesCoverBox img");
    const source = image?.getAttribute("data-src") || image?.getAttribute("src");
    if (!source) {{
      return;
    }}

    let coverUrl;
    try {{
      coverUrl = new URL(source, location.href).href;
    }} catch {{
      return;
    }}
    if (coverUrl === lastCoverUrl) {{
      return;
    }}
    lastCoverUrl = coverUrl;

    const params = new URLSearchParams({{ url: coverUrl }});
    fetch(`https://aniworld-rpc.invalid/cover?${{params}}`, {{
      cache: "no-store",
      credentials: "omit",
      mode: "no-cors"
    }}).catch(() => {{}});
  }};

  const observeAnimeCover = () => {{
    reportAnimeCover();
    if (document.documentElement) {{
      new MutationObserver(reportAnimeCover).observe(
        document.documentElement,
        {{ attributes: true, attributeFilter: ["src", "data-src"], childList: true, subtree: true }}
      );
    }}
  }};
  if (document.documentElement) {{
    observeAnimeCover();
  }} else {{
    document.addEventListener("DOMContentLoaded", observeAnimeCover, {{ once: true }});
  }}

  const installCosmeticFilters = () => {{
    if (document.querySelector("style[data-aniworld-adblock]")) {{
      return;
    }}
    const style = document.createElement("style");
    style.dataset.aniworldAdblock = "true";
    style.textContent = {css_json};
    (document.head || document.documentElement).appendChild(style);
  }};

  if (document.documentElement) {{
    installCosmeticFilters();
  }} else {{
    document.addEventListener("DOMContentLoaded", installCosmeticFilters, {{ once: true }});
  }}
}})();
"#
        )
    }
}

fn cache_path(user_data_directory: &Path) -> PathBuf {
    user_data_directory.join("adblock").join("easylist.txt")
}

fn cache_is_fresh(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age <= CACHE_MAX_AGE)
}

fn looks_like_filter_list(contents: &str) -> bool {
    contents.len() >= 100_000
        && contents.len() <= MAX_FILTER_LIST_SIZE
        && contents
            .lines()
            .take(20)
            .any(|line| line.starts_with("[Adblock Plus") || line.starts_with("! Title: EasyList"))
}

fn read_valid_cache(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .filter(|contents| looks_like_filter_list(contents))
}

fn download_filter_list() -> Result<String, Box<dyn std::error::Error>> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let contents = agent
        .get(EASYLIST_URL)
        .header("User-Agent", "AniWorldDesktop/0.1")
        .call()?
        .body_mut()
        .with_config()
        .limit(MAX_FILTER_LIST_SIZE as u64)
        .read_to_string()?;

    if looks_like_filter_list(&contents) {
        Ok(contents)
    } else {
        Err("The EasyList response is not a valid filter list".into())
    }
}

fn load_filter_list(user_data_directory: &Path) -> String {
    let path = cache_path(user_data_directory);
    let cached = read_valid_cache(&path);

    if cache_is_fresh(&path) {
        if let Some(contents) = cached {
            return contents;
        }
    }

    match download_filter_list() {
        Ok(contents) => {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(error) = fs::write(&path, &contents) {
                eprintln!("Could not save the EasyList cache: {error}");
            }
            contents
        }
        Err(error) => {
            eprintln!("Could not update EasyList: {error}");
            cached.unwrap_or_default()
        }
    }
}

#[cfg(windows)]
pub fn install_network_filter(
    window: &WebviewWindow,
    blocker: Arc<AdBlocker>,
    page_context: PageContext,
    cover_handler: CoverHandler,
    playback_handler: PlaybackHandler,
    settings_handler: SettingsHandler,
) -> tauri::Result<()> {
    window.with_webview(move |platform_webview| {
        if let Err(error) = unsafe {
            install_webview2_network_filter(
                platform_webview,
                blocker,
                page_context,
                cover_handler,
                playback_handler,
                settings_handler,
            )
        } {
            eprintln!("Could not enable the WebView2 ad blocker: {error}");
        }
    })
}

#[cfg(windows)]
unsafe fn install_webview2_network_filter(
    platform_webview: tauri::webview::PlatformWebview,
    blocker: Arc<AdBlocker>,
    page_context: PageContext,
    cover_handler: CoverHandler,
    playback_handler: PlaybackHandler,
    settings_handler: SettingsHandler,
) -> windows::core::Result<()> {
    use webview2_com::{
        take_pwstr, Microsoft::Web::WebView2::Win32::*, WebResourceRequestedEventHandler,
    };
    use windows::core::{Interface, HSTRING, PWSTR};

    let webview = platform_webview.controller().CoreWebView2()?;
    let environment = platform_webview.environment();
    let filter = HSTRING::from("*");

    if let Ok(webview_22) = webview.cast::<ICoreWebView2_22>() {
        webview_22.AddWebResourceRequestedFilterWithRequestSourceKinds(
            &filter,
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        )?;
    } else {
        webview.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
    }

    let mut registration_token = 0_i64;
    webview.add_WebResourceRequested(
        &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };

            let request = args.Request()?;
            let url = {
                let mut value = PWSTR::null();
                request.Uri(&mut value)?;
                take_pwstr(value)
            };
            if let Some(candidate) = cover_update(&url) {
                let source_url = page_context.current_url();
                if let Some(cover_url) = anime_cover_url(&candidate, &source_url) {
                    cover_handler(source_url, cover_url);
                }
                let status = HSTRING::from("No Content");
                let headers =
                    HSTRING::from("Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
                return Ok(());
            }
            if let Some((playing, seeking, position_ms, duration_ms, rate_milli)) =
                playback_update(&url)
            {
                let source_url = page_context.current_url();
                if is_aniworld_episode_page(&source_url) {
                    playback_handler(playing, seeking, position_ms, duration_ms, rate_milli);
                }
                let status = HSTRING::from("No Content");
                let headers =
                    HSTRING::from("Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
                return Ok(());
            }
            if settings_open_request(&url) {
                let source_url = page_context.current_url();
                if is_aniworld_page(&source_url) {
                    settings_handler();
                }
                let status = HSTRING::from("No Content");
                let headers =
                    HSTRING::from("Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
                return Ok(());
            }
            let method = {
                let mut value = PWSTR::null();
                request.Method(&mut value)?;
                take_pwstr(value)
            };
            let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_OTHER;
            args.ResourceContext(&mut context)?;

            let source_url = page_context.current_url();
            if let Some(cover_url) = anime_cover_url(&url, &source_url) {
                cover_handler(source_url.clone(), cover_url);
            }
            let request_type = webview2_request_type(context);
            if blocker.blocks(&url, &source_url, request_type, &method) {
                let status = HSTRING::from("No Content");
                let headers = HSTRING::from("Cache-Control: no-store\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
            }

            Ok(())
        })),
        &mut registration_token,
    )?;

    Ok(())
}

fn cover_update(request_url: &str) -> Option<String> {
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

fn playback_update(request_url: &str) -> Option<(bool, bool, u64, u64, u32)> {
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

fn settings_open_request(request_url: &str) -> bool {
    tauri::Url::parse(request_url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("aniworld-rpc.invalid")
            && url.path() == "/settings/open"
    })
}

fn is_aniworld_page(page_url: &str) -> bool {
    tauri::Url::parse(page_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "aniworld.to" || host.ends_with(".aniworld.to"))
}

fn is_aniworld_episode_page(page_url: &str) -> bool {
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

fn anime_cover_url(request_url: &str, page_url: &str) -> Option<String> {
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

#[cfg(windows)]
fn webview2_request_type(
    context: webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT,
) -> &'static str {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;

    match context {
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => "document",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => "media",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST => "xmlhttprequest",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => "fetch",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING => "ping",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => "websocket",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_CSP_VIOLATION_REPORT => "csp_report",
        _ => "other",
    }
}

#[cfg(not(windows))]
pub fn install_network_filter(
    _window: &WebviewWindow,
    _blocker: Arc<AdBlocker>,
    _page_context: PageContext,
    _cover_handler: CoverHandler,
    _playback_handler: PlaybackHandler,
    _settings_handler: SettingsHandler,
) -> tauri::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_blocks_a_known_ad_domain() {
        let mut filter_set = FilterSet::new(false);
        filter_set.add_filter_list(FALLBACK_FILTERS.to_owned(), ParseOptions::default());
        let blocker = AdBlocker {
            engine: Engine::new_with_filter_set(filter_set),
        };

        assert!(blocker.blocks(
            "https://securepubads.g.doubleclick.net/tag.js",
            "https://aniworld.to/",
            "script",
            "GET"
        ));
        assert!(!blocker.blocks(
            "https://aniworld.to/",
            "https://aniworld.to/",
            "document",
            "GET"
        ));
    }

    #[test]
    fn initialization_script_removes_the_inline_iframe_ad() {
        let blocker = AdBlocker {
            engine: Engine::new_with_list_text(""),
        };
        let script = blocker.initialization_script();

        assert!(script.contains("const isInlineIframeAd"));
        assert!(script.contains("style.height === \"250px\""));
        assert!(script.contains("style.marginBottom === \"10px\""));
        assert!(script.contains("attributeFilter: [\"scrolling\", \"style\"]"));
    }

    #[test]
    fn initialization_script_embeds_all_supported_hoster_switches() {
        let blocker = AdBlocker {
            engine: Engine::new_with_list_text(""),
        };
        let script = blocker.initialization_script();

        assert!(script.contains("const isInlineHosterLink"));
        assert!(script.contains("hosterItem?.dataset.externalEmbed === \"false\""));
        assert!(script.contains("document.querySelector(\".inSiteWebStream\")"));
        assert!(script.contains("player.src = linkTarget"));
        assert!(!script.contains("anchor.closest(\".generateInlinePlayer\")"));
    }

    #[test]
    fn initialization_script_isolates_the_filemoon_embed_player() {
        let blocker = AdBlocker {
            engine: Engine::new_with_list_text(""),
        };
        let script = blocker.initialization_script();

        assert!(script.contains("style[data-aniworld-embed-player]"));
        assert!(script.contains("host === \"filemoon.to\""));
        assert!(script.contains("location.pathname.startsWith(\"/d/\")"));
        assert!(script.contains("location.pathname.replace(/^\\/d\\//, \"/e/\")"));
        assert!(script.contains("location.replace(embedUrl.href)"));
        assert!(script.contains("body.video-embed-mode > :not(#root)"));
        assert!(script.contains(".video-embed-page > :not(.video-page__player)"));
        assert!(script.contains(".video-page__player-frame"));
    }

    #[test]
    fn rejects_small_or_unrelated_cache_files() {
        assert!(!looks_like_filter_list(
            "[Adblock Plus 2.0]\n||example.test^"
        ));
        assert!(!looks_like_filter_list(&"x".repeat(100_000)));
    }

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
    fn parses_playback_bridge_updates() {
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
    fn recognizes_only_the_internal_settings_bridge() {
        assert!(settings_open_request(
            "https://aniworld-rpc.invalid/settings/open"
        ));
        assert!(!settings_open_request(
            "https://aniworld-rpc.invalid/settings/close"
        ));
        assert!(!settings_open_request("https://example.com/settings/open"));
    }

    #[test]
    fn initialization_script_installs_the_settings_button() {
        let blocker = AdBlocker {
            engine: Engine::new_with_list_text(""),
        };
        let script = blocker.initialization_script();

        assert!(script.contains("button[data-aniworld-settings-button]"));
        assert!(script.contains("aria-label\", \"Open settings"));
        assert!(script.contains("https://aniworld-rpc.invalid/settings/open"));
    }

    #[test]
    fn parses_dom_cover_bridge_updates() {
        let cover = "https://aniworld.to/public/img/cover/serial-experiments-lain-stream-cover-VrUstkIXsXoHFWITlqWOOh8egVAtK3QA_220x330.jpg";
        let bridge_url = "https://aniworld-rpc.invalid/cover?url=https%3A%2F%2Faniworld.to%2Fpublic%2Fimg%2Fcover%2Fserial-experiments-lain-stream-cover-VrUstkIXsXoHFWITlqWOOh8egVAtK3QA_220x330.jpg";

        assert_eq!(cover_update(&bridge_url).as_deref(), Some(cover));
    }
}
