import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  cancel,
  confirm,
  intro,
  isCancel,
  log,
  note,
  outro,
  select,
  spinner,
} from "@clack/prompts";

const rootDirectory = dirname(dirname(fileURLToPath(import.meta.url)));
const githubRepository = "Zorblock/AniWorldDesktop";
const versionFiles = [
  "package.json",
  "package-lock.json",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "src-tauri/tauri.conf.json",
];
const allowedTypes = new Set(["patch", "minor", "major"]);
const args = process.argv.slice(2);
const releaseTypeArgument = args.find((argument) => allowedTypes.has(argument));
const assumeYes = args.includes("--yes");
const dryRun = args.includes("--dry-run");
const createPublic = args.includes("--create-public");

if (args.includes("--help")) {
  console.log(`AniWorld Desktop Release

Usage:
  npm run release
  npm run release -- patch|minor|major
  npm run release -- patch --yes
  npm run release -- patch --yes --create-public
  npm run release -- patch --dry-run

Options:
  --yes            Skip the final confirmation.
  --create-public  Allow creation of the public GitHub repository with --yes.
  --dry-run        Validate and show the plan without changing anything.`);
  process.exit(0);
}

function run(command, commandArgs, options = {}) {
  const executable =
    process.platform === "win32" && ["git", "gh"].includes(command)
      ? `${command}.exe`
      : command;
  return spawnSync(executable, commandArgs, {
    cwd: rootDirectory,
    encoding: "utf8",
    ...options,
  });
}

function runNpm(commandArgs, options = {}) {
  const npmExecPath = process.env.npm_execpath;
  if (npmExecPath) {
    return spawnSync(process.execPath, [npmExecPath, ...commandArgs], {
      cwd: rootDirectory,
      encoding: "utf8",
      ...options,
    });
  }

  return spawnSync(
    process.platform === "win32" ? "npm.cmd" : "npm",
    commandArgs,
    {
      cwd: rootDirectory,
      encoding: "utf8",
      shell: process.platform === "win32",
      ...options,
    },
  );
}

function outputOf(result) {
  return `${result.stdout ?? ""}${result.stderr ?? ""}`.trim();
}

function fail(message, result) {
  cancel(message);
  const output = result && outputOf(result);
  if (output) log.error(output);
  process.exit(result?.status ?? 1);
}

function ensureSuccess(result, message) {
  if (result.error || result.status !== 0) fail(message, result);
  return result;
}

function readJson(relativePath) {
  return JSON.parse(readFileSync(join(rootDirectory, relativePath), "utf8"));
}

function cargoVersion(relativePath) {
  const contents = readFileSync(join(rootDirectory, relativePath), "utf8");
  const packageSection = contents.match(/\[package\][\s\S]*?(?=\n\[|$)/)?.[0];
  return packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
}

function cargoLockVersion() {
  const contents = readFileSync(
    join(rootDirectory, "src-tauri/Cargo.lock"),
    "utf8",
  );
  const packageSection = contents
    .split("[[package]]")
    .find((section) => /^\s*name\s*=\s*"aniworld-desktop"\s*$/m.test(section));
  return packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
}

function currentVersions() {
  return {
    npm: readJson("package.json").version,
    npmLock: readJson("package-lock.json").packages[""].version,
    cargo: cargoVersion("src-tauri/Cargo.toml"),
    cargoLock: cargoLockVersion(),
    tauri: readJson("src-tauri/tauri.conf.json").version,
  };
}

function validateVersion(version) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    fail(`Invalid version: ${version ?? "(missing)"}`);
  }
}

function nextVersion(currentVersion, type) {
  const [major, minor, patch] = currentVersion.split(".").map(Number);
  if (type === "major") return `${major + 1}.0.0`;
  if (type === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

function replaceVersion(relativePath, pattern, replacement) {
  const path = join(rootDirectory, relativePath);
  const contents = readFileSync(path, "utf8");
  const updated = contents.replace(pattern, replacement);
  if (updated === contents) {
    throw new Error(
      `Could not update the version in ${relativePath}.`,
    );
  }
  writeFileSync(path, updated, "utf8");
}

function restoreFiles(snapshots) {
  for (const [relativePath, contents] of snapshots) {
    writeFileSync(join(rootDirectory, relativePath), contents);
  }
}

function findInstaller(version) {
  const bundleDirectory = join(
    rootDirectory,
    "src-tauri",
    "target",
    "release",
    "bundle",
    "nsis",
  );
  const expected = join(
    bundleDirectory,
    `AniWorld Desktop_${version}_x64-setup.exe`,
  );
  if (existsSync(expected)) return expected;

  if (!existsSync(bundleDirectory)) return undefined;
  const matches = readdirSync(bundleDirectory)
    .filter(
      (name) => name.includes(`_${version}_`) && name.endsWith("-setup.exe"),
    )
    .map((name) => join(bundleDirectory, name));
  return matches.length === 1 ? matches[0] : undefined;
}

function githubAssetName(fileName) {
  return fileName.replaceAll(" ", ".");
}

function writeChecksum(installerPath) {
  const checksum = createHash("sha256")
    .update(readFileSync(installerPath))
    .digest("hex");
  const checksumPath = `${installerPath}.sha256.txt`;
  writeFileSync(
    checksumPath,
    `${checksum}  ${githubAssetName(basename(installerPath))}\n`,
    "utf8",
  );
  return checksumPath;
}

function writeUpdateManifest(version, installerPath, signaturePath) {
  const tag = `v${version}`;
  const installerName = githubAssetName(basename(installerPath));
  const manifestPath = join(dirname(installerPath), "latest.json");
  const manifest = {
    version,
    notes: `AniWorld Desktop ${tag}`,
    pub_date: new Date().toISOString(),
    platforms: {
      "windows-x86_64": {
        signature: readFileSync(signaturePath, "utf8").trim(),
        url: `https://github.com/${githubRepository}/releases/download/${tag}/${installerName}`,
      },
    },
  };
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  return manifestPath;
}

function normalizedRepository(remoteUrl) {
  return remoteUrl
    .trim()
    .replace(/^git@github\.com:/i, "")
    .replace(/^https?:\/\/github\.com\//i, "")
    .replace(/\.git$/i, "")
    .toLowerCase();
}

function repositoryExists() {
  const result = run("gh", [
    "repo",
    "view",
    githubRepository,
    "--json",
    "nameWithOwner",
  ]);
  return result.status === 0;
}

function connectOrCreateRepository(visibility) {
  const origin = run("git", ["remote", "get-url", "origin"]);
  if (origin.status === 0) {
    if (
      normalizedRepository(origin.stdout) !== githubRepository.toLowerCase()
    ) {
      fail(
        `origin does not point to ${githubRepository}: ${origin.stdout.trim()}`,
      );
    }
    return;
  }

  if (repositoryExists()) {
    ensureSuccess(
      run("git", [
        "remote",
        "add",
        "origin",
        `https://github.com/${githubRepository}.git`,
      ]),
      "Could not add the GitHub remote.",
    );
    return;
  }

  ensureSuccess(
    run("gh", [
      "repo",
      "create",
      githubRepository,
      `--${visibility}`,
      "--source",
      rootDirectory,
      "--remote",
      "origin",
      "--description",
      "AniWorld Desktop with Discord Rich Presence",
    ]),
    "Could not create the GitHub repository.",
  );
}

function verifyRemoteState(branch, tag) {
  ensureSuccess(
    run("git", ["fetch", "origin", "--tags"]),
    "git fetch failed.",
  );

  const localTag = run("git", [
    "rev-parse",
    "--verify",
    "--quiet",
    `refs/tags/${tag}`,
  ]);
  if (localTag.status === 0) fail(`The tag ${tag} already exists.`);

  const release = run("gh", [
    "release",
    "view",
    tag,
    "--repo",
    githubRepository,
  ]);
  if (release.status === 0)
    fail(`The GitHub release ${tag} already exists.`);

  const remoteBranch = run("git", [
    "show-ref",
    "--verify",
    "--quiet",
    `refs/remotes/origin/${branch}`,
  ]);
  if (remoteBranch.status === 0) {
    const containsRemote = run("git", [
      "merge-base",
      "--is-ancestor",
      `origin/${branch}`,
      "HEAD",
    ]);
    if (containsRemote.status !== 0) {
      fail(
        `The local branch ${branch} is behind origin/${branch}. Pull the latest changes first.`,
      );
    }
  }
}

intro("AniWorld Desktop Release");

const versions = currentVersions();
for (const version of Object.values(versions)) validateVersion(version);
const uniqueVersions = new Set(Object.values(versions));
if (uniqueVersions.size !== 1) {
  fail(
    `Version files are out of sync:\n${JSON.stringify(versions, null, 2)}`,
  );
}
const currentVersion = versions.npm;

note(`v${currentVersion}\nMajor.Minor.Patch`, "Current Version");

let releaseType = releaseTypeArgument;
if (!releaseType) {
  releaseType = await select({
    message: "Which version should be released?",
    initialValue: "patch",
    options: [
      {
        value: "patch",
        label: `Patch  ->  v${nextVersion(currentVersion, "patch")}`,
        hint: "Bug fixes",
      },
      {
        value: "minor",
        label: `Minor  ->  v${nextVersion(currentVersion, "minor")}`,
        hint: "New features",
      },
      {
        value: "major",
        label: `Major  ->  v${nextVersion(currentVersion, "major")}`,
        hint: "Breaking changes",
      },
    ],
  });
  if (isCancel(releaseType)) {
    cancel("Release cancelled.");
    process.exit(0);
  }
}

const targetVersion = nextVersion(currentVersion, releaseType);
const tag = `v${targetVersion}`;

ensureSuccess(
  run("git", ["rev-parse", "--is-inside-work-tree"]),
  "No Git repository found.",
);
const branch = outputOf(
  ensureSuccess(
    run("git", ["branch", "--show-current"]),
    "Could not determine the Git branch.",
  ),
);
if (!branch) fail("Releases cannot be created from a detached HEAD.");

const status = outputOf(run("git", ["status", "--porcelain"]));
if (status && !dryRun) {
  fail(`The Git working tree must be clean before a release:\n${status}`);
}
if (status && dryRun) {
  log.warn("Dry Run: The current Git working tree is not clean.");
}

ensureSuccess(
  run("gh", ["auth", "status"]),
  "GitHub CLI is not authenticated.",
);
const repoExists = repositoryExists();
let visibility = "public";
if (!repoExists) {
  if (assumeYes && !createPublic) {
    fail(
      `${githubRepository} does not exist yet. Run interactively or confirm its creation with --create-public.`,
    );
  }
  if (!assumeYes && !dryRun) {
    visibility = await select({
      message: `${githubRepository} does not exist yet. Choose its visibility:`,
      initialValue: "public",
      options: [
        {
          value: "public",
          label: "Public",
          hint: "Setup is publicly downloadable",
        },
        {
          value: "private",
          label: "Private",
          hint: "Authorized users only",
        },
      ],
    });
    if (isCancel(visibility)) {
      cancel("Release cancelled.");
      process.exit(0);
    }
  }
}

if (!assumeYes && !dryRun) {
  const approved = await confirm({
    message: `Build, commit, tag, push, and publish ${tag} on GitHub?`,
    initialValue: true,
  });
  if (isCancel(approved) || !approved) {
    cancel("Release cancelled.");
    process.exit(0);
  }
}

if (dryRun) {
  note(
    [
      `Version: ${currentVersion} -> ${targetVersion}`,
      `Branch: ${branch}`,
      `Repository: ${githubRepository}${repoExists ? "" : " (will be created)"}`,
      "Artifacts: NSIS setup + signature + SHA-256 + latest.json",
    ].join("\n"),
    "Dry Run",
  );
  outro("No files or external data were changed.");
  process.exit(0);
}

connectOrCreateRepository(visibility);
verifyRemoteState(branch, tag);

const snapshots = new Map(
  versionFiles.map((relativePath) => [
    relativePath,
    readFileSync(join(rootDirectory, relativePath)),
  ]),
);
const versionSpinner = spinner();
versionSpinner.start(`Setting version to ${targetVersion}`);

try {
  ensureSuccess(
    runNpm(["version", targetVersion, "--no-git-tag-version"]),
    "Could not update the npm version.",
  );
  replaceVersion(
    "src-tauri/Cargo.toml",
    /(^\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
    `$1"${targetVersion}"`,
  );
  replaceVersion(
    "src-tauri/tauri.conf.json",
    /(^\s*"version"\s*:\s*)"[^"]+"/m,
    `$1"${targetVersion}"`,
  );
} catch (error) {
  restoreFiles(snapshots);
  versionSpinner.error(
    "Version update failed and was reverted.",
  );
  fail(error.message);
}
versionSpinner.stop(`Version updated to ${targetVersion}`);

log.step("Building the NSIS setup...");
const buildResult = runNpm(["run", "installer"], { stdio: "inherit" });
if (buildResult.error || buildResult.status !== 0) {
  restoreFiles(snapshots);
  fail(
    "Installer build failed. Version files were reverted.",
    buildResult,
  );
}

const installerPath = findInstaller(targetVersion);
if (!installerPath) {
  restoreFiles(snapshots);
  fail(`Could not find the setup for version ${targetVersion}.`);
}
const signaturePath = `${installerPath}.sig`;
if (!existsSync(signaturePath)) {
  restoreFiles(snapshots);
  fail(`Could not find the updater signature for version ${targetVersion}.`);
}
const checksumPath = writeChecksum(installerPath);
const updateManifestPath = writeUpdateManifest(
  targetVersion,
  installerPath,
  signaturePath,
);

const builtVersions = currentVersions();
if (Object.values(builtVersions).some((version) => version !== targetVersion)) {
  restoreFiles(snapshots);
  fail(
    `Version files are out of sync after the build:\n${JSON.stringify(builtVersions, null, 2)}`,
  );
}

ensureSuccess(
  run("git", ["add", "--", ...versionFiles]),
  "Could not stage the version files.",
);
ensureSuccess(
  run("git", ["commit", "-m", `release: ${tag}`], { stdio: "inherit" }),
  "Release commit failed.",
);
ensureSuccess(
  run("git", ["tag", "-a", tag, "-m", `AniWorld Desktop ${tag}`]),
  "Git tag failed.",
);

const upstream = run("git", [
  "rev-parse",
  "--abbrev-ref",
  "--symbolic-full-name",
  "@{upstream}",
]);
const pushBranchArgs =
  upstream.status === 0
    ? ["push", "origin", branch]
    : ["push", "--set-upstream", "origin", branch];
ensureSuccess(
  run("git", pushBranchArgs, { stdio: "inherit" }),
  "Branch push failed.",
);
ensureSuccess(
  run("git", ["push", "origin", tag], { stdio: "inherit" }),
  "Tag push failed.",
);

ensureSuccess(
  run(
    "gh",
    [
      "release",
      "create",
      tag,
      installerPath,
      signaturePath,
      checksumPath,
      updateManifestPath,
      "--repo",
      githubRepository,
      "--verify-tag",
      "--latest",
      "--generate-notes",
      "--title",
      `AniWorld Desktop ${tag}`,
    ],
    { stdio: "inherit" },
  ),
  "GitHub release or upload failed.",
);

outro(`AniWorld Desktop ${tag} was published successfully.`);
