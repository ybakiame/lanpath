import { test, expect } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { unzipSync } from "fflate";
import { packagePortable } from "./package-portable";

test("Portable ZIP isolates user data and rejects wrong-architecture seeds", () => {
  const root = mkdtempSync(join(tmpdir(), "lanpath-portable-"));
  const client = join(root, "apps/client");
  writeFileSync(join(root, "LICENSE"), "fixture license");
  const target = "x86_64-pc-windows-msvc";
  const write = (path: string, text: string) => {
    mkdirSync(join(client, path, ".."), { recursive: true });
    writeFileSync(join(client, path), text);
  };
  try {
    write("src-tauri/tauri.conf.json", '{"version":"0.1.1"}');
    write(`src-tauri/target/${target}/release/lanpath-app.exe`, "fixture app");
    write(`src-tauri/target/${target}/release/data/client.json`, "private data");
    write("src-tauri/resources/seed/manifest.json", '{"arch":"amd64"}');
    for (const file of [
      "sing-box.exe",
      "wintun.dll",
      "licenses/sing-box-LICENSE.txt",
      "licenses/wintun-LICENSE.txt",
      "licenses/gpl-3.0.txt",
    ]) {
      write(`src-tauri/resources/seed/${file}`, file);
    }
    const entries = unzipSync(readFileSync(packagePortable(client, target)));
    const prefix = "Lanpath_0.1.1_windows_x64-portable/";
    expect(entries[`${prefix}Lanpath.exe`]).toBeDefined();
    expect(entries[`${prefix}lanpath.portable`]).toHaveLength(0);
    expect(entries[`${prefix}data/`]).toHaveLength(0);
    expect(Object.keys(entries).filter((path) => path.includes("data/")).length).toBe(1);
    expect(entries[`${prefix}seed/wintun.dll`]).toBeDefined();
    expect(entries[`${prefix}LICENSE.txt`]).toBeDefined();
    write("src-tauri/resources/seed/manifest.json", '{"arch":"arm64"}');
    expect(() => packagePortable(client, target)).toThrow("architecture mismatch");
    const arm = "aarch64-pc-windows-msvc";
    write(`src-tauri/target/${arm}/release/lanpath-app.exe`, "fixture arm app");
    const armEntries = unzipSync(readFileSync(packagePortable(client, arm)));
    expect(armEntries["Lanpath_0.1.1_windows_arm64-portable/Lanpath.exe"]).toBeDefined();
    expect(() => packagePortable(client, "invalid")).toThrow("Unsupported Portable target");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
