import { spawn, execSync } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, chmodSync } from "node:fs";
import { get } from "node:https";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { platform } from "node:process";
import { createGunzip } from "node:zlib";
import { createWriteStream as fsCreateWriteStream } from "node:fs";
import { Readable } from "node:stream";

const VERSION = "0.3.2";
const GITHUB_RELEASES = "https://github.com/alitrack/typepress/releases/download";

const BINARY_NAME = platform === "win32" ? "typepress.exe" : "typepress";

function getCacheDir(): string {
  if (platform === "linux") {
    return join(process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"), "typepress");
  }
  if (platform === "darwin") {
    return join(homedir(), "Library", "Caches", "typepress");
  }
  return join(process.env.LOCALAPPDATA ?? join(homedir(), "AppData", "Local"), "typepress");
}

function getPlatformTag(): string {
  if (platform === "linux") return "linux-x86_64";
  if (platform === "darwin") return "macos-arm64"; // Intel Macs use Rosetta 2
  if (platform === "win32") return "windows-x86_64";
  throw new Error(`Unsupported platform: ${platform}`);
}

function findSystemBinary(): string | null {
  const pathSep = platform === "win32" ? ";" : ":";
  const dirs = (process.env.PATH ?? "").split(pathSep);
  for (const dir of dirs) {
    const candidate = join(dir, BINARY_NAME);
    if (existsSync(candidate)) return candidate;
  }
  const candidates = [join(homedir(), ".local", "bin", BINARY_NAME), join(homedir(), ".cargo", "bin", BINARY_NAME)];
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  return null;
}

async function downloadFile(url: string, dest: string): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const file = createWriteStream(dest);
    get(url, (res) => {
      if (res.statusCode === 302 || res.statusCode === 301) {
        get(res.headers.location!, (r2) => {
          r2.pipe(file);
          file.on("finish", resolve);
          file.on("error", reject);
        });
        return;
      }
      res.pipe(file);
      file.on("finish", resolve);
      file.on("error", reject);
    }).on("error", reject);
  });
}

async function downloadBinary(version: string = VERSION): Promise<string> {
  const cacheDir = getCacheDir();
  mkdirSync(cacheDir, { recursive: true });
  const cached = join(cacheDir, BINARY_NAME);
  if (existsSync(cached)) return cached;

  const plat = getPlatformTag();
  const ext = platform === "win32" ? "zip" : "tar.gz";
  const url = `${GITHUB_RELEASES}/v${version}/typepress-${plat}.${ext}`;

  console.error(`Downloading TypePress v${version} for ${plat}...`);

  const tmpDir = join(cacheDir, ".tmp");
  mkdirSync(tmpDir, { recursive: true });
  const archivePath = join(tmpDir, `typepress.${ext}`);

  await downloadFile(url, archivePath);

  if (ext === "tar.gz") {
    execSync(`tar -xzf "${archivePath}" -C "${cacheDir}"`, { stdio: "ignore" });
  } else {
    try {
      execSync(`unzip -o "${archivePath}" -d "${cacheDir}"`, { stdio: "ignore" });
    } catch {
      execSync(`powershell -command "Expand-Archive -Path '${archivePath}' -DestinationPath '${cacheDir}' -Force"`, { stdio: "ignore" });
    }
  }

  try { const { unlinkSync, rmdirSync } = (await eval('import("node:fs")')) as any; unlinkSync(archivePath); rmdirSync(tmpDir); } catch {}

  if (!existsSync(cached)) {
    throw new Error(`Failed to download TypePress binary`);
  }

  try { chmodSync(cached, 0o755); } catch {}

  console.error(`TypePress installed to ${cached}`);
  return cached;
}

function resolveBinary(binaryPath?: string): string {
  if (binaryPath) return resolve(binaryPath);
  const sysBin = findSystemBinary();
  if (sysBin) return sysBin;
  const cached = join(getCacheDir(), BINARY_NAME);
  if (existsSync(cached)) return cached;
  throw new Error("TypePress binary not found. Call `await TypePress.download()` first.");
}

// ── Types ───────────────────────────────────────────────────────────────

export type OutputFormat = "pdf" | "svg" | "png";
export type InputFormat = "html" | "md";

export interface ConvertOptions {
  format?: OutputFormat;
  size?: string;
  landscape?: boolean;
  margin?: string;
  scale?: number;
  cssFiles?: string[];
  fonts?: string[];
  header?: string;
  footer?: string;
  title?: string;
  inputFormat?: InputFormat;
}

// ── API ─────────────────────────────────────────────────────────────────

export class TypePress {
  readonly binaryPath: string;

  constructor(binaryPath?: string) {
    this.binaryPath = resolveBinary(binaryPath);
  }

  /** Download the TypePress binary for this platform. */
  static async download(): Promise<string> {
    return downloadBinary();
  }

  /**
   * Convert HTML/Markdown → PDF/SVG/PNG.
   */
  async convert(input: string, output: string, options: ConvertOptions = {}): Promise<void> {
    const {
      format = "pdf", size, landscape = false, margin, scale,
      cssFiles, fonts, header, footer, title, inputFormat = "html",
    } = options;

    const args: string[] = [];
    if (inputFormat === "md") args.push("--from", "md");
    if (format !== "pdf") args.push("--format", format);
    if (size) args.push("--size", size);
    if (landscape) args.push("--landscape");
    if (margin) args.push("--margin", margin);
    if (scale !== undefined && scale !== 2.0) args.push("--scale", String(scale));
    if (cssFiles) for (const f of cssFiles) args.push("--css", f);
    if (fonts) for (const f of fonts) args.push("--font", f);
    if (header) args.push("--header", header);
    if (footer) args.push("--footer", footer);
    if (title) args.push("--title", title);
    args.push(input, "-o", output);

    return new Promise<void>((resolve, reject) => {
      const child = spawn(this.binaryPath, args, { stdio: ["ignore", "pipe", "pipe"] });
      let stderr = "";
      child.stderr?.on("data", (data: Buffer) => { stderr += data.toString(); });
      child.on("close", (code: number | null) => {
        code === 0 ? resolve() : reject(new Error(`TypePress failed (exit ${code}): ${stderr.trim()}`));
      });
      child.on("error", (err: Error) => reject(new Error(`Failed to start TypePress: ${err.message}`)));
    });
  }

  async htmlToPdf(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "pdf", inputFormat: "html" });
    return output;
  }

  async mdToPdf(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "pdf", inputFormat: "md" });
    return output;
  }

  async htmlToSvg(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "svg", inputFormat: "html" });
    return output;
  }

  async htmlToPng(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "png", inputFormat: "html" });
    return output;
  }
}

export default TypePress;
