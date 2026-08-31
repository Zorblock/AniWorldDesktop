use crate::settings::DiscordSettings;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Url;

const RECONNECT_INTERVAL: Duration = Duration::from_secs(15);
const PRESENCE_UPDATE_INTERVAL: Duration = Duration::from_secs(15);
const TIMESTAMP_DRIFT_TOLERANCE_MS: u64 = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    anime_slug: Option<String>,
    title: Option<String>,
    season: Option<String>,
    episode: Option<String>,
    cover_url: Option<String>,
    playback: Option<Playback>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Playback {
    playing: bool,
    position_ms: u64,
    duration_ms: u64,
    rate_milli: u32,
    observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PresenceSnapshot {
    name: String,
    details: String,
    state: String,
    cover_url: Option<String>,
    timestamps: Option<(i64, i64)>,
}

impl PresenceSnapshot {
    fn materially_differs(&self, other: &Self) -> bool {
        self.name != other.name
            || self.details != other.details
            || self.state != other.state
            || self.cover_url != other.cover_url
            || timestamps_materially_differ(self.timestamps, other.timestamps)
    }
}

fn timestamps_materially_differ(current: Option<(i64, i64)>, previous: Option<(i64, i64)>) -> bool {
    match (current, previous) {
        (Some(current), Some(previous)) => {
            current.0.abs_diff(previous.0) > TIMESTAMP_DRIFT_TOLERANCE_MS
                || current.1.abs_diff(previous.1) > TIMESTAMP_DRIFT_TOLERANCE_MS
        }
        (None, None) => false,
        _ => true,
    }
}

impl Activity {
    pub fn from_url(url: &Url, document_title: Option<&str>) -> Self {
        let Some(anime_slug) = Self::anime_slug_from_url(url) else {
            return Self::idle();
        };

        let segments: Vec<_> = url
            .path_segments()
            .map(|segments| segments.collect())
            .unwrap_or_default();

        let season = segments
            .iter()
            .find_map(|segment| segment.strip_prefix("staffel-"))
            .map(str::to_owned);
        let episode = segments
            .iter()
            .find_map(|segment| segment.strip_prefix("episode-"))
            .map(str::to_owned);
        let title = clean_document_title(document_title).filter(|title| !title.is_empty());

        Self {
            anime_slug: Some(anime_slug),
            title,
            season,
            episode,
            cover_url: None,
            playback: None,
        }
    }

    pub(crate) fn idle() -> Self {
        Self {
            anime_slug: None,
            title: None,
            season: None,
            episode: None,
            cover_url: None,
            playback: None,
        }
    }

    pub(crate) fn anime_slug_from_url(url: &Url) -> Option<String> {
        if url.host_str() != Some("aniworld.to") {
            return None;
        }

        let segments: Vec<_> = url.path_segments()?.collect();
        let stream_index = segments
            .windows(2)
            .position(|pair| pair == ["anime", "stream"])?;
        let slug = segments.get(stream_index + 2)?.trim();

        (!slug.is_empty()).then(|| slug.to_owned())
    }

    pub(crate) fn anime_slug(&self) -> Option<&str> {
        self.anime_slug.as_deref()
    }

    pub(crate) fn anime_title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub(crate) fn set_cover_url(&mut self, cover_url: String) {
        if self.episode_fields().is_some() {
            self.cover_url = Some(cover_url);
        }
    }

    pub(crate) fn set_playback(
        &mut self,
        playing: bool,
        seeking: bool,
        position_ms: u64,
        duration_ms: u64,
        rate_milli: u32,
    ) -> bool {
        let playing = playing
            || (seeking
                && self
                    .playback
                    .as_ref()
                    .is_some_and(|playback| playback.playing));
        self.set_playback_at(
            playing,
            position_ms,
            duration_ms,
            rate_milli,
            unix_timestamp_millis(),
        )
    }

    fn set_playback_at(
        &mut self,
        playing: bool,
        position_ms: u64,
        duration_ms: u64,
        rate_milli: u32,
        observed_at_ms: i64,
    ) -> bool {
        if self.episode_fields().is_none()
            || !(60_000..=12 * 60 * 60 * 1_000).contains(&duration_ms)
            || position_ms > duration_ms
            || !(250..=4_000).contains(&rate_milli)
        {
            return false;
        }

        self.playback = Some(Playback {
            playing,
            position_ms,
            duration_ms,
            rate_milli,
            observed_at_ms,
        });
        true
    }

    fn playback_timestamps(&self) -> Option<(i64, i64)> {
        let playback = self.playback.as_ref()?;
        if !playback.playing {
            return None;
        }

        let rate = i64::from(playback.rate_milli);
        let position_at_rate = i64::try_from(playback.position_ms)
            .ok()?
            .saturating_mul(1_000)
            / rate;
        let duration_at_rate = i64::try_from(playback.duration_ms)
            .ok()?
            .saturating_mul(1_000)
            / rate;
        let start = playback.observed_at_ms.saturating_sub(position_at_rate);
        Some((start, start.saturating_add(duration_at_rate)))
    }

    fn discord_fields(&self) -> Option<(String, String)> {
        self.episode_fields()
    }

    fn presence_snapshot(&self, settings: &DiscordSettings) -> Option<PresenceSnapshot> {
        if self.discord_fields().is_some() {
            let name = self.render_template(&settings.status_template, settings);
            let details = self.render_template(&settings.details_template, settings);
            let mut state = self.render_template(&settings.state_template, settings);
            if settings.show_playback_state
                && !settings.state_template.contains("{playback}")
                && self.playback_label().is_some()
            {
                state = append_presence_part(&state, self.playback_label().unwrap_or_default());
            }

            return Some(PresenceSnapshot {
                name: presence_text_or(name, "AniWorld"),
                details: presence_text_or(details, "Watching anime"),
                state: presence_text_or(state, "Watching on AniWorld"),
                cover_url: settings
                    .show_cover
                    .then(|| self.cover_url.clone())
                    .flatten(),
                timestamps: settings
                    .show_progress
                    .then(|| self.playback_timestamps())
                    .flatten(),
            });
        }

        settings.show_browsing_activity.then(|| PresenceSnapshot {
            name: "AniWorld".to_owned(),
            details: presence_text_or(settings.browsing_details.clone(), "Browsing AniWorld"),
            state: presence_text_or(
                settings.browsing_state.clone(),
                "Looking for something to watch",
            ),
            cover_url: None,
            timestamps: None,
        })
    }

    fn render_template(&self, template: &str, settings: &DiscordSettings) -> String {
        let mut template = template.to_owned();
        if !settings.show_season {
            template = template
                .replace("Season {season}", "")
                .replace("S{season}", "");
        }
        if !settings.show_episode {
            template = template
                .replace("Episode {episode}", "")
                .replace("E{episode}", "");
        }

        let anime = if settings.show_anime_title {
            self.title.as_deref().unwrap_or("AniWorld")
        } else {
            "AniWorld"
        };
        let season = if settings.show_season {
            self.season.as_deref().unwrap_or("")
        } else {
            ""
        };
        let episode = if settings.show_episode {
            self.episode.as_deref().unwrap_or("")
        } else {
            ""
        };
        let playback = settings
            .show_playback_state
            .then(|| self.playback_label())
            .flatten()
            .unwrap_or("");

        normalize_presence_text(
            &template
                .replace("{anime}", anime)
                .replace("{season}", season)
                .replace("{episode}", episode)
                .replace("{playback}", playback),
        )
    }

    fn playback_label(&self) -> Option<&'static str> {
        self.playback.as_ref().map(|playback| {
            if playback.playing {
                "Playing"
            } else {
                "Paused"
            }
        })
    }

    fn episode_fields(&self) -> Option<(String, String)> {
        let title = self.title.clone()?;
        let season = self.season.as_deref()?;
        let episode = self.episode.as_deref()?;

        Some((title, format!("Season {season} • Episode {episode}")))
    }
}

fn append_presence_part(value: &str, part: &str) -> String {
    if value.is_empty() {
        part.to_owned()
    } else if part.is_empty() {
        value.to_owned()
    } else {
        format!("{value} • {part}")
    }
}

fn normalize_presence_text(value: &str) -> String {
    let mut value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    for (duplicate, separator) in [("• •", "•"), ("| |", "|"), ("— —", "—"), ("- -", "-")]
    {
        while value.contains(duplicate) {
            value = value.replace(duplicate, separator);
        }
    }
    value
        .trim_matches(|character: char| {
            character.is_whitespace() || matches!(character, '•' | '|' | '—' | '-' | ':' | ',')
        })
        .chars()
        .take(128)
        .collect()
}

fn presence_text_or(value: String, fallback: &str) -> String {
    let value = normalize_presence_text(&value);
    if value.chars().count() >= 2 {
        value
    } else {
        fallback.to_owned()
    }
}

fn clean_document_title(title: Option<&str>) -> Option<String> {
    let title = title?.trim();
    if title.is_empty()
        || title.eq_ignore_ascii_case("aniworld.to")
        || title.to_ascii_lowercase().contains("just a moment")
    {
        return None;
    }

    let title = title
        .split(" | AniWorld")
        .next()
        .unwrap_or(title)
        .split(" - Staffel ")
        .next()
        .unwrap_or(title)
        .trim();
    let title = title
        .strip_prefix("Episode ")
        .and_then(|episode_title| episode_title.split_once(" von "))
        .map(|(_, anime_title)| anime_title)
        .unwrap_or(title)
        .trim();

    (!title.is_empty()).then(|| title.to_owned())
}

pub struct DiscordPresence {
    sender: Sender<PresenceCommand>,
}

impl DiscordPresence {
    pub fn start(client_id: &str, settings: DiscordSettings) -> Self {
        let client_id = client_id.to_owned();
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("discord-rpc".to_owned())
            .spawn(move || rpc_worker(&client_id, receiver, settings))
            .expect("Discord RPC thread could not be started");
        let _ = sender.send(PresenceCommand::Activity(Activity::idle()));

        Self { sender }
    }

    pub fn update(&self, activity: Activity) {
        let _ = self.sender.send(PresenceCommand::Activity(activity));
    }

    pub fn update_settings(&self, settings: DiscordSettings) {
        let _ = self.sender.send(PresenceCommand::Settings(settings));
    }
}

enum PresenceCommand {
    Activity(Activity),
    Settings(DiscordSettings),
}

fn rpc_worker(client_id: &str, receiver: Receiver<PresenceCommand>, mut settings: DiscordSettings) {
    let mut client = DiscordIpcClient::new(client_id);
    let mut connected = false;
    let mut current = Activity::idle();
    let mut published: Option<PresenceSnapshot> = None;
    let mut activity_cleared = false;
    let mut last_write: Option<Instant> = None;

    loop {
        let desired = settings
            .enabled
            .then(|| current.presence_snapshot(&settings))
            .flatten();
        let pending_update = desired.as_ref().is_some_and(|desired| {
            activity_cleared
                || published
                    .as_ref()
                    .is_none_or(|previous| desired.materially_differs(previous))
        });
        let wait_time = if connected && pending_update {
            last_write
                .map(|last| PRESENCE_UPDATE_INTERVAL.saturating_sub(last.elapsed()))
                .unwrap_or(Duration::ZERO)
        } else {
            RECONNECT_INTERVAL
        };

        match receiver.recv_timeout(wait_time) {
            Ok(command) => {
                let mut settings_changed = apply_command(command, &mut current, &mut settings);
                for command in receiver.try_iter() {
                    settings_changed |= apply_command(command, &mut current, &mut settings);
                }
                if settings_changed {
                    published = None;
                    last_write = None;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if connected {
                    let _ = client.clear_activity();
                    let _ = client.close();
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        if !settings.enabled {
            if connected {
                let _ = client.clear_activity();
                let _ = client.close();
            }
            connected = false;
            published = None;
            activity_cleared = true;
            last_write = None;
            continue;
        }

        let Some(desired) = current.presence_snapshot(&settings) else {
            if connected && !activity_cleared {
                if client.clear_activity().is_err() {
                    connected = false;
                    let _ = client.close();
                } else {
                    published = None;
                    activity_cleared = true;
                }
            }
            continue;
        };

        if !connected {
            connected = client.connect().is_ok();
            if !connected {
                continue;
            }
            published = None;
            activity_cleared = false;
            last_write = None;
        }

        if published
            .as_ref()
            .is_some_and(|previous| !desired.materially_differs(previous))
        {
            continue;
        }

        if last_write.is_some_and(|last| last.elapsed() < PRESENCE_UPDATE_INTERVAL) {
            continue;
        }

        let mut payload = activity::Activity::new()
            .name(&desired.name)
            .details(&desired.details)
            .state(&desired.state)
            .activity_type(activity::ActivityType::Watching)
            .status_display_type(activity::StatusDisplayType::Name);
        if let Some((start, end)) = desired.timestamps {
            payload = payload.timestamps(activity::Timestamps::new().start(start).end(end));
        }
        if let Some(cover_url) = desired.cover_url.as_deref() {
            let assets = activity::Assets::new()
                .large_image(cover_url)
                .large_text(&desired.details);
            payload = payload.assets(assets);
        }

        if client.set_activity(payload).is_err() {
            connected = false;
            published = None;
            let _ = client.close();
        } else {
            published = Some(desired);
            activity_cleared = false;
            last_write = Some(Instant::now());
        }
    }
}

fn apply_command(
    command: PresenceCommand,
    current: &mut Activity,
    settings: &mut DiscordSettings,
) -> bool {
    match command {
        PresenceCommand::Activity(activity) => {
            *current = activity;
            false
        }
        PresenceCommand::Settings(updated) => {
            *settings = updated;
            true
        }
    }
}

fn unix_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_episode_url() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();

        let parsed = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to - Animes gratis legal online ansehen"),
        );

        assert_eq!(
            parsed.episode_fields(),
            Some(("Demon Slayer".to_owned(), "Season 2 • Episode 7".to_owned()))
        );
    }

    #[test]
    fn parses_saved_aniworld_episode_title() {
        let url = Url::parse(
            "https://aniworld.to/anime/stream/skeleton-knight-in-another-world/staffel-2/episode-1",
        )
        .unwrap();
        let parsed = Activity::from_url(
            &url,
            Some(
                "Episode 1 Staffel 2 von Skeleton Knight in Another World | AniWorld.to - Animes gratis legal online ansehen",
            ),
        );

        assert_eq!(
            parsed.episode_fields(),
            Some((
                "Skeleton Knight in Another World".to_owned(),
                "Season 2 • Episode 1".to_owned()
            ))
        );
    }

    #[test]
    fn ignores_titles_from_other_hosts() {
        let url = Url::parse("https://example.com/anime/stream/fake/staffel-1/episode-1").unwrap();
        let parsed = Activity::from_url(&url, Some("Fake"));

        assert_eq!(parsed.discord_fields(), None);
    }

    #[test]
    fn does_not_publish_browsing_as_watching() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2").unwrap();
        let parsed = Activity::from_url(
            &url,
            Some("Staffel 2 von Demon Slayer | AniWorld.to - Animes gratis legal online ansehen"),
        );

        assert_eq!(parsed.discord_fields(), None);
    }

    #[test]
    fn only_adds_cover_to_a_real_episode_activity() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let mut parsed = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );
        parsed.set_cover_url("https://aniworld.to/cover.jpg".to_owned());

        assert_eq!(
            parsed.cover_url.as_deref(),
            Some("https://aniworld.to/cover.jpg")
        );
        assert_eq!(parsed.anime_slug(), Some("demon-slayer"));
    }

    #[test]
    fn calculates_discord_progress_timestamps() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let mut parsed = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );

        assert!(parsed.set_playback_at(true, 30_000, 120_000, 1_000, 1_000_000));
        assert!(parsed.discord_fields().is_some());
        assert_eq!(parsed.playback_timestamps(), Some((970_000, 1_090_000)));

        assert!(parsed.set_playback_at(false, 30_000, 120_000, 1_000, 1_000_000));
        assert!(parsed.discord_fields().is_some());
        assert_eq!(parsed.playback_timestamps(), None);
    }

    #[test]
    fn ignores_normal_playback_timestamp_drift() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let mut first = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );
        let mut later = first.clone();

        assert!(first.set_playback_at(true, 30_000, 120_000, 1_000, 1_000_000));
        assert!(later.set_playback_at(true, 40_000, 120_000, 1_000, 1_010_000));

        let settings = DiscordSettings::default();
        let first = first.presence_snapshot(&settings).unwrap();
        let later = later.presence_snapshot(&settings).unwrap();
        assert!(!later.materially_differs(&first));
    }

    #[test]
    fn detects_a_playback_seek() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let mut first = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );
        let mut seeked = first.clone();

        assert!(first.set_playback_at(true, 30_000, 120_000, 1_000, 1_000_000));
        assert!(seeked.set_playback_at(true, 70_000, 120_000, 1_000, 1_010_000));

        let settings = DiscordSettings::default();
        let first = first.presence_snapshot(&settings).unwrap();
        let seeked = seeked.presence_snapshot(&settings).unwrap();
        assert!(seeked.materially_differs(&first));
    }

    #[test]
    fn keeps_episode_presence_without_progress_while_paused() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let mut parsed = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );

        assert!(parsed.set_playback_at(false, 30_000, 120_000, 1_000, 1_000_000));
        let presence = parsed
            .presence_snapshot(&DiscordSettings::default())
            .unwrap();

        assert_eq!(presence.details, "Demon Slayer");
        assert_eq!(presence.state, "Season 2 • Episode 7");
        assert_eq!(presence.timestamps, None);
    }

    #[test]
    fn keeps_playback_active_during_a_seek() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let mut parsed = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );

        assert!(parsed.set_playback_at(true, 30_000, 120_000, 1_000, 1_000_000));
        assert!(parsed.set_playback(false, true, 70_000, 120_000, 1_000));

        assert!(parsed
            .playback
            .as_ref()
            .is_some_and(|playback| playback.playing));
        assert!(parsed.playback_timestamps().is_some());
    }

    #[test]
    fn can_put_the_anime_title_in_the_discord_status() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let parsed = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );
        let settings = DiscordSettings {
            status_template: "{anime}".to_owned(),
            ..DiscordSettings::default()
        };

        let presence = parsed.presence_snapshot(&settings).unwrap();

        assert_eq!(presence.name, "Demon Slayer");
        assert_eq!(presence.details, "Demon Slayer");
    }

    #[test]
    fn privacy_controls_remove_episode_metadata_and_assets() {
        let url = Url::parse("https://aniworld.to/anime/stream/demon-slayer/staffel-2/episode-7")
            .unwrap();
        let mut parsed = Activity::from_url(
            &url,
            Some("Episode 7 Staffel 2 von Demon Slayer | AniWorld.to"),
        );
        parsed.set_cover_url(
            "https://s4.anilist.co/file/anilistcdn/media/anime/cover.jpg".to_owned(),
        );
        assert!(parsed.set_playback_at(true, 30_000, 120_000, 1_000, 1_000_000));
        let settings = DiscordSettings {
            show_anime_title: false,
            show_season: false,
            show_episode: false,
            show_cover: false,
            show_progress: false,
            status_template: "{anime}".to_owned(),
            details_template: "Watching anime".to_owned(),
            state_template: "Season {season} • Episode {episode}".to_owned(),
            ..DiscordSettings::default()
        };

        let presence = parsed.presence_snapshot(&settings).unwrap();

        assert_eq!(presence.name, "AniWorld");
        assert!(!presence.details.contains("Demon Slayer"));
        assert!(!presence.state.contains('2'));
        assert!(!presence.state.contains('7'));
        assert_eq!(presence.cover_url, None);
        assert_eq!(presence.timestamps, None);
    }

    #[test]
    fn browsing_presence_can_be_disabled() {
        let settings = DiscordSettings {
            show_browsing_activity: false,
            ..DiscordSettings::default()
        };

        assert_eq!(Activity::idle().presence_snapshot(&settings), None);
    }
}
