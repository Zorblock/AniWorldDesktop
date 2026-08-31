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
pub type WindowHandler = Arc<dyn Fn(WindowAction) + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowAction {
    Close,
    Drag,
    Minimize,
    ToggleMaximize,
}

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

  const sendWindowAction = (action) => {{
    const params = new URLSearchParams({{ action }});
    fetch(`https://aniworld-rpc.invalid/window?${{params}}`, {{
      cache: "no-store",
      credentials: "omit",
      mode: "no-cors"
    }}).catch(() => {{}});
  }};

  const installTitlebar = () => {{
    if (document.querySelector("[data-aniworld-titlebar]")) {{
      return;
    }}

    const style = document.createElement("style");
    style.dataset.aniworldTitlebar = "true";
    style.textContent = `
      html.aniworld-desktop-framed {{
        min-height: 100% !important;
      }}
      html.aniworld-desktop-framed > body {{
        min-height: calc(100vh - 46px) !important;
        margin: 46px 0 0 !important;
      }}
      [data-aniworld-titlebar] {{
        position: fixed !important;
        inset: 0 0 auto 0 !important;
        z-index: 2147483647 !important;
        display: flex !important;
        align-items: stretch !important;
        width: 100% !important;
        height: 46px !important;
        margin: 0 !important;
        padding: 0 !important;
        border: 0 !important;
        border-bottom: 1px solid rgba(255, 255, 255, 0.08) !important;
        border-radius: 0 !important;
        color: #f7f8ff !important;
        background: #101622 !important;
        box-shadow: 0 4px 18px rgba(0, 0, 0, 0.22) !important;
        font-family: "Segoe UI Variable Text", "Segoe UI", sans-serif !important;
        user-select: none !important;
      }}
      [data-aniworld-titlebar-brand] {{
        display: flex !important;
        align-items: center !important;
        gap: 9px !important;
        min-width: 185px !important;
        padding: 0 15px !important;
        color: #f5f7ff !important;
        font-size: 13px !important;
        font-weight: 600 !important;
        white-space: nowrap !important;
      }}
      [data-aniworld-titlebar-brand] [data-native-icon] {{
        color: #7d8fff !important;
        font-size: 18px !important;
      }}
      [data-aniworld-titlebar-navigation],
      [data-aniworld-window-controls] {{
        display: flex !important;
        align-items: stretch !important;
      }}
      [data-aniworld-titlebar-drag] {{
        display: flex !important;
        flex: 1 1 auto !important;
        align-items: center !important;
        min-width: 36px !important;
        padding: 0 18px !important;
        overflow: hidden !important;
        color: #737e92 !important;
        font-size: 12px !important;
        white-space: nowrap !important;
        cursor: default !important;
      }}
      [data-aniworld-titlebar-page-title] {{
        overflow: hidden !important;
        text-overflow: ellipsis !important;
      }}
      button[data-aniworld-titlebar-button] {{
        display: grid !important;
        place-items: center !important;
        width: 44px !important;
        height: 45px !important;
        min-width: 44px !important;
        margin: 0 !important;
        padding: 0 !important;
        border: 0 !important;
        border-radius: 0 !important;
        outline: 0 !important;
        color: #c5ccda !important;
        background: transparent !important;
        box-shadow: none !important;
        font: inherit !important;
        appearance: none !important;
        cursor: default !important;
        transition: color 100ms ease, background 100ms ease !important;
      }}
      button[data-aniworld-titlebar-button]:hover {{
        color: #ffffff !important;
        background: rgba(255, 255, 255, 0.08) !important;
      }}
      button[data-aniworld-titlebar-button]:active {{
        background: rgba(255, 255, 255, 0.13) !important;
      }}
      button[data-aniworld-titlebar-button]:focus-visible {{
        outline: 2px solid #7184ff !important;
        outline-offset: -3px !important;
      }}
      button[data-aniworld-titlebar-button="settings"] {{
        margin-left: 4px !important;
        border-left: 1px solid rgba(255, 255, 255, 0.06) !important;
      }}
      button[data-aniworld-titlebar-button="close"] {{
        width: 48px !important;
        min-width: 48px !important;
      }}
      button[data-aniworld-titlebar-button="close"]:hover {{
        color: #ffffff !important;
        background: #c42b1c !important;
      }}
      [data-native-icon] {{
        font-family: "Segoe Fluent Icons", "Segoe MDL2 Assets" !important;
        font-size: 15px !important;
        font-style: normal !important;
        font-weight: 400 !important;
        line-height: 1 !important;
        pointer-events: none !important;
      }}
    `;

    const nativeIcon = (glyph) => {{
      const icon = document.createElement("span");
      icon.dataset.nativeIcon = "true";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = glyph;
      return icon;
    }};

    const titlebarButton = (name, label, glyph, action) => {{
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.aniworldTitlebarButton = name;
      button.title = label;
      button.setAttribute("aria-label", label);
      button.appendChild(nativeIcon(glyph));
      button.addEventListener("click", (event) => {{
        event.preventDefault();
        event.stopImmediatePropagation();
        action();
      }});
      return button;
    }};

    const startDragging = (event) => {{
      if (event.button !== 0) {{
        return;
      }}
      event.preventDefault();
      sendWindowAction(event.detail >= 2 ? "toggle-maximize" : "drag");
    }};

    const titlebar = document.createElement("div");
    titlebar.dataset.aniworldTitlebar = "true";
    titlebar.setAttribute("role", "toolbar");
    titlebar.setAttribute("aria-label", "Application controls");

    const brand = document.createElement("div");
    brand.dataset.aniworldTitlebarBrand = "true";
    brand.appendChild(nativeIcon("\uE768"));
    const brandText = document.createElement("span");
    brandText.textContent = "AniWorld Desktop";
    brand.appendChild(brandText);
    brand.addEventListener("mousedown", startDragging);

    const navigation = document.createElement("nav");
    navigation.dataset.aniworldTitlebarNavigation = "true";
    navigation.setAttribute("aria-label", "Browser navigation");
    navigation.append(
      titlebarButton("back", "Back", "\uE72B", () => history.back()),
      titlebarButton("forward", "Forward", "\uE72A", () => history.forward()),
      titlebarButton("home", "Home", "\uE80F", () => location.assign("https://aniworld.to/")),
      titlebarButton("reload", "Reload", "\uE72C", () => location.reload())
    );

    const dragRegion = document.createElement("div");
    dragRegion.dataset.aniworldTitlebarDrag = "true";
    const pageTitle = document.createElement("span");
    pageTitle.dataset.aniworldTitlebarPageTitle = "true";
    const updatePageTitle = () => {{
      pageTitle.textContent = document.title.replace(/^AniWorld Desktop(?: v[^—]+)?\s*—\s*/, "");
    }};
    updatePageTitle();
    new MutationObserver(updatePageTitle).observe(document.querySelector("title") || document.documentElement, {{
      childList: true,
      subtree: true
    }});
    dragRegion.appendChild(pageTitle);
    dragRegion.addEventListener("mousedown", startDragging);

    const settingsButton = titlebarButton("settings", "Settings", "\uE713", openSettings);

    const windowControls = document.createElement("div");
    windowControls.dataset.aniworldWindowControls = "true";
    windowControls.setAttribute("aria-label", "Window controls");
    windowControls.append(
      titlebarButton("minimize", "Minimize", "\uE921", () => sendWindowAction("minimize")),
      titlebarButton("maximize", "Maximize or restore", "\uE922", () => sendWindowAction("toggle-maximize")),
      titlebarButton("close", "Close", "\uE8BB", () => sendWindowAction("close"))
    );

    titlebar.append(brand, navigation, dragRegion, settingsButton, windowControls);
    document.documentElement.classList.add("aniworld-desktop-framed");
    (document.head || document.documentElement).appendChild(style);
    document.body.prepend(titlebar);
  }};

  if (document.body) {{
    installTitlebar();
  }} else {{
    document.addEventListener("DOMContentLoaded", installTitlebar, {{ once: true }});
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
    window_handler: WindowHandler,
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
                window_handler,
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
    window_handler: WindowHandler,
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
            if let Some(action) = window_action_request(&url) {
                let source_url = page_context.current_url();
                if is_aniworld_page(&source_url) {
                    window_handler(action);
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

fn window_action_request(request_url: &str) -> Option<WindowAction> {
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
        "minimize" => Some(WindowAction::Minimize),
        "toggle-maximize" => Some(WindowAction::ToggleMaximize),
        _ => None,
    }
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
    _window_handler: WindowHandler,
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
    fn initialization_script_installs_the_custom_titlebar() {
        let blocker = AdBlocker {
            engine: Engine::new_with_list_text(""),
        };
        let script = blocker.initialization_script();

        assert!(script.contains("[data-aniworld-titlebar]"));
        assert!(script.contains("Segoe Fluent Icons"));
        assert!(script.contains("titlebarButton(\"back\", \"Back\""));
        assert!(script.contains("titlebarButton(\"close\", \"Close\""));
        assert!(script.contains("https://aniworld-rpc.invalid/settings/open"));
        assert!(script.contains("min-height: calc(100vh - 46px) !important"));
        assert!(!script.contains("overflow: auto !important"));
        assert!(!script.contains("data-aniworld-settings-button"));
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
            window_action_request("https://aniworld-rpc.invalid/window?action=unsupported"),
            None
        );
        assert_eq!(
            window_action_request("https://example.com/window?action=close"),
            None
        );
    }

    #[test]
    fn parses_dom_cover_bridge_updates() {
        let cover = "https://aniworld.to/public/img/cover/serial-experiments-lain-stream-cover-VrUstkIXsXoHFWITlqWOOh8egVAtK3QA_220x330.jpg";
        let bridge_url = "https://aniworld-rpc.invalid/cover?url=https%3A%2F%2Faniworld.to%2Fpublic%2Fimg%2Fcover%2Fserial-experiments-lain-stream-cover-VrUstkIXsXoHFWITlqWOOh8egVAtK3QA_220x330.jpg";

        assert_eq!(cover_update(&bridge_url).as_deref(), Some(cover));
    }
}
