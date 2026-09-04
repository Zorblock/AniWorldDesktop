import { invoke } from "@tauri-apps/api/core";

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
  browserLanguage:
    | "automatic"
    | "de-DE"
    | "en-US"
    | "fr-FR"
    | "es-ES"
    | "it-IT"
    | "pl-PL"
    | "pt-BR"
    | "ja-JP";
  appearance: {
    nyanCatScrollbar: boolean;
  };
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

interface SettingsSaveResult {
  settings: AppSettings;
  restartRequired: boolean;
}

interface StorageInfo {
  cacheBytes: number;
  browserDataBytes: number;
  appDataBytes: number;
}

type DangerAction = "cache" | "siteData" | "all" | "reset";
type SettingsCategory = "updates" | "general" | "appearance" | "discord" | "data";

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
const browserLanguage = requiredElement<HTMLSelectElement>("browser-language");
const nyanCatScrollbar = checkbox("nyan-cat-scrollbar");
const cacheSize = requiredElement<HTMLElement>("cache-size");
const browserDataSize = requiredElement<HTMLElement>("browser-data-size");
const appDataSize = requiredElement<HTMLElement>("app-data-size");
const storageStatus = requiredElement<HTMLElement>("storage-status");
const dangerDialog = requiredElement<HTMLDialogElement>("danger-confirm");
const customText = requiredElement<HTMLDetailsElement>("custom-text");
const confirmTitle = requiredElement<HTMLElement>("confirm-title");
const confirmMessage = requiredElement<HTMLElement>("confirm-message");
const confirmDangerButton = requiredElement<HTMLButtonElement>("confirm-danger");
const cancelDangerButton = requiredElement<HTMLButtonElement>("cancel-danger");
const dangerButtons = Array.from(
  document.querySelectorAll<HTMLButtonElement>("[data-danger-action]"),
);
const categoryButtons = Array.from(
  document.querySelectorAll<HTMLButtonElement>("[data-settings-category]"),
);
const settingsPanels = Array.from(
  document.querySelectorAll<HTMLElement>("[data-settings-panel]"),
);

let settingsLoaded = false;
let applyingPreset = false;
let pendingDangerAction: DangerAction | null = null;

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

const formatBytes = (bytes: number): string => {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** unit;
  return `${value.toLocaleString(undefined, {
    maximumFractionDigits: unit === 0 ? 0 : 1,
  })} ${units[unit]}`;
};

const renderStorageInfo = (info: StorageInfo) => {
  cacheSize.textContent = formatBytes(info.cacheBytes);
  browserDataSize.textContent = formatBytes(info.browserDataBytes);
  appDataSize.textContent = formatBytes(info.appDataBytes);
};

const loadStorageInfo = () => {
  void invoke<StorageInfo>("browser_storage_info")
    .then((info) => {
      renderStorageInfo(info);
      setStatus(storageStatus, "");
    })
    .catch((error: unknown) => {
      setStatus(storageStatus, errorMessage(error), "error");
    });
};

const restartApp = (statusElement: HTMLElement, message: string) => {
  setStatus(statusElement, message, "success");
  void invoke("restart_app").catch((error: unknown) => {
    setStatus(statusElement, errorMessage(error), "error");
  });
};

const activateCategory = (category: SettingsCategory, moveFocus = false) => {
  for (const button of categoryButtons) {
    const active = button.dataset.settingsCategory === category;
    button.setAttribute("aria-selected", String(active));
    button.tabIndex = active ? 0 : -1;
    if (active && moveFocus) {
      button.focus();
    }
  }
  for (const panel of settingsPanels) {
    const active = panel.dataset.settingsPanel === category;
    panel.hidden = !active;
    if (active) {
      panel.scrollTop = 0;
    }
  }
  if (category === "data") {
    loadStorageInfo();
  }
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

  if (!checkbox("discord-enabled").checked) {
    previewStatus.textContent = "Discord Rich Presence is off";
    previewDetails.textContent = "";
    previewState.textContent = "";
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
  browserLanguage: browserLanguage.value as AppSettings["browserLanguage"],
  appearance: {
    nyanCatScrollbar: checkbox("nyan-cat-scrollbar").checked,
  },
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
  browserLanguage.value = settings.browserLanguage;
  checkbox("nyan-cat-scrollbar").checked = settings.appearance.nyanCatScrollbar;
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
  const preset = detectPreset(settings.discord);
  presencePreset.value = preset;
  customText.open = preset === "custom";
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

for (const [index, button] of categoryButtons.entries()) {
  button.addEventListener("click", () => {
    const category = button.dataset.settingsCategory as SettingsCategory | undefined;
    if (category) {
      activateCategory(category);
    }
  });
  button.addEventListener("keydown", (event) => {
    let nextIndex: number | undefined;
    if (event.key === "ArrowDown" || event.key === "ArrowRight") {
      nextIndex = (index + 1) % categoryButtons.length;
    } else if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
      nextIndex = (index - 1 + categoryButtons.length) % categoryButtons.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = categoryButtons.length - 1;
    }
    if (nextIndex === undefined) {
      return;
    }
    event.preventDefault();
    const nextCategory = categoryButtons[nextIndex]?.dataset
      .settingsCategory as SettingsCategory | undefined;
    if (nextCategory) {
      activateCategory(nextCategory, true);
    }
  });
}

document
  .querySelector<HTMLButtonElement>('[data-window-action="close"]')
  ?.addEventListener("click", () => {
    window.parent.postMessage({ type: "aniworld-desktop-settings-close" }, "*");
  });

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !dangerDialog.open) {
    window.parent.postMessage({ type: "aniworld-desktop-settings-close" }, "*");
  }
});

presencePreset.addEventListener("change", () => {
  const preset = presencePreset.value as PresetName;
  if (preset === "custom") {
    customText.open = true;
  }
  applyPresencePreset(preset);
});

for (const input of [statusTemplate, detailsTemplate, stateTemplate]) {
  input.addEventListener("input", () => {
    if (!applyingPreset) {
      presencePreset.value = "custom";
    }
  });
}

nyanCatScrollbar.addEventListener("change", () => {
  const requestedState = nyanCatScrollbar.checked;
  nyanCatScrollbar.disabled = true;
  void invoke<boolean>("set_nyan_cat_scrollbar", { enabled: requestedState })
    .then((enabled) => {
      nyanCatScrollbar.checked = enabled;
    })
    .catch((error: unknown) => {
      nyanCatScrollbar.checked = !requestedState;
      setStatus(saveStatus, errorMessage(error), "error");
    })
    .finally(() => {
      nyanCatScrollbar.disabled = false;
    });
});

settingsForm.addEventListener("input", () => {
  setDiscordAvailability();
  updatePreview();
  setDirty();
});

settingsForm.addEventListener("submit", (event) => {
  event.preventDefault();
  saveButton.disabled = true;
  setStatus(saveStatus, "Saving…");
  void invoke<SettingsSaveResult>("save_settings", { settings: readSettings() })
    .then((result) => {
      populateSettings(result.settings);
      if (result.restartRequired) {
        restartApp(saveStatus, "Restarting to apply the browser language…");
      } else {
        setStatus(saveStatus, "Settings saved", "success");
      }
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
  void invoke<SettingsSaveResult>("reset_settings")
    .then((result) => {
      populateSettings(result.settings);
      saveButton.disabled = true;
      if (result.restartRequired) {
        restartApp(saveStatus, "Restarting with default settings…");
      } else {
        setStatus(saveStatus, "Defaults restored", "success");
      }
    })
    .catch((error: unknown) => {
      setStatus(saveStatus, errorMessage(error), "error");
    })
    .finally(() => {
      resetButton.disabled = false;
    });
});

const dangerCopy: Record<
  DangerAction,
  { title: string; message: string; confirm: string }
> = {
  cache: {
    title: "Clear browser cache?",
    message: "Removes temporary website files. Sign-ins and settings stay.",
    confirm: "Clear cache",
  },
  siteData: {
    title: "Clear sign-ins?",
    message: "Signs you out and removes website storage, passwords and autofill data. The app will restart.",
    confirm: "Clear sign-ins",
  },
  all: {
    title: "Clear browser data?",
    message: "Deletes browser history, cache, cookies and saved credentials. App settings stay. The app will restart.",
    confirm: "Clear browser data",
  },
  reset: {
    title: "Reset AniWorld Desktop?",
    message: "Deletes all local browser data and app settings. The app will restart with defaults.",
    confirm: "Reset app",
  },
};

for (const button of dangerButtons) {
  button.addEventListener("click", () => {
    const action = button.dataset.dangerAction as DangerAction | undefined;
    if (!action || !(action in dangerCopy)) {
      return;
    }
    pendingDangerAction = action;
    const copy = dangerCopy[action];
    confirmTitle.textContent = copy.title;
    confirmMessage.textContent = copy.message;
    confirmDangerButton.textContent = copy.confirm;
    dangerDialog.showModal();
  });
}

cancelDangerButton.addEventListener("click", () => {
  pendingDangerAction = null;
  dangerDialog.close();
});

dangerDialog.addEventListener("cancel", () => {
  pendingDangerAction = null;
});

confirmDangerButton.addEventListener("click", () => {
  const action = pendingDangerAction;
  if (!action) {
    return;
  }
  pendingDangerAction = null;
  dangerDialog.close();
  dangerButtons.forEach((button) => {
    button.disabled = true;
  });
  setStatus(storageStatus, action === "reset" ? "Preparing reset…" : "Clearing data…");

  if (action === "reset") {
    void invoke("reset_all_app_data")
      .then(() => {
        setStatus(storageStatus, "Restarting with a clean profile…", "success");
      })
      .catch((error: unknown) => {
        setStatus(storageStatus, errorMessage(error), "error");
        dangerButtons.forEach((button) => {
          button.disabled = false;
        });
      });
    return;
  }

  void invoke<StorageInfo>("clear_browser_data", { kind: action })
    .then((info) => {
      renderStorageInfo(info);
      if (action === "siteData" || action === "all") {
        restartApp(storageStatus, "Data cleared. Restarting…");
      } else {
        setStatus(storageStatus, "Cache cleared", "success");
        dangerButtons.forEach((button) => {
          button.disabled = false;
        });
      }
    })
    .catch((error: unknown) => {
      setStatus(storageStatus, errorMessage(error), "error");
      dangerButtons.forEach((button) => {
        button.disabled = false;
      });
    });
});

checkUpdatesButton.addEventListener("click", () => {
  checkUpdatesButton.disabled = true;
  setStatus(updateStatus, "Checking…");
  void invoke<UpdateCheckResult>("check_for_updates")
    .then((result) => {
      currentVersion.textContent = `Version ${result.currentVersion}`;
      if (result.availableVersion) {
        setStatus(
          updateStatus,
          `Version ${result.availableVersion} is ready in the title bar`,
          "success",
        );
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
    currentVersion.textContent = `Version ${bootstrap.appVersion}`;
    settingsLoaded = true;
    setStatus(saveStatus, "");
    loadStorageInfo();
  })
  .catch((error: unknown) => {
    setStatus(saveStatus, errorMessage(error), "error");
  });
