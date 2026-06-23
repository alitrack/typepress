#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { platform } from "node:process";

const BINARY_NAME = platform === "win32" ? "typepress.exe" : "typepress";

function findBinary(): string {
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
  throw new Error("TypePress binary not found.");
}

const binary = findBinary();
const args = process.argv.slice(2);
const result = spawnSync(binary, args, { stdio: "inherit" });
process.exit(result.status ?? 1);
