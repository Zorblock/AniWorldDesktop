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
    details: String,
    state: String,
    cover_url: Option<String>,
    timestamps: (i64, i64),
}

impl PresenceSnapshot {
    fn materially_differs(&self, other: &Self) -> bool {
        self.details != other.details
            || self.state != other.state
            || self.cover_url != other.cover_url
            || self.timestamps.0.abs_diff(other.timestamps.0) > TIMESTAMP_DRIFT_TOLERANCE_MS
            || self.timestamps.1.abs_diff(other.timestamps.1) > TIMESTAMP_DRIFT_TOLERANCE_MS
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
        position_ms: u64,
        duration_ms: u64,
        rate_milli: u32,
    ) -> bool {
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
        if !self.playback.as_ref()?.playing {
            return None;
        }
        self.episode_fields()
    }

    fn presence_snapshot(&self) -> Option<PresenceSnapshot> {
        let (details, state) = self.discord_fields()?;
        Some(PresenceSnapshot {
            details,
            state,
            cover_url: self.cover_url.clone(),
            timestamps: self.playback_timestamps()?,
        })
    }

    fn episode_fields(&self) -> Option<(String, String)> {
        let title = self.title.clone()?;
        let season = self.season.as_deref()?;
        let episode = self.episode.as_deref()?;

        Some((title, format!("Season {season} • Episode {episode}")))
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
    sender: Sender<Activity>,
}

impl DiscordPresence {
    pub fn start(client_id: &str) -> Self {
        let client_id = client_id.to_owned();
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("discord-rpc".to_owned())
            .spawn(move || rpc_worker(&client_id, receiver))
            .expect("Discord RPC thread could not be started");

        Self { sender }
    }

    pub fn update(&self, activity: Activity) {
        let _ = self.sender.send(activity);
    }
}

fn rpc_worker(client_id: &str, receiver: Receiver<Activity>) {
    let mut client = DiscordIpcClient::new(client_id);
    let mut connected = false;
    let mut current = Activity::idle();
    let mut published: Option<PresenceSnapshot> = None;
    let mut activity_visible = false;
    let mut last_write: Option<Instant> = None;

    loop {
        let pending_update = current.presence_snapshot().is_some_and(|desired| {
            published
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
            Ok(activity) => {
                current = activity;
                for queued in receiver.try_iter() {
                    current = queued;
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

        let Some(desired) = current.presence_snapshot() else {
            if connected && activity_visible {
                if client.clear_activity().is_err() {
                    connected = false;
                    let _ = client.close();
                } else {
                    activity_visible = false;
                    published = None;
                    last_write = Some(Instant::now());
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
            activity_visible = false;
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
            .details(&desired.details)
            .state(&desired.state)
            .activity_type(activity::ActivityType::Watching);
        payload = payload.timestamps(
            activity::Timestamps::new()
                .start(desired.timestamps.0)
                .end(desired.timestamps.1),
        );
        if let Some(cover_url) = desired.cover_url.as_deref() {
            let assets = activity::Assets::new()
                .large_image(cover_url)
                .large_text(&desired.details);
            payload = payload.assets(assets);
        }

        if client.set_activity(payload).is_err() {
            connected = false;
            published = None;
            activity_visible = false;
            let _ = client.close();
        } else {
            published = Some(desired);
            activity_visible = true;
            last_write = Some(Instant::now());
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
        assert_eq!(parsed.discord_fields(), None);
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

        let first = first.presence_snapshot().unwrap();
        let later = later.presence_snapshot().unwrap();
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

        let first = first.presence_snapshot().unwrap();
        let seeked = seeked.presence_snapshot().unwrap();
        assert!(seeked.materially_differs(&first));
    }
}
