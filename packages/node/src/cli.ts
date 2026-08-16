#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { platform } from "node:process";
import { downloadBinary } from "./index.js";

const BINARY_NAME = platform === "win32" ? "typepress.exe" : "typepress";

function findBinary(): string | null {
  // Check PATH
  const pathDirs = (process.env.PATH ?? "").split(platform === "win32" ? ";" : ":");
  for (const dir of pathDirs) {
    const candidate = join(dir, BINARY_NAME);
    if (existsSync(candidate)) return candidate;
  }
  const cacheDir = join(
    process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"),
    "typepress",
  );
  const cached = join(cacheDir, BINARY_NAME);
  if (existsSync(cached)) return cached;
  return null;
}

async function resolveBinary(): Promise<string> {
  const found = findBinary();
  if (found) return found;
  // Not on PATH and not cached — fetch the platform binary from GitHub
  // Releases (same mechanism as the programmatic API).
  return downloadBinary();
}

const binary = await resolveBinary();
const args = process.argv.slice(2);
const result = spawnSync(binary, args, { stdio: "inherit" });
process.exit(result.status ?? 1);
