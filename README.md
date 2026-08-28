# AniWorld Desktop

A lightweight Windows desktop app for AniWorld, built with Rust and Tauri.

## Features

- Persistent AniWorld login in a dedicated app profile
- Built-in ad and popup blocking
- Discord Rich Presence with anime title, season, episode, cover, and playback progress
- Rich Presence automatically hides while playback is paused
- Signed automatic updates with visible download and installation progress
- Per-user installation without administrator rights

## Install

Download the latest setup from
[GitHub Releases](https://github.com/Zorblock/AniWorldDesktop/releases/latest)
and run it. The app checks for new versions whenever it starts.

```text
Installation: %APPDATA%\zorblock\apps\AniWorldDesktop
User data:    %APPDATA%\zorblock\userData\AniWorldDesktop
```

## Run from Source

Requires Node.js, Rust, WebView2, and Discord Desktop.

```powershell
npm install
npm run start
```

Create a signed NSIS setup with:

```powershell
npm run installer
```

## Disclaimer

AniWorld Desktop is an independent desktop wrapper. Content availability and
rights remain the responsibility of the embedded website and the user.

Author and publisher: **Zorblock**
