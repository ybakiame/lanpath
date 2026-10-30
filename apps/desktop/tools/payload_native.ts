import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve, relative, isAbsolute } from "node:path";
import { spawnSync } from "node:child_process";
import { digest, product, publicKey, shippedFiles, targetInfo, verifyBytes, verifyManifest, verifyPe, type Manifest } from "./protocol";
import { assertNotLink } from "./package";
import { localWindowsPath, nativePath, windowsTemporaryRoot } from "./windows_manifest";
import { verifyManagedSeed } from "./managed_image";

export function requireNativeWindows(): void {
  if (process.platform !== "win32" && !(process.platform === "linux" &&
      (process.env.WSL_INTEROP || process.env.WSL_DISTRO_NAME || existsSync("/proc/sys/fs/binfmt_misc/WSLInterop")))) {
    throw new Error("Payload effects require native Windows file-identity admission (Windows or WSL Windows interop); no offline JS mutation fallback");
  }
}

export function bundleIdentity(manifest: Manifest): string {
  return digest("identity", Buffer.from(JSON.stringify([manifest.product, manifest.channel, manifest.arch,
    manifest.version, manifest.files.map(({ path, size, sha256 }) => ({ path, size, sha256 }))]))).sha256;
}

function runHelper(executable: string, command: string, root: string, identity?: string): void {
  const result = spawnSync(executable, [command, nativePath(root), ...(identity ? [identity] : [])],
    { encoding: "utf8", windowsHide: true });
  if (result.error || result.status !== 0) {
    throw new Error(`Native payload transaction failed (${result.status ?? "missing Windows helper/interop prerequisite"}): ${result.stderr || result.stdout || result.error?.message || "native helper refused; recovery state preserved"}`);
  }
}

function withHelper(entries: Map<string, Uint8Array>, run: (executable: string) => void): void {
  const scratch = mkdtempSync(join(windowsTemporaryRoot(), "lanpath-gpui-payload-"));
  try {
    for (const path of shippedFiles) {
      const destination = join(scratch, path);
      mkdirSync(dirname(destination), { recursive: true });
      writeFileSync(destination, entries.get(path)!, { flag: "wx" });
    }
    writeFileSync(join(scratch, product.portable_marker), "", { flag: "wx" });
    mkdirSync(join(scratch, "data"));
    run(join(scratch, product.binary_name));
  } finally {
    // Only this invocation's owned helper stage is recursively removed, never
    // target data, recovery state, or a caller-provided helper bundle.
    rmSync(scratch, { recursive: true, force: true });
  }
}

export function replacePayload(root: string, entries: Map<string, Uint8Array>, bundle: Manifest): void {
  requireNativeWindows();
  root = localWindowsPath(root);
  withHelper(entries, executable => runHelper(executable, "--lanpath-update-payload", root, bundleIdentity(bundle)));
}

export function recoverNativePayload(root: string): void {
  requireNativeWindows();
  const helper = process.env.GPUI_UPDATE_HELPER;
  if (!helper) throw new Error("Set GPUI_UPDATE_HELPER to an external signed GPUI bundle executable supporting --lanpath-recover-payload; never run recovery from the selected root");
  root = localWindowsPath(root);
  const executable = localWindowsPath(helper);
  const source = dirname(executable);
  const overlaps = (a: string, b: string) => { const path = relative(resolve(a), resolve(b)); return !path || (!path.startsWith("..") && !isAbsolute(path)); };
  if (overlaps(root, source) || overlaps(source, root)) throw new Error("Payload recovery helper must be outside the selected root");
  if (executable !== join(source, product.binary_name)) throw new Error("Expected the GPUI product helper executable");
  assertNotLink(source);
  const payload = join(source, "bundle.json"), signature = `${payload}.sig`;
  assertNotLink(payload); assertNotLink(signature);
  const payloadBytes = readFileSync(payload), signatureBytes = readFileSync(signature);
  const raw = JSON.parse(payloadBytes.toString("utf8"));
  if (raw.arch !== "x86_64" && raw.arch !== "aarch64") throw new Error("Unsupported native helper architecture");
  const manifest = verifyManifest(payloadBytes, signatureBytes.toString("utf8"), raw.arch, undefined, publicKey());
  const entries = new Map<string, Uint8Array>([["bundle.json", payloadBytes], ["bundle.json.sig", signatureBytes]]);
  const app = manifest.files.find(file => file.path === product.binary_name)!;
  if (manifest.artifact.url.split("/").at(-1) !== product.binary_name ||
      manifest.artifact.size !== app.size || manifest.artifact.sha256 !== app.sha256) throw new Error("Expected an authenticated executable bundle helper");
  for (const file of manifest.files) {
    assertNotLink(join(source, file.path));
    const bytes = readFileSync(join(source, file.path));
    verifyBytes(bytes, file);
    entries.set(file.path, bytes);
  }
  const target = `${manifest.arch}-pc-windows-msvc`;
  for (const path of [product.binary_name, "resources/seed/sing-box.exe", "resources/seed/wintun.dll"]) {
    verifyPe(entries.get(path)!, target);
  }
  const image = entries.get(product.binary_name)!;
  const core = manifest.files.find(file => file.path === "resources/seed/sing-box.exe")!;
  verifyManagedSeed(Buffer.from(image.buffer, image.byteOffset, image.byteLength),
    entries.get("resources/seed/manifest.json")!, core.sha256, targetInfo(target).seed);
  withHelper(entries, executable => runHelper(executable, "--lanpath-recover-payload", root));
}
