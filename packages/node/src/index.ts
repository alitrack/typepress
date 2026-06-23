import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { platform } from "node:process";

// ── Platform detection ──────────────────────────────────────────────────

const BINARY_NAME = platform === "win32" ? "typepress.exe" : "typepress";

function getCacheDir(): string {
  if (platform === "linux") {
    return join(process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"), "typepress");
  }
  if (platform === "darwin") {
    return join(homedir(), "Library", "Caches", "typepress");
  }
  // win32
  return join(process.env.LOCALAPPDATA ?? join(homedir(), "AppData", "Local"), "typepress");
}

function findSystemBinary(): string | null {
  // Check PATH
  const pathSep = platform === "win32" ? ";" : ":";
  const dirs = (process.env.PATH ?? "").split(pathSep);

  for (const dir of dirs) {
    const candidate = join(dir, BINARY_NAME);
    if (existsSync(candidate)) return candidate;
  }

  // Check common locations
  const candidates = [
    join(homedir(), ".local", "bin", BINARY_NAME),
    join(homedir(), ".cargo", "bin", BINARY_NAME),
  ];
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }

  return null;
}

function resolveBinary(binaryPath?: string): string {
  if (binaryPath) return resolve(binaryPath);

  const sysBin = findSystemBinary();
  if (sysBin) return sysBin;

  const cached = join(getCacheDir(), BINARY_NAME);
  if (existsSync(cached)) return cached;

  throw new Error(
    "TypePress binary not found. Install with: npm install typepress or from https://github.com/alitrack/typepress",
  );
}

// ── Types ───────────────────────────────────────────────────────────────

export type OutputFormat = "pdf" | "svg" | "png";
export type InputFormat = "html" | "md";

export interface ConvertOptions {
  /** Output format (default: pdf). */
  format?: OutputFormat;
  /** Page size: A4, A3, Letter, etc. */
  size?: string;
  /** Landscape orientation. */
  landscape?: boolean;
  /** Page margins (e.g. "20mm" or "10 20 30 40"). */
  margin?: string;
  /** PNG scale factor (default: 2.0). */
  scale?: number;
  /** Additional CSS files. */
  cssFiles?: string[];
  /** Additional font files. */
  fonts?: string[];
  /** Header text (top-center, every page). */
  header?: string;
  /** Footer text (bottom-center, every page). */
  footer?: string;
  /** PDF metadata title. */
  title?: string;
  /** Input format (default: html). */
  inputFormat?: InputFormat;
}

// ── API ─────────────────────────────────────────────────────────────────

export class TypePress {
  readonly binaryPath: string;

  constructor(binaryPath?: string) {
    this.binaryPath = resolveBinary(binaryPath);
  }

  /**
   * Convert HTML/Markdown → PDF/SVG/PNG.
   */
  async convert(input: string, output: string, options: ConvertOptions = {}): Promise<void> {
    const {
      format = "pdf",
      size,
      landscape = false,
      margin,
      scale,
      cssFiles,
      fonts,
      header,
      footer,
      title,
      inputFormat = "html",
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
      child.stderr?.on("data", (data: Buffer) => {
        stderr += data.toString();
      });

      child.on("close", (code) => {
        if (code === 0) {
          resolve();
        } else {
          reject(new Error(`TypePress failed (exit ${code}): ${stderr.trim()}`));
        }
      });

      child.on("error", (err) => {
        reject(new Error(`Failed to start TypePress: ${err.message}`));
      });
    });
  }

  /** Convert HTML to PDF. */
  async htmlToPdf(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "pdf", inputFormat: "html" });
    return output;
  }

  /** Convert Markdown to PDF. */
  async mdToPdf(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "pdf", inputFormat: "md" });
    return output;
  }

  /** Convert HTML to multi-page SVG. */
  async htmlToSvg(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "svg", inputFormat: "html" });
    return output;
  }

  /** Convert HTML to PNG. */
  async htmlToPng(input: string, output: string, options: Omit<ConvertOptions, "format" | "inputFormat"> = {}): Promise<string> {
    await this.convert(input, output, { ...options, format: "png", inputFormat: "html" });
    return output;
  }
}

export default TypePress;
