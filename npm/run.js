#!/usr/bin/env node
// npm run script — downloads correct binary then execs

const { execSync } = require("child_process");
const path = require("path");
const fs = require("fs");
const os = require("os");

const BIN_DIR = path.join(__dirname, "bin");
const BIN_NAME = process.platform === "win32" ? "typepress.exe" : "typepress";
const BIN_PATH = path.join(BIN_DIR, BIN_NAME);

function getPlatform() {
  const p = process.platform;
  const a = process.arch;
  if (p === "linux" && a === "x64") return "linux-x86_64";
  if (p === "darwin" && a === "arm64") return "macos-arm64";
  if (p === "darwin" && a === "x64") return "macos-x86_64";
  if (p === "win32" && a === "x64") return "windows-x86_64";
  throw new Error(`Unsupported platform: ${p}/${a}`);
}

function getDownloadUrl(version) {
  const plat = getPlatform();
  const ext = process.platform === "win32" ? "zip" : "tar.gz";
  return `https://github.com/alitrack/typepress/releases/download/v${version}/typepress-${plat}.${ext}`;
}

const pkg = require("../package.json");
const version = pkg.version;

if (!fs.existsSync(BIN_PATH)) {
  fs.mkdirSync(BIN_DIR, { recursive: true });
  const url = getDownloadUrl(version);
  console.error(`Downloading typepress v${version} for ${getPlatform()}...`);
  console.error(`  ${url}`);

  const { spawnSync } = require("child_process");
  if (process.platform === "win32") {
    // TODO: download + unzip on Windows
    throw new Error("Windows auto-download not yet implemented. Please install manually.");
  } else {
    // curl + tar
    execSync(`curl -fsSL "${url}" | tar -xz -C "${BIN_DIR}"`, { stdio: "inherit" });
  }
  fs.chmodSync(BIN_PATH, 0o755);
}

// Exec the binary
const args = process.argv.slice(2);
const result = require("child_process").spawnSync(BIN_PATH, args, { stdio: "inherit" });
process.exit(result.status || 0);
