use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

const SETTINGS_FILE_NAME: &str = "settings.json";
const MAX_TEMPLATE_LENGTH: usize = 120;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub start_maximized: bool,
    pub check_updates_on_start: bool,
    pub discord: DiscordSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            start_maximized: true,
            check_updates_on_start: true,
            discord: DiscordSettings::default(),
        }
    }
}

impl AppSettings {
    fn normalized(mut self) -> Self {
        self.discord.normalize();
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DiscordSettings {
    pub enabled: bool,
    pub show_browsing_activity: bool,
    pub show_anime_title: bool,
    pub show_season: bool,
    pub show_episode: bool,
    pub show_cover: bool,
    pub show_progress: bool,
    pub show_playback_state: bool,
    pub status_template: String,
    pub details_template: String,
    pub state_template: String,
    pub browsing_details: String,
    pub browsing_state: String,
}

impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_browsing_activity: true,
            show_anime_title: true,
            show_season: true,
            show_episode: true,
            show_cover: true,
            show_progress: true,
            show_playback_state: false,
            status_template: "AniWorld".to_owned(),
            details_template: "{anime}".to_owned(),
            state_template: "Season {season} • Episode {episode}".to_owned(),
            browsing_details: "Browsing AniWorld".to_owned(),
            browsing_state: "Looking for something to watch".to_owned(),
        }
    }
}

impl DiscordSettings {
    fn normalize(&mut self) {
        let defaults = Self::default();
        self.status_template = normalize_template(&self.status_template, &defaults.status_template);
        self.details_template =
            normalize_template(&self.details_template, &defaults.details_template);
        self.state_template = normalize_template(&self.state_template, &defaults.state_template);
        self.browsing_details =
            normalize_template(&self.browsing_details, &defaults.browsing_details);
        self.browsing_state = normalize_template(&self.browsing_state, &defaults.browsing_state);
    }
}

fn normalize_template(value: &str, fallback: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let value = if normalized.is_empty() {
        fallback
    } else {
        &normalized
    };
    value.chars().take(MAX_TEMPLATE_LENGTH).collect()
}

pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<AppSettings>,
}

impl SettingsStore {
    pub fn load(data_directory: &Path) -> Self {
        let path = data_directory.join(SETTINGS_FILE_NAME);
        let settings = match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<AppSettings>(&contents) {
                Ok(settings) => settings.normalized(),
                Err(error) => {
                    eprintln!("Could not parse {}: {error}", path.display());
                    AppSettings::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppSettings::default(),
            Err(error) => {
                eprintln!("Could not read {}: {error}", path.display());
                AppSettings::default()
            }
        };

        Self {
            path,
            current: RwLock::new(settings),
        }
    }

    pub fn get(&self) -> AppSettings {
        self.current
            .read()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    pub fn save(&self, settings: AppSettings) -> Result<AppSettings, String> {
        let settings = settings.normalized();
        let contents = serde_json::to_string_pretty(&settings)
            .map_err(|error| format!("Could not serialize settings: {error}"))?;
        fs::write(&self.path, format!("{contents}\n"))
            .map_err(|error| format!("Could not write {}: {error}", self.path.display()))?;
        let mut current = self
            .current
            .write()
            .map_err(|_| "Could not lock the settings store".to_owned())?;
        *current = settings.clone();
        Ok(settings)
    }

    pub fn reset(&self) -> Result<AppSettings, String> {
        self.save(AppSettings::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_json_fields_keep_their_defaults() {
        let parsed: AppSettings = serde_json::from_str(r#"{"discord":{"enabled":false}}"#).unwrap();

        assert!(!parsed.discord.enabled);
        assert!(parsed.discord.show_cover);
        assert!(parsed.start_maximized);
        assert!(parsed.check_updates_on_start);
    }

    #[test]
    fn templates_are_single_line_and_limited() {
        let mut settings = AppSettings::default();
        settings.discord.details_template = format!("  Watching\n{}  ", "a".repeat(200));

        let settings = settings.normalized();

        assert!(!settings.discord.details_template.contains('\n'));
        assert_eq!(settings.discord.details_template.chars().count(), 120);
    }

    #[test]
    fn blank_templates_restore_safe_defaults() {
        let mut settings = AppSettings::default();
        settings.discord.status_template = "   ".to_owned();
        settings.discord.state_template = "\n".to_owned();

        let settings = settings.normalized();

        assert_eq!(settings.discord.status_template, "AniWorld");
        assert_eq!(
            settings.discord.state_template,
            "Season {season} • Episode {episode}"
        );
    }

    #[test]
    fn saved_settings_are_loaded_from_the_user_data_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "aniworld-desktop-settings-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let store = SettingsStore::load(&directory);
        let mut settings = AppSettings::default();
        settings.discord.enabled = false;
        settings.discord.status_template = "{anime}".to_owned();

        store.save(settings.clone()).unwrap();
        let loaded = SettingsStore::load(&directory).get();

        assert_eq!(loaded, settings);
        fs::remove_file(directory.join(SETTINGS_FILE_NAME)).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
