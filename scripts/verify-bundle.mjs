import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptsDir, "..");
const desktop = resolve(root, "crates", "desktop");
const pin = JSON.parse(readFileSync(resolve(desktop, "gateway-release.json"), "utf8"));

function hostTarget() {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  if (process.platform === "win32") return `${arch}-pc-windows-msvc`;
  if (process.platform === "darwin") return `${arch}-apple-darwin`;
  if (process.platform === "linux") return `${arch}-unknown-linux-gnu`;
  throw new Error(`Unsupported packaging platform: ${process.platform}/${process.arch}`);
}

function packagingTarget() {
  const configured = [
    process.env.ZEST_TARGET_TRIPLE,
    process.env.TAURI_ENV_TARGET_TRIPLE,
    process.env.CARGO_BUILD_TARGET,
  ].filter(Boolean);
  const distinct = [...new Set(configured)];
  if (distinct.length > 1) {
    throw new Error(`Conflicting packaging targets: ${distinct.join(", ")}`);
  }
  return distinct[0] || hostTarget();
}

function targetDirectory() {
  const configured = process.env.CARGO_TARGET_DIR;
  if (!configured) return resolve(root, "target");
  return isAbsolute(configured) ? configured : resolve(process.cwd(), configured);
}

function releaseDirectory(target) {
  const targetDir = targetDirectory();
  const host = hostTarget();
  const explicitTarget = process.env.ZEST_TARGET_TRIPLE || process.env.CARGO_BUILD_TARGET;
  const tauriTarget = process.env.TAURI_ENV_TARGET_TRIPLE;
  const targetScoped = Boolean(explicitTarget || (tauriTarget && tauriTarget !== host));
  return targetScoped ? resolve(targetDir, target, "release") : resolve(targetDir, "release");
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function requireFile(path, label) {
  if (!existsSync(path) || !statSync(path).isFile()) {
    throw new Error(`${label} is missing from the Tauri staging directory: ${path}`);
  }
}

const target = packagingTarget();
const releaseRoot = releaseDirectory(target);
const entry = pin.targets[target];
if (!entry) throw new Error(`No gateway release pin for packaging target ${target}`);

const extension = target.includes("windows") ? ".exe" : "";
const sourceName = `cli-proxy-api-${target}${extension}`;
const sourceSidecar = resolve(desktop, "binaries", sourceName);
const sourceStamp = `${sourceSidecar}.source.json`;
const stagedSidecar = resolve(releaseRoot, `cli-proxy-api${extension}`);
const stagedLicense = resolve(releaseRoot, "licenses", "CLIProxyAPI-LICENSE.txt");

requireFile(sourceSidecar, "Pinned source sidecar");
requireFile(sourceStamp, "Pinned source provenance");
requireFile(stagedSidecar, "Staged sidecar");
requireFile(stagedLicense, "Staged CLIProxyAPI licence");

const sourceHash = sha256(sourceSidecar);
const stagedHash = sha256(stagedSidecar);
if (sourceHash !== entry.binary_sha256) {
  throw new Error(`Source sidecar hash mismatch: expected ${entry.binary_sha256}, got ${sourceHash}`);
}
if (stagedHash !== entry.binary_sha256) {
  throw new Error(`Staged sidecar hash mismatch: expected ${entry.binary_sha256}, got ${stagedHash}`);
}
if (!target.includes("windows")) {
  if ((statSync(sourceSidecar).mode & 0o111) === 0) {
    throw new Error(`Pinned source sidecar is not executable: ${sourceSidecar}`);
  }
  if ((statSync(stagedSidecar).mode & 0o111) === 0) {
    throw new Error(`Staged sidecar is not executable: ${stagedSidecar}`);
  }
}

const license = readFileSync(stagedLicense, "utf8");
for (const marker of [
  "MIT License",
  "Copyright (c) 2025.9-present Router-For.ME",
  "Permission is hereby granted",
]) {
  if (!license.includes(marker)) throw new Error(`Staged CLIProxyAPI licence is missing: ${marker}`);
}

console.log(`Tauri gateway bundle OK: ${target}, CLIProxyAPI ${pin.version}`);
