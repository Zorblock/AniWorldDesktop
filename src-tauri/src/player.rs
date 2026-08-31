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

const FULLSCREEN_LAYOUT_CSS: &str = r#"
html.aniworld-desktop-fullscreen,
html.aniworld-desktop-fullscreen > body,
html.aniworld-desktop-framed.aniworld-desktop-fullscreen > body {
  width: 100% !important;
  height: 100% !important;
  min-width: 0 !important;
  min-height: 0 !important;
  margin: 0 !important;
  padding: 0 !important;
  overflow: hidden !important;
  background: #000 !important;
}
html.aniworld-desktop-fullscreen [data-aniworld-titlebar] {
  display: none !important;
}
html.aniworld-desktop-fullscreen :fullscreen,
html.aniworld-desktop-fullscreen :-webkit-full-screen,
html.aniworld-desktop-fullscreen .jwplayer.jw-flag-fullscreen,
html.aniworld-desktop-fullscreen .video-js.vjs-fullscreen,
html.aniworld-desktop-fullscreen .plyr--fullscreen-active,
html.aniworld-desktop-fullscreen .dplayer-fulled {
  position: fixed !important;
  inset: 0 !important;
  box-sizing: border-box !important;
  width: 100vw !important;
  height: 100vh !important;
  min-width: 0 !important;
  min-height: 0 !important;
  max-width: none !important;
  max-height: none !important;
  margin: 0 !important;
  padding: 0 !important;
  border: 0 !important;
  border-radius: 0 !important;
  overflow: hidden !important;
  background: #000 !important;
}
html.aniworld-desktop-fullscreen :fullscreen > iframe,
html.aniworld-desktop-fullscreen :fullscreen > video,
html.aniworld-desktop-fullscreen :fullscreen video,
html.aniworld-desktop-fullscreen :-webkit-full-screen > iframe,
html.aniworld-desktop-fullscreen :-webkit-full-screen > video,
html.aniworld-desktop-fullscreen :-webkit-full-screen video,
html.aniworld-desktop-fullscreen .jwplayer.jw-flag-fullscreen .jw-media,
html.aniworld-desktop-fullscreen .jwplayer.jw-flag-fullscreen .jw-video,
html.aniworld-desktop-fullscreen .video-js.vjs-fullscreen .vjs-tech,
html.aniworld-desktop-fullscreen .plyr--fullscreen-active .plyr__video-wrapper,
html.aniworld-desktop-fullscreen .plyr--fullscreen-active video,
html.aniworld-desktop-fullscreen .dplayer-fulled .dplayer-video-wrap,
html.aniworld-desktop-fullscreen .dplayer-fulled video {
  box-sizing: border-box !important;
  width: 100% !important;
  height: 100% !important;
  min-width: 0 !important;
  min-height: 0 !important;
  max-width: none !important;
  max-height: none !important;
  margin: 0 !important;
  padding: 0 !important;
  border: 0 !important;
  border-radius: 0 !important;
}
html.aniworld-desktop-fullscreen video {
  object-fit: contain !important;
}
"#;

pub fn stylesheet() -> String {
    format!("{EMBED_PLAYER_CSS}\n{FULLSCREEN_LAYOUT_CSS}")
}
