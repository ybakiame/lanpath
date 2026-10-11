import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { zipSync } from "fflate";

const targets = {
  "x86_64-pc-windows-msvc": { arch: "x64", seed: "amd64" },
  "aarch64-pc-windows-msvc": { arch: "arm64", seed: "arm64" },
} as const;

/** 明确列举分发资源，避免把用户 data、旧构建或多余文件装进 ZIP。 */
export function packagePortable(client: string, target: string): string {
  if (!Object.hasOwn(targets, target)) throw new Error(`Unsupported Portable target: ${target}`);
  const { arch, seed: seedArch } = targets[target as keyof typeof targets];
  const config = JSON.parse(readFileSync(join(client, "src-tauri/tauri.conf.json"), "utf8"));
  const release = join(client, "src-tauri/target", target, "release");
  const seed = join(client, "src-tauri/resources/seed");
  const manifest = JSON.parse(readFileSync(join(seed, "manifest.json"), "utf8"));
  if (manifest.arch !== seedArch) throw new Error(`Seed architecture mismatch: expected ${seedArch}`);
  const name = `Lanpath_${config.version}_windows_${arch}-portable`;
  const entries: Record<string, Uint8Array> = {};
  entries[`${name}/Lanpath.exe`] = readFileSync(join(release, "lanpath-app.exe"));
  entries[`${name}/LICENSE.txt`] = readFileSync(resolve(client, "../../LICENSE"));
  for (const file of [
    "sing-box.exe",
    "wintun.dll",
    "manifest.json",
    "licenses/sing-box-LICENSE.txt",
    "licenses/wintun-LICENSE.txt",
    "licenses/gpl-3.0.txt",
  ]) {
    entries[`${name}/seed/${file}`] = readFileSync(join(seed, file));
  }
  entries[`${name}/lanpath.portable`] = new Uint8Array();
  entries[`${name}/data/`] = new Uint8Array();
  entries[`${name}/README.txt`] = new TextEncoder().encode(
    `Lanpath Portable ${config.version} (${arch})\n\n` +
      "Extract the entire folder to a writable location, then run Lanpath.exe.\n" +
      "Keep lanpath.portable next to the executable. Application data is in data/.\n" +
      "Microsoft Edge WebView2 Runtime must already be installed on Windows.\n" +
      "Source and release history: https://github.com/ybakiame/lanpath\n" +
      "To update: exit Lanpath, replace the program and seed/ from a new Portable zip,\n" +
      "and keep your existing data/ directory and lanpath.portable file.\n" +
      "Do not run two copies using the same data/ directory.\n",
  );
  const output = join(release, "bundle/portable", `${name}.zip`);
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, zipSync(entries));
  return output;
}

if (import.meta.main) {
  const client = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  console.log(packagePortable(client, process.argv[2] ?? ""));
}
