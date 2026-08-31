import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface DiscordSettings {
  enabled: boolean;
  showBrowsingActivity: boolean;
  showAnimeTitle: boolean;
  showSeason: boolean;
  showEpisode: boolean;
  showCover: boolean;
  showProgress: boolean;
  showPlaybackState: boolean;
  statusTemplate: string;
  detailsTemplate: string;
  stateTemplate: string;
  browsingDetails: string;
  browsingState: string;
}

interface AppSettings {
  startMaximized: boolean;
  checkUpdatesOnStart: boolean;
  discord: DiscordSettings;
}

interface SettingsBootstrap {
  settings: AppSettings;
  appVersion: string;
}

interface UpdateCheckResult {
  currentVersion: string;
  availableVersion: string | null;
}

type PresetName = "standard" | "anime-status" | "compact" | "private" | "custom";

interface PresencePreset {
  statusTemplate: string;
  detailsTemplate: string;
  stateTemplate: string;
  showAnimeTitle: boolean;
  showSeason: boolean;
  showEpisode: boolean;
  showCover: boolean;
  showProgress: boolean;
  showPlaybackState: boolean;
}

const presets: Record<Exclude<PresetName, "custom">, PresencePreset> = {
  standard: {
    statusTemplate: "AniWorld",
    detailsTemplate: "{anime}",
    stateTemplate: "Season {season} • Episode {episode}",
    showAnimeTitle: true,
    showSeason: true,
    showEpisode: true,
    showCover: true,
    showProgress: true,
    showPlaybackState: false,
  },
  "anime-status": {
    statusTemplate: "{anime}",
    detailsTemplate: "Watching on AniWorld",
    stateTemplate: "Season {season} • Episode {episode}",
    showAnimeTitle: true,
    showSeason: true,
    showEpisode: true,
    showCover: true,
    showProgress: true,
    showPlaybackState: false,
  },
  compact: {
    statusTemplate: "{anime}",
    detailsTemplate: "S{season} E{episode}",
    stateTemplate: "AniWorld",
    showAnimeTitle: true,
    showSeason: true,
    showEpisode: true,
    showCover: true,
    showProgress: true,
    showPlaybackState: false,
  },
  private: {
    statusTemplate: "AniWorld",
    detailsTemplate: "Watching anime",
    stateTemplate: "Private activity",
    showAnimeTitle: false,
    showSeason: false,
    showEpisode: false,
    showCover: false,
    showProgress: false,
    showPlaybackState: false,
  },
};

const requiredElement = <T extends HTMLElement>(id: string): T => {
  const element = document.getElementById(id);
  if (!(element instanceof HTMLElement)) {
    throw new Error(`Missing settings element: ${id}`);
  }
  return element as T;
};

const checkbox = (id: string) => requiredElement<HTMLInputElement>(id);
const textInput = (id: string) => requiredElement<HTMLInputElement>(id);

const settingsWindow = getCurrentWindow();
const settingsForm = requiredElement<HTMLFormElement>("settings-form");
const discordOptions = requiredElement<HTMLFieldSetElement>("discord-options");
const presencePreset = requiredElement<HTMLSelectElement>("presence-preset");
const saveButton = requiredElement<HTMLButtonElement>("save-settings");
const resetButton = requiredElement<HTMLButtonElement>("reset-settings");
const checkUpdatesButton = requiredElement<HTMLButtonElement>("check-updates");
const saveStatus = requiredElement<HTMLSpanElement>("save-status");
const updateStatus = requiredElement<HTMLSpanElement>("update-status");
const currentVersion = requiredElement<HTMLParagraphElement>("current-version");
const browsingDetails = textInput("browsing-details");
const browsingState = textInput("browsing-state");
const statusTemplate = textInput("status-template");
const detailsTemplate = textInput("details-template");
const stateTemplate = textInput("state-template");

let settingsLoaded = false;
let applyingPreset = false;

const errorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const setStatus = (
  element: HTMLElement,
  message: string,
  kind?: "error" | "success",
) => {
  element.textContent = message;
  if (kind) {
    element.dataset.kind = kind;
  } else {
    delete element.dataset.kind;
  }
};

const runWindowAction = (action: () => Promise<void>) => {
  void action().catch((error: unknown) => {
    console.error("Could not execute the settings window action", error);
  });
};

const setDirty = () => {
  if (!settingsLoaded) {
    return;
  }
  saveButton.disabled = false;
  setStatus(saveStatus, "Unsaved changes");
};

const setDiscordAvailability = () => {
  const enabled = checkbox("discord-enabled").checked;
  discordOptions.disabled = !enabled;
  const browsingEnabled = enabled && checkbox("show-browsing-activity").checked;
  browsingDetails.disabled = !browsingEnabled;
  browsingState.disabled = !browsingEnabled;
};

const cleanPreviewText = (value: string): string =>
  value
    .replace(/\s+/g, " ")
    .replace(/(?:•\s*){2,}/g, "• ")
    .replace(/^[\s•|—,:-]+|[\s•|—,:-]+$/g, "")
    .trim();

const replaceEvery = (value: string, search: string, replacement: string): string =>
  value.split(search).join(replacement);

const renderTemplate = (template: string): string => {
  let value = template;
  if (!checkbox("show-season").checked) {
    value = replaceEvery(replaceEvery(value, "Season {season}", ""), "S{season}", "");
  }
  if (!checkbox("show-episode").checked) {
    value = replaceEvery(replaceEvery(value, "Episode {episode}", ""), "E{episode}", "");
  }

  return cleanPreviewText(
    replaceEvery(
      replaceEvery(
        replaceEvery(
          replaceEvery(
            value,
            "{anime}",
            checkbox("show-anime-title").checked
              ? "Frieren: Beyond Journey's End"
              : "AniWorld",
          ),
          "{season}",
          checkbox("show-season").checked ? "1" : "",
        ),
        "{episode}",
        checkbox("show-episode").checked ? "12" : "",
      ),
      "{playback}",
      checkbox("show-playback-state").checked ? "Playing" : "",
    ),
  );
};

const updatePreview = () => {
  const previewStatus = requiredElement<HTMLElement>("preview-status");
  const previewDetails = requiredElement<HTMLElement>("preview-details");
  const previewState = requiredElement<HTMLElement>("preview-state");
  const previewMeta = requiredElement<HTMLElement>("preview-meta");

  if (!checkbox("discord-enabled").checked) {
    previewStatus.textContent = "Discord Rich Presence is off";
    previewDetails.textContent = "";
    previewState.textContent = "";
    previewMeta.textContent = "";
    return;
  }

  previewStatus.textContent = `Watching ${renderTemplate(statusTemplate.value) || "AniWorld"}`;
  previewDetails.textContent = renderTemplate(detailsTemplate.value) || "Watching anime";
  let state = renderTemplate(stateTemplate.value) || "Watching on AniWorld";
  if (
    checkbox("show-playback-state").checked &&
    !stateTemplate.value.includes("{playback}")
  ) {
    state = `${state} • Playing`;
  }
  previewState.textContent = state;

  const meta: string[] = [];
  if (checkbox("show-cover").checked) {
    meta.push("Anime cover");
  }
  if (checkbox("show-progress").checked) {
    meta.push("Playback progress");
  }
  previewMeta.textContent = meta.join(" • ");
};

const readDiscordSettings = (): DiscordSettings => ({
  enabled: checkbox("discord-enabled").checked,
  showBrowsingActivity: checkbox("show-browsing-activity").checked,
  showAnimeTitle: checkbox("show-anime-title").checked,
  showSeason: checkbox("show-season").checked,
  showEpisode: checkbox("show-episode").checked,
  showCover: checkbox("show-cover").checked,
  showProgress: checkbox("show-progress").checked,
  showPlaybackState: checkbox("show-playback-state").checked,
  statusTemplate: statusTemplate.value,
  detailsTemplate: detailsTemplate.value,
  stateTemplate: stateTemplate.value,
  browsingDetails: browsingDetails.value,
  browsingState: browsingState.value,
});

const readSettings = (): AppSettings => ({
  startMaximized: checkbox("start-maximized").checked,
  checkUpdatesOnStart: checkbox("check-updates-on-start").checked,
  discord: readDiscordSettings(),
});

const detectPreset = (settings: DiscordSettings): PresetName => {
  for (const [name, preset] of Object.entries(presets)) {
    if (
      preset.statusTemplate === settings.statusTemplate &&
      preset.detailsTemplate === settings.detailsTemplate &&
      preset.stateTemplate === settings.stateTemplate
    ) {
      return name as PresetName;
    }
  }
  return "custom";
};

const populateSettings = (settings: AppSettings) => {
  checkbox("start-maximized").checked = settings.startMaximized;
  checkbox("check-updates-on-start").checked = settings.checkUpdatesOnStart;
  checkbox("discord-enabled").checked = settings.discord.enabled;
  checkbox("show-browsing-activity").checked = settings.discord.showBrowsingActivity;
  checkbox("show-anime-title").checked = settings.discord.showAnimeTitle;
  checkbox("show-season").checked = settings.discord.showSeason;
  checkbox("show-episode").checked = settings.discord.showEpisode;
  checkbox("show-cover").checked = settings.discord.showCover;
  checkbox("show-progress").checked = settings.discord.showProgress;
  checkbox("show-playback-state").checked = settings.discord.showPlaybackState;
  statusTemplate.value = settings.discord.statusTemplate;
  detailsTemplate.value = settings.discord.detailsTemplate;
  stateTemplate.value = settings.discord.stateTemplate;
  browsingDetails.value = settings.discord.browsingDetails;
  browsingState.value = settings.discord.browsingState;
  presencePreset.value = detectPreset(settings.discord);
  setDiscordAvailability();
  updatePreview();
};

const applyPresencePreset = (name: PresetName) => {
  if (name === "custom") {
    return;
  }

  applyingPreset = true;
  const preset = presets[name];
  statusTemplate.value = preset.statusTemplate;
  detailsTemplate.value = preset.detailsTemplate;
  stateTemplate.value = preset.stateTemplate;
  checkbox("show-anime-title").checked = preset.showAnimeTitle;
  checkbox("show-season").checked = preset.showSeason;
  checkbox("show-episode").checked = preset.showEpisode;
  checkbox("show-cover").checked = preset.showCover;
  checkbox("show-progress").checked = preset.showProgress;
  checkbox("show-playback-state").checked = preset.showPlaybackState;
  applyingPreset = false;
  updatePreview();
  setDirty();
};

document
  .querySelector<HTMLElement>("[data-settings-drag-region]")
  ?.addEventListener("mousedown", (event) => {
    if (event.button === 0) {
      runWindowAction(() => settingsWindow.startDragging());
    }
  });

document
  .querySelector<HTMLButtonElement>('[data-window-action="close"]')
  ?.addEventListener("click", () => {
    runWindowAction(() => settingsWindow.hide());
  });

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    runWindowAction(() => settingsWindow.hide());
  }
});

presencePreset.addEventListener("change", () => {
  applyPresencePreset(presencePreset.value as PresetName);
});

for (const input of [statusTemplate, detailsTemplate, stateTemplate]) {
  input.addEventListener("input", () => {
    if (!applyingPreset) {
      presencePreset.value = "custom";
    }
  });
}

settingsForm.addEventListener("input", () => {
  setDiscordAvailability();
  updatePreview();
  setDirty();
});

settingsForm.addEventListener("submit", (event) => {
  event.preventDefault();
  saveButton.disabled = true;
  setStatus(saveStatus, "Saving…");
  void invoke<AppSettings>("save_settings", { settings: readSettings() })
    .then((settings) => {
      populateSettings(settings);
      setStatus(saveStatus, "Settings saved", "success");
    })
    .catch((error: unknown) => {
      saveButton.disabled = false;
      setStatus(saveStatus, errorMessage(error), "error");
    });
});

resetButton.addEventListener("click", () => {
  if (!window.confirm("Reset all settings to their defaults?")) {
    return;
  }

  resetButton.disabled = true;
  setStatus(saveStatus, "Resetting…");
  void invoke<AppSettings>("reset_settings")
    .then((settings) => {
      populateSettings(settings);
      saveButton.disabled = true;
      setStatus(saveStatus, "Defaults restored", "success");
    })
    .catch((error: unknown) => {
      setStatus(saveStatus, errorMessage(error), "error");
    })
    .finally(() => {
      resetButton.disabled = false;
    });
});

checkUpdatesButton.addEventListener("click", () => {
  checkUpdatesButton.disabled = true;
  setStatus(updateStatus, "Checking…");
  void invoke<UpdateCheckResult>("check_for_updates")
    .then((result) => {
      currentVersion.textContent = `Current version ${result.currentVersion}`;
      if (result.availableVersion) {
        setStatus(updateStatus, `Version ${result.availableVersion} is available`, "success");
      } else {
        setStatus(updateStatus, "You are up to date", "success");
      }
    })
    .catch((error: unknown) => {
      setStatus(updateStatus, errorMessage(error), "error");
    })
    .finally(() => {
      checkUpdatesButton.disabled = false;
    });
});

saveButton.disabled = true;
setStatus(saveStatus, "Loading settings…");
void invoke<SettingsBootstrap>("load_settings")
  .then((bootstrap) => {
    populateSettings(bootstrap.settings);
    currentVersion.textContent = `Current version ${bootstrap.appVersion}`;
    settingsLoaded = true;
    setStatus(saveStatus, "");
  })
  .catch((error: unknown) => {
    setStatus(saveStatus, errorMessage(error), "error");
  });
