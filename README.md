# AniWorld Desktop

Schlanker Tauri-2-Desktop-Wrapper für `https://aniworld.to` mit persistenter
WebView-Anmeldung und Discord Rich Presence.

Autor und Herausgeber: **Zorblock**

## Einrichtung

Voraussetzungen unter Windows:

- Rust (stable)
- Node.js mit npm
- Microsoft Edge WebView2 Runtime (unter aktuellem Windows normalerweise vorhanden)
- Discord Desktop

Abhängigkeiten installieren:

```powershell
npm install
```

## Discord Rich Presence

Die Discord Application ID ist bereits fest eingebaut. Es ist keine zusätzliche
Konfiguration nötig. Läuft Discord beim App-Start noch nicht, versucht sich die
App automatisch alle 15 Sekunden erneut zu verbinden.

Discord zeigt ausschließlich eine tatsächlich geöffnete Episodenseite an. Als
Details erscheint der echte Anime-Titel der AniWorld-Seite, darunter zum Beispiel
`Season 2 • Episode 1`. Beim Stöbern, auf Staffelübersichten oder außerhalb von
AniWorld wird die Aktivität gelöscht, statt erfundene Statustexte anzuzeigen.
Das zugehörige Anime-Cover wird bevorzugt über die AniList-API geladen und als
großes Rich-Presence-Bild gesetzt. Falls AniList nicht erreichbar ist, bleibt das
Cover der AniWorld-Seite als Fallback aktiv.
Während das Video läuft, zeigt Discord außerdem einen Fortschrittsbalken aus der
echten Wiedergabeposition und Gesamtdauer. Springen und geänderte
Wiedergabegeschwindigkeit werden berücksichtigt. Bei Pause, Wiedergabeende oder
vor dem ersten Start wird die Discord-Aktivität vollständig ausgeblendet.

## Werbeblocker

Die App blockiert Werbe- und Popup-Anfragen direkt in WebView2 mit der nativen
Rust-Filter-Engine von Brave. EasyList wird beim ersten Start geladen und danach
höchstens alle vier Tage aktualisiert. Ist keine Verbindung verfügbar, verwendet
die App den letzten Cache oder eine eingebaute Grundliste. Der Cache liegt unter:

```text
%APPDATA%\zorblock\userData\AniWorldDesktop\adblock\easylist.txt
```

Zusätzlich werden neue Werbe- und Popunder-Fenster auch bei Mausklicks vollständig
unterdrückt. Bewusst gewählte AniWorld-Hosterlinks öffnen sich stattdessen im
vorhandenen App-Fenster. Passende AniWorld-Werbeelemente werden ausgeblendet.
EasyList stammt von den
[EasyList-Autoren](https://easylist.to/) und steht unter deren Lizenzbedingungen.

## Starten und bauen

```powershell
npm run start
npm run tauri dev
npm run installer
```

`npm run start` startet die vollständige Tauri-App im Entwicklungsmodus zum
Testen. `npm run dev` startet dagegen nur das lokale Vite-Frontend.

`npm run installer` baut automatisch das Frontend, den optimierten Rust-Release
und anschließend das fertige deutsche NSIS-Setup. Die Ausgabedatei liegt unter:

```text
src-tauri\target\release\bundle\nsis\AniWorld Desktop_<VERSION>_x64-setup.exe
```

## GitHub-Release

Der interaktive Release-Ablauf wird so gestartet:

```powershell
npm run release
```

Das Menü bietet Patch, Minor und Major an. Der Ablauf hält die Versionen in
`package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock` und
`tauri.conf.json` synchron, baut einmalig das NSIS-Setup, erstellt Release-Commit
und Git-Tag, pusht beides und lädt Setup sowie SHA-256-Prüfsumme in ein neues
GitHub-Release hoch. Der Git-Arbeitsbaum muss vorher sauber sein.

Eine sichere Vorschau ohne Änderungen, Build oder Upload ist ebenfalls möglich:

```powershell
npm run release -- patch --dry-run
```

Die Anmeldung wird im anwendungseigenen WebView-Profil gespeichert. Die externe
Website erhält bewusst keine Tauri-Capabilities oder Rust-Kommandos. Die RPC-
Anzeige wird nur aus der aktuellen AniWorld-URL und dem Dokumenttitel erzeugt;
Zugangsdaten und Seiteninhalte werden nicht an das Rust-Backend übergeben.

Unter Windows verwendet die App folgende festen Benutzerpfade:

```text
Installation: %APPDATA%\zorblock\apps\AniWorldDesktop
Benutzerdaten: %APPDATA%\zorblock\userData\AniWorldDesktop
```

## Einschränkung

Die App zeigt in Discord die geöffnete Serie sowie Staffel/Folge. Die
Wiedergabeerkennung setzt einen zugänglichen HTML5-Videoplayer im eingebetteten
Hoster voraus. Für Inhalte und Verfügbarkeit der eingebundenen Website ist deren
Betreiber verantwortlich; beachte die in deinem Land geltenden Rechte und
Bedingungen.
