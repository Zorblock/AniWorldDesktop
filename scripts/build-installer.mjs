import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const rootDirectory = dirname(dirname(fileURLToPath(import.meta.url)));
const defaultKeyPath = join(homedir(), ".tauri", "AniWorldDesktop.key");
const environment = { ...process.env };

if (
  !environment.TAURI_SIGNING_PRIVATE_KEY &&
  !environment.TAURI_SIGNING_PRIVATE_KEY_PATH
) {
  if (!existsSync(defaultKeyPath)) {
    console.error(`Updater-Signierschluessel fehlt: ${defaultKeyPath}`);
    process.exit(1);
  }
  // Passing the key contents also works across Tauri CLI versions where the
  // path variable is not picked up by the automatic bundle signer.
  environment.TAURI_SIGNING_PRIVATE_KEY = readFileSync(defaultKeyPath, "utf8");
}

environment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= "";

const tauriCli = join(
  rootDirectory,
  "node_modules",
  "@tauri-apps",
  "cli",
  "tauri.js",
);
const result = spawnSync(
  process.execPath,
  [tauriCli, "build", "--bundles", "nsis"],
  {
    cwd: rootDirectory,
    env: environment,
    stdio: "inherit",
  },
);

if (result.error) {
  console.error(result.error.message);
}
process.exit(result.status ?? 1);
