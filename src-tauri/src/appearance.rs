use tauri::WebviewWindow;

const STORAGE_KEY: &str = "aniworld-desktop-nyan-scrollbar";
const NYAN_HORIZONTAL: &str = include_str!("../assets/nyan-scroll/nyan-hor.svg");
const NYAN_VERTICAL: &str = include_str!("../assets/nyan-scroll/nyan-vert.svg");
const RAINBOW_HORIZONTAL: &str = include_str!("../assets/nyan-scroll/rainbow-hor.svg");
const RAINBOW_VERTICAL: &str = include_str!("../assets/nyan-scroll/rainbow-vert.svg");

fn svg_data_url(svg: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(svg.len() * 2);
    encoded.push_str("data:image/svg+xml,");
    for byte in svg.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
        }
    }
    encoded
}

pub fn stylesheet() -> String {
    let nyan_horizontal = svg_data_url(NYAN_HORIZONTAL);
    let nyan_vertical = svg_data_url(NYAN_VERTICAL);
    let rainbow_horizontal = svg_data_url(RAINBOW_HORIZONTAL);
    let rainbow_vertical = svg_data_url(RAINBOW_VERTICAL);

    format!(
        r#"
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body {{
  scrollbar-color: auto !important;
  scrollbar-width: auto !important;
}}
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar,
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar-corner,
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar-track,
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar-track-piece:end {{
  width: 16px !important;
  height: 16px !important;
  border: 0 !important;
  background-color: #003366 !important;
  background-image: radial-gradient(circle, #ffffff 0 1px, transparent 1.5px) !important;
  background-size: 17px 17px !important;
}}
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar-track-piece:vertical:start {{
  background-color: #003366 !important;
  background-image: url("{rainbow_vertical}") !important;
  background-position: bottom right !important;
  background-repeat: repeat-y !important;
  background-size: 100% 14px !important;
}}
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar-thumb:vertical {{
  min-height: 32px !important;
  border: 0 !important;
  border-radius: 0 !important;
  background-color: transparent !important;
  background-image: url("{nyan_vertical}"), url("{rainbow_vertical}") !important;
  background-position: bottom right, top right !important;
  background-repeat: no-repeat, repeat-y !important;
  background-size: 100% auto, 100% 14px !important;
}}
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar-track-piece:horizontal:start {{
  background-color: #003366 !important;
  background-image: url("{rainbow_horizontal}") !important;
  background-position: bottom right !important;
  background-repeat: repeat-x !important;
  background-size: 14px 100% !important;
}}
html.aniworld-nyan-scrollbar.aniworld-desktop-framed > body::-webkit-scrollbar-thumb:horizontal {{
  min-width: 32px !important;
  border: 0 !important;
  border-radius: 0 !important;
  background-color: transparent !important;
  background-image: url("{nyan_horizontal}"), url("{rainbow_horizontal}") !important;
  background-position: bottom right, top left !important;
  background-repeat: no-repeat, repeat-x !important;
  background-size: auto 100%, 14px 100% !important;
}}
"#
    )
}

pub fn initialization_script(initially_enabled: bool) -> String {
    let css = serde_json::to_string(&stylesheet()).unwrap_or_else(|_| "\"\"".to_owned());
    format!(
        r#"
  (() => {{
    const storageKey = "{STORAGE_KEY}";
    let nyanScrollbarEnabled = {initially_enabled};
    try {{
      const storedValue = localStorage.getItem(storageKey);
      if (storedValue !== null) {{
        nyanScrollbarEnabled = storedValue === "1";
      }}
    }} catch (_) {{}}

    const installNyanScrollbar = () => {{
      if (!document.documentElement) {{
        return false;
      }}
      if (!document.querySelector("style[data-aniworld-appearance]")) {{
        const style = document.createElement("style");
        style.dataset.aniworldAppearance = "true";
        style.textContent = {css};
        (document.head || document.documentElement).appendChild(style);
      }}
      document.documentElement.classList.toggle(
        "aniworld-nyan-scrollbar",
        nyanScrollbarEnabled
      );
      return true;
    }};

    if (!installNyanScrollbar()) {{
      const appearanceObserver = new MutationObserver(() => {{
        if (installNyanScrollbar()) {{
          appearanceObserver.disconnect();
        }}
      }});
      appearanceObserver.observe(document, {{ childList: true, subtree: true }});
    }}
  }})();
"#
    )
}

pub fn apply(window: &WebviewWindow, enabled: bool) -> tauri::Result<()> {
    let css = serde_json::to_string(&stylesheet()).unwrap_or_else(|_| "\"\"".to_owned());
    window.eval(format!(
        r#"try {{
  localStorage.setItem("{STORAGE_KEY}", "{}");
}} catch (_) {{}}
const appearanceRoot = document.documentElement;
if (appearanceRoot) {{
  let appearanceStyle = document.querySelector("style[data-aniworld-appearance]");
  if (!appearanceStyle) {{
    appearanceStyle = document.createElement("style");
    appearanceStyle.dataset.aniworldAppearance = "true";
  }}
  appearanceStyle.textContent = {css};
  (document.head || appearanceRoot).appendChild(appearanceStyle);
  appearanceRoot.classList.toggle("aniworld-nyan-scrollbar", {enabled});
}}"#,
        if enabled { "1" } else { "0" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_local_nyan_assets_in_the_stylesheet() {
        let css = stylesheet();
        assert!(css.contains("data:image/svg+xml,"));
        assert!(css.contains("aniworld-nyan-scrollbar"));
        assert!(css.contains("scrollbar-color: auto !important"));
        assert!(css.contains("scrollbar-width: auto !important"));
        assert!(!css.contains("chrome-extension://"));
    }

    #[test]
    fn initializes_the_saved_appearance_before_page_load() {
        let script = initialization_script(true);
        assert!(script.contains("let nyanScrollbarEnabled = true"));
        assert!(script.contains(STORAGE_KEY));
        assert!(script.contains("appearanceObserver.observe(document"));
    }
}
