pub fn initialization_script() -> String {
    format!(
        r#"
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

  const installUpdate = () => {{
    fetch("https://aniworld-rpc.invalid/update/install", {{
      cache: "no-store",
      credentials: "omit",
      mode: "no-cors"
    }}).catch(() => {{}});
  }};

  let renderUpdateState = () => {{}};
  window.__ANIWORLD_UPDATE_STATE__ = window.__ANIWORLD_UPDATE_STATE__ || {{ phase: "hidden" }};
  window.__aniworldSetUpdateState = (state) => {{
    window.__ANIWORLD_UPDATE_STATE__ = state;
    renderUpdateState(state);
  }};

  const installTitlebar = () => {{
    if (document.querySelector("[data-aniworld-titlebar]")) {{
      return;
    }}

    const style = document.createElement("style");
    style.dataset.aniworldTitlebar = "true";
    style.textContent = `
      html.aniworld-desktop-framed {{
        width: 100% !important;
        height: 100% !important;
        min-height: 0 !important;
        overflow: hidden !important;
      }}
      html.aniworld-desktop-framed > body {{
        box-sizing: border-box !important;
        width: 100% !important;
        height: calc(100vh - 46px) !important;
        min-height: calc(100vh - 46px) !important;
        margin: 46px 0 0 !important;
        overflow-x: hidden !important;
        overflow-y: auto !important;
        overscroll-behavior-y: contain !important;
        scrollbar-color: #596477 #111722 !important;
        scrollbar-width: thin !important;
      }}
      html.aniworld-desktop-framed > body::-webkit-scrollbar {{
        width: 10px !important;
        height: 10px !important;
      }}
      html.aniworld-desktop-framed > body::-webkit-scrollbar-track {{
        background: #111722 !important;
      }}
      html.aniworld-desktop-framed > body::-webkit-scrollbar-thumb {{
        min-height: 36px !important;
        border: 2px solid #111722 !important;
        border-radius: 8px !important;
        background: #596477 !important;
      }}
      html.aniworld-desktop-framed > body::-webkit-scrollbar-thumb:hover {{
        background: #707c91 !important;
      }}
      html.aniworld-desktop-framed > body::-webkit-scrollbar-corner {{
        background: #111722 !important;
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
        min-width: 220px !important;
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
      [data-aniworld-update-control] {{
        position: relative !important;
        display: none !important;
        align-items: stretch !important;
        color: #c5ccda !important;
      }}
      [data-aniworld-update-control][data-visible="true"] {{
        display: flex !important;
      }}
      [data-aniworld-update-progress] {{
        display: flex !important;
        align-items: center !important;
        min-width: 34px !important;
        padding: 0 8px 0 0 !important;
        color: #aeb7c8 !important;
        font-size: 11px !important;
        font-variant-numeric: tabular-nums !important;
        white-space: nowrap !important;
      }}
      [data-aniworld-update-progress][hidden] {{
        display: none !important;
      }}
      [data-aniworld-update-tooltip] {{
        position: absolute !important;
        top: 51px !important;
        right: 0 !important;
        width: max-content !important;
        max-width: 280px !important;
        padding: 7px 9px !important;
        border: 1px solid #353d4b !important;
        border-radius: 4px !important;
        opacity: 0 !important;
        color: #e8ebf2 !important;
        background: #1a202a !important;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.28) !important;
        font-size: 12px !important;
        font-weight: 400 !important;
        line-height: 1.35 !important;
        pointer-events: none !important;
        transform: translateY(-2px) !important;
        transition: opacity 100ms ease, transform 100ms ease !important;
      }}
      [data-aniworld-update-control]:hover [data-aniworld-update-tooltip],
      [data-aniworld-update-control]:focus-within [data-aniworld-update-tooltip] {{
        opacity: 1 !important;
        transform: translateY(0) !important;
      }}
      button[data-aniworld-titlebar-button="update"]:disabled {{
        opacity: 0.7 !important;
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
    brandText.textContent = "AniWorld Desktop v{app_version}";
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

    const updateControl = document.createElement("div");
    updateControl.dataset.aniworldUpdateControl = "true";
    const updateButton = titlebarButton("update", "Install update", "\uE896", () => {{
      updateButton.disabled = true;
      updateProgress.hidden = false;
      updateProgress.textContent = "…";
      installUpdate();
    }});
    updateButton.removeAttribute("title");
    const updateProgress = document.createElement("span");
    updateProgress.dataset.aniworldUpdateProgress = "true";
    updateProgress.hidden = true;
    const updateTooltip = document.createElement("span");
    updateTooltip.dataset.aniworldUpdateTooltip = "true";
    updateTooltip.setAttribute("role", "tooltip");
    updateControl.append(updateButton, updateProgress, updateTooltip);

    renderUpdateState = (state) => {{
      const phase = state?.phase || "hidden";
      const version = state?.version || "";
      const busy = ["downloading", "verifying", "preparing", "installing"].includes(phase);
      updateControl.dataset.visible = String(phase !== "hidden");
      updateButton.disabled = busy;

      if (phase === "available") {{
        updateProgress.hidden = true;
        updateProgress.textContent = "";
        updateTooltip.textContent = `Install Update ${{version}}`;
      }} else if (phase === "error") {{
        updateProgress.hidden = false;
        updateProgress.textContent = "Retry";
        updateTooltip.textContent = state?.message
          ? `Update failed: ${{state.message}}. Click to retry.`
          : "Update failed. Click to retry.";
      }} else if (busy) {{
        updateProgress.hidden = false;
        updateProgress.textContent = Number.isFinite(state?.percentage)
          ? `${{state.percentage}}%`
          : "…";
        updateTooltip.textContent = `${{state?.message || "Installing update"}} ${{version}}`;
      }} else {{
        updateProgress.hidden = true;
        updateProgress.textContent = "";
        updateTooltip.textContent = "";
      }}

      updateButton.setAttribute("aria-label", updateTooltip.textContent || "Install update");
    }};
    renderUpdateState(window.__ANIWORLD_UPDATE_STATE__);

    const settingsButton = titlebarButton("settings", "Settings", "\uE713", openSettings);

    const windowControls = document.createElement("div");
    windowControls.dataset.aniworldWindowControls = "true";
    windowControls.setAttribute("aria-label", "Window controls");
    windowControls.append(
      titlebarButton("minimize", "Minimize", "\uE921", () => sendWindowAction("minimize")),
      titlebarButton("maximize", "Maximize or restore", "\uE922", () => sendWindowAction("toggle-maximize")),
      titlebarButton("close", "Close", "\uE8BB", () => sendWindowAction("close"))
    );

    titlebar.append(brand, navigation, dragRegion, updateControl, settingsButton, windowControls);
    document.documentElement.classList.add("aniworld-desktop-framed");
    (document.head || document.documentElement).appendChild(style);
    document.body.prepend(titlebar);
  }};

  if (document.body) {{
    installTitlebar();
  }} else {{
    document.addEventListener("DOMContentLoaded", installTitlebar, {{ once: true }});
  }}
"#,
        app_version = env!("CARGO_PKG_VERSION"),
    )
}
