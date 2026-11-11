import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { desktop } from "./protocol";
import { uniqueResource } from "./pe_resources";

function run(executable: string, args: string[]): string {
  const result = spawnSync(executable, args, { encoding: "utf8", windowsHide: true });
  if (result.error || result.status !== 0) {
    throw new Error(`Windows manifest tooling failed (${result.status ?? "launch failure"}): ${result.stderr || result.stdout || result.error?.message}`);
  }
  return result.stdout;
}

export function localWindowsPath(path: string): string {
  if (process.platform !== "win32" && /^(?:[A-Za-z]:[\\/]|\\\\)/.test(path)) {
    return run("wslpath", ["-u", path]).trim();
  }
  return resolve(path);
}

export function nativePath(path: string): string {
  const absolute = localWindowsPath(path);
  return process.platform === "win32" ? absolute : run("wslpath", ["-w", absolute]).trim();
}

export function windowsTemporaryRoot(): string {
  if (process.platform === "win32") return tmpdir();
  if (process.env.GPUI_WINDOWS_TEMP_DIR) return localWindowsPath(process.env.GPUI_WINDOWS_TEMP_DIR);
  // Fixed code only: no paths or other caller input are interpolated into PowerShell.
  const directory = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command",
    "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); [Console]::Write([IO.Path]::GetTempPath())"]).trim();
  if (!directory || directory.includes("\0")) throw new Error("Windows temporary directory discovery failed");
  return run("wslpath", ["-u", directory]).trim();
}


/** Finalize only an owned stage copy, before hashing/signing; never patch the build image. */
export function stampWindowsManifest(executable: string): void {
  const tool = process.env.GPUI_MT ?? (process.platform === "win32" ? "mt.exe" : undefined);
  if (!tool) throw new Error("Set GPUI_MT to the Windows SDK mt.exe when packaging from WSL");
  // SDK mt.exe rejects WSL UNC XML paths. Work entirely in a fresh Windows temp
  // directory, then copy the validated result back to the unsigned bundle stage.
  const scratch = mkdtempSync(join(windowsTemporaryRoot(), "lanpath-gpui-manifest-"));
  try {
    const image = join(scratch, "lanpath-gpui.exe");
    const source = join(scratch, "source.manifest");
    const expected = join(scratch, "expected.manifest");
    const actual = join(scratch, "actual.manifest");
    copyFileSync(executable, image);
    writeFileSync(source, readFileSync(join(desktop, "app.manifest")));
    const validate = ["-validate_manifest", "-check_for_duplicates", "-canonicalize"];
    run(tool, ["-nologo", "-manifest", nativePath(source), ...validate, `-out:${nativePath(expected)}`]);
    run(tool, ["-nologo", "-manifest", nativePath(expected), `-outputresource:${nativePath(image)};#1`]);
    run(tool, ["-nologo", `-inputresource:${nativePath(image)};#1`, ...validate, `-out:${nativePath(actual)}`]);
    if (!readFileSync(expected).equals(readFileSync(actual))) throw new Error("Embedded Windows manifest differs from the canonical product manifest");
    uniqueResource(readFileSync(image), 24, 1, 128 * 1024);
    copyFileSync(image, executable);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}
