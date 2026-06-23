// Post-install script: download platform binary from GitHub Releases

const { execSync } = require("child_process");
const path = require("path");
const fs = require("fs");
const os = require("os");

const BIN_DIR = path.join(__dirname, "bin");
const BIN_NAME = process.platform === "win32" ? "typepress.exe" : "typepress";
const BIN_PATH = path.join(BIN_DIR, BIN_NAME);

// Skip if already installed and matches version
const pkg = require("../package.json");
const versionFile = path.join(BIN_DIR, ".version");
if (fs.existsSync(BIN_PATH) && fs.existsSync(versionFile)) {
  const installed = fs.readFileSync(versionFile, "utf8").trim();
  if (installed === pkg.version) {
    console.error(`typepress v${pkg.version} already installed`);
    process.exit(0);
  }
}

function getTriple() {
  const p = process.platform;
  const a = process.arch;
  if (p === "linux" && a === "x64") return "linux-x86_64";
  if (p === "darwin" && a === "arm64") return "macos-arm64";
  if (p === "darwin" && a === "x64") return "macos-x86_64";
  if (p === "win32" && a === "x64") return "windows-x86_64";
  throw new Error(`Unsupported platform: ${p}/${a}`);
}

const triple = getTriple();
const ext = process.platform === "win32" ? "zip" : "tar.gz";
const url = `https://github.com/alitrack/typepress/releases/download/v${pkg.version}/typepress-${triple}.${ext}`;

console.error(`Installing typepress v${pkg.version} for ${triple}...`);
fs.mkdirSync(BIN_DIR, { recursive: true });

if (process.platform === "win32") {
  throw new Error("Windows auto-install via npm not yet supported. Download manually.");
} else {
  execSync(`curl -fsSL "${url}" | tar -xz -C "${BIN_DIR}"`, { stdio: "inherit" });
  fs.chmodSync(BIN_PATH, 0o755);
}

fs.writeFileSync(versionFile, pkg.version);
console.error("typepress installed ✓");
