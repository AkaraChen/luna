import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync, spawnSync } from "node:child_process";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(appDir, "../..");
const args = process.argv.slice(2);
const release = args.includes("--release");
const targetArgIndex = args.indexOf("--target");
const target =
  targetArgIndex >= 0 ? args[targetArgIndex + 1] : execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
const profile = release ? "release" : "debug";
const extension = target.includes("windows") ? ".exe" : "";

const cargoArgs = ["build", "-p", "luna"];
if (release) cargoArgs.push("--release");
if (target) cargoArgs.push("--target", target);

const result = spawnSync("cargo", cargoArgs, {
  cwd: repoRoot,
  stdio: "inherit",
});

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const source = join(repoRoot, "target", target, profile, `luna${extension}`);
const destinationDir = join(appDir, "src-tauri", "binaries");
const destination = join(destinationDir, `luna-${target}${extension}`);

mkdirSync(destinationDir, { recursive: true });
copyFileSync(source, destination);
chmodSync(destination, 0o755);
console.log(`prepared ${destination}`);
