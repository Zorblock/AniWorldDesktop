pub fn initialization_script(css: &str, nyan_cat_scrollbar: bool) -> String {
    let css_json = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".to_owned());
    let frame_css = crate::player::stylesheet();
    let embed_player_css_json =
        serde_json::to_string(&frame_css).unwrap_or_else(|_| "\"\"".to_owned());
    let titlebar_script = crate::titlebar::initialization_script();
    let appearance_script = crate::appearance::initialization_script(nyan_cat_scrollbar);
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

  const fullscreenMessageType = "aniworld-desktop-fullscreen-state";
  const fullscreenFrames = new Set();
  const hasLocalFullscreenElement = () =>
    Boolean(document.fullscreenElement || document.webkitFullscreenElement);
  const applyFullscreenLayout = () => {{
    const fullscreenActive = hasLocalFullscreenElement() || fullscreenFrames.size > 0;
    document.documentElement?.classList.toggle(
      "aniworld-desktop-fullscreen",
      fullscreenActive
    );
  }};
  const reportFullscreenLayout = () => {{
    applyFullscreenLayout();
    if (!isTopFrame) {{
      window.top.postMessage({{
        type: fullscreenMessageType,
        active: hasLocalFullscreenElement()
      }}, "*");
    }}
  }};
  if (isTopFrame) {{
    window.addEventListener("message", (event) => {{
      if (event.data?.type !== fullscreenMessageType || !event.source) {{
        return;
      }}
      if (event.data.active) {{
        fullscreenFrames.add(event.source);
      }} else {{
        fullscreenFrames.delete(event.source);
      }}
      applyFullscreenLayout();
    }});
  }}
  const syncFullscreenLayout = () => {{
    reportFullscreenLayout();
  }};
  document.addEventListener("fullscreenchange", syncFullscreenLayout);
  document.addEventListener("webkitfullscreenchange", syncFullscreenLayout);
  window.addEventListener("pagehide", () => {{
    if (!isTopFrame) {{
      window.top.postMessage({{ type: fullscreenMessageType, active: false }}, "*");
    }}
  }});
  syncFullscreenLayout();

  if (!isAniWorld || !isTopFrame) {{
    return;
  }}

  {titlebar_script}
  {appearance_script}

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
"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_the_inline_iframe_ad() {
        let script = initialization_script("", false);

        assert!(script.contains("const isInlineIframeAd"));
        assert!(script.contains("style.height === \"250px\""));
        assert!(script.contains("style.marginBottom === \"10px\""));
        assert!(script.contains("attributeFilter: [\"scrolling\", \"style\"]"));
    }

    #[test]
    fn embeds_all_supported_hoster_switches() {
        let script = initialization_script("", false);

        assert!(script.contains("const isInlineHosterLink"));
        assert!(script.contains("hosterItem?.dataset.externalEmbed === \"false\""));
        assert!(script.contains("document.querySelector(\".inSiteWebStream\")"));
        assert!(script.contains("player.src = linkTarget"));
        assert!(!script.contains("anchor.closest(\".generateInlinePlayer\")"));
    }

    #[test]
    fn isolates_the_filemoon_embed_player() {
        let script = initialization_script("", false);

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
    fn installs_the_custom_titlebar() {
        let script = initialization_script("", false);

        assert!(script.contains("[data-aniworld-titlebar]"));
        assert!(script.contains("Segoe Fluent Icons"));
        assert!(script.contains("titlebarButton(\"back\", \"Back\""));
        assert!(script.contains("titlebarButton(\"close\", \"Close\""));
        assert!(script.contains(concat!("AniWorld Desktop v", env!("CARGO_PKG_VERSION"))));
        assert!(script.contains("settings.html?embedded=1"));
        assert!(script.contains("data-aniworld-settings-overlay"));
        assert!(script.contains("aniworld-desktop-settings-close"));
        assert!(script.contains("titlebarObserver.observe(document"));
        assert!(script.contains("https://aniworld-rpc.invalid/update/install"));
        assert!(script.contains("dataset.aniworldUpdateTooltip"));
        assert!(script.contains("dataset.aniworldUpdateOverlay"));
        assert!(script.contains("Updating AniWorld Desktop"));
        assert!(script.contains("reopen the app"));
        assert!(script.contains("Install Update"));
        assert!(script.contains("updateButton.removeAttribute(\"title\")"));
        assert!(script.contains("height: calc(100vh - 46px) !important"));
        assert!(script.contains("overflow-y: auto !important"));
        assert!(script.contains("body::-webkit-scrollbar-thumb"));
        assert!(script.contains("scrollbar-color: #596477 #111722"));
        assert!(!script.contains("data-aniworld-settings-button"));
    }

    #[test]
    fn resets_native_fullscreen_layout_in_every_frame() {
        let script = initialization_script("", false);

        assert!(script.contains("aniworld-desktop-fullscreen"));
        assert!(script.contains("document.fullscreenElement"));
        assert!(script.contains("document.webkitFullscreenElement"));
        assert!(script.contains("aniworld-desktop-fullscreen-state"));
        assert!(script.contains("fullscreenFrames.add(event.source)"));
        assert!(script.contains("window.top.postMessage"));
        assert!(script.contains("width: 100vw !important"));
        assert!(script.contains("height: 100vh !important"));
        assert!(script.contains("outline: 0 !important"));
        assert!(script.contains("box-shadow: none !important"));
        assert!(script.contains(".jwplayer.jw-flag-fullscreen"));
        assert!(script.contains(":fullscreen > iframe"));
    }
}
