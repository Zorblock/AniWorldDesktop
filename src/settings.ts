import { getCurrentWindow } from "@tauri-apps/api/window";

const settingsWindow = getCurrentWindow();

const runWindowAction = (action: () => Promise<void>) => {
  void action().catch((error: unknown) => {
    console.error("Could not execute the settings window action", error);
  });
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
