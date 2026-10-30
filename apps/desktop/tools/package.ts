import { cpSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { zipSync } from "fflate";
import { stampWindowsManifest } from "./windows_manifest";
import { buildManagedSeed, managedSourceDigest } from "./managed_core";
import { verifyManagedImageSource, verifyManagedSeed } from "./managed_image";
import {
  assetBase as releaseAssetBase, desktop, digest, product, publicKey, requiredFiles, shippedFiles, signedManifest, targetInfo,
  verifyBytes, verifyManifest, verifyPe, version, type Arch, type Manifest,
} from "./protocol";

export function releaseDir(target: string) { targetInfo(target); return join(desktop, "target", target, "release"); }
export function bundleDir(target: string) { return join(releaseDir(target), "bundle", "gpui"); }
export function stageDir(target: string) { return join(bundleDir(target), "stage"); }
export const assetBase = (arch: Arch) => releaseAssetBase(version, arch);
function readKey() {
  const path = process.env.GPUI_SIGNING_PRIVATE_KEY_FILE;
  if (!path) throw new Error("GPUI_SIGNING_PRIVATE_KEY_FILE must name the independent Ed25519 PKCS8 PEM key");
  return readFileSync(path, "utf8");
}
export async function stage(target: string) {
  const info = targetInfo(target);
  // Fail before network or deleting previous output when the independent key is absent.
  const key = readKey();
  const sourceDigest = managedSourceDigest();
  const exe = readFileSync(join(releaseDir(target), product.binary_name));
  verifyPe(exe, target);
  verifyManagedImageSource(exe, sourceDigest);
  const stage = stageDir(target);
  assertNotLink(stage);
  if (existsSync(stage)) rmSync(stage, { recursive: true });
  mkdirSync(stage, { recursive: true });
  // GPUI alone uses the source-pinned, ownership-reporting managed core.
  await buildManagedSeed(target, join(stage, "resources", "seed"));
  const seed = JSON.parse(readFileSync(join(stage, "resources", "seed", "manifest.json"), "utf8"));
  if (seed.managed_source_digest !== sourceDigest) throw new Error("Managed core source changed after the application binding was verified");
  writeFileSync(join(stage, product.binary_name), exe);
  stampWindowsManifest(join(stage, product.binary_name));
  cpSync(resolve(desktop, "../../LICENSE"), join(stage, "LICENSE.txt"));
  verifyPe(readFileSync(join(stage, "resources/seed/sing-box.exe")), target);
  verifyPe(readFileSync(join(stage, "resources/seed/wintun.dll")), target);
  const files = requiredFiles.map(path => digest(path, readFileSync(join(stage, path))));
  const app = files.find(file => file.path === product.binary_name);
  if (!app) throw new Error("Product executable absent from trusted bundle");
  const manifest: Manifest = {
    schema: 1, product: product.app_id, channel: product.update_channel, version: version!, arch: info.arch,
    artifact: { url: `${product.repository}/releases/download/gpui-v${version}/${product.binary_name}`,
      size: app.size, sha256: app.sha256 }, files,
  };
  const signed = signedManifest(manifest, key);
  verifyManifest(signed.payload, signed.signature, info.arch, version);
  writeFileSync(join(stage, "bundle.json"), signed.payload);
  writeFileSync(join(stage, "bundle.json.sig"), signed.signature);
  return stage;
}
export function assertNotLink(path: string) {
  let current = resolve(path);
  while (true) {
    try {
      if (lstatSync(current).isSymbolicLink()) throw new Error("GPUI packaging refuses symlink/reparse paths");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    const parent = dirname(current);
    if (current === parent) break;
    current = parent;
  }
}
export function verifyStage(target: string, stage = stageDir(target), key = publicKey()) {
  const info = targetInfo(target);
  assertNotLink(stage);
  const found = new Set<string>();
  function walk(root: string, prefix = "") {
    for (const entry of readdirSync(root, { withFileTypes: true })) {
      const relative = `${prefix}${entry.name}`;
      if (entry.isDirectory()) walk(join(root, entry.name), `${relative}/`);
      else if (!entry.isFile() || !shippedFiles.includes(relative)) throw new Error("Non-shipped or nonregular bundle resource");
      else found.add(relative);
    }
  }
  walk(stage);
  if (found.size !== shippedFiles.length) throw new Error("Incomplete GPUI stage");
  const manifest = verifyManifest(readFileSync(join(stage, "bundle.json")),
    readFileSync(join(stage, "bundle.json.sig"), "utf8"), info.arch, version, key);
  if (manifest.artifact.url.split("/").at(-1) !== product.binary_name) throw new Error("Stage requires executable bundle descriptor");
  let image: Buffer | undefined, seed: Buffer | undefined;
  for (const file of manifest.files) {
    const bytes = readFileSync(join(stage, file.path));
    verifyBytes(bytes, file);
    if ([product.binary_name, "resources/seed/sing-box.exe", "resources/seed/wintun.dll"].includes(file.path)) verifyPe(bytes, target);
    if (file.path === product.binary_name) image = bytes;
    if (file.path === "resources/seed/manifest.json") seed = bytes;
  }
  const app = manifest.files.find(file => file.path === product.binary_name)!;
  if (app.size !== manifest.artifact.size || app.sha256 !== manifest.artifact.sha256) throw new Error("Bundle application digest mismatch");
  const core = manifest.files.find(file => file.path === "resources/seed/sing-box.exe")!;
  if (!image || !seed) throw new Error("Stage is missing its application or managed seed identity");
  verifyManagedSeed(image, seed, core.sha256, info.seed);
  return manifest;
}
export function portable(target: string) {
  const info = targetInfo(target);
  const stage = stageDir(target);
  verifyStage(target, stage);
  const name = `${assetBase(info.arch)}-portable`;
  const entries: Record<string, Uint8Array> = {};
  for (const path of shippedFiles) entries[`${name}/${path}`] = readFileSync(join(stage, path));
  entries[`${name}/${product.portable_marker}`] = new Uint8Array();
  entries[`${name}/data/`] = new Uint8Array();
  const output = join(bundleDir(target), `${name}.zip`);
  writeFileSync(output, zipSync(entries, { level: 9, mtime: new Date("1980-01-01T00:00:00Z") }));
  return output;
}
export function releaseManifest(target: string) {
  const info = targetInfo(target);
  const files = verifyStage(target).files;
  const key = readKey();
  const path = join(bundleDir(target), `gpui-${info.arch}.json`);
  const asset = join(bundleDir(target), `${assetBase(info.arch)}-portable.zip`);
  const file = digest(basename(asset), readFileSync(asset));
  const manifest: Manifest = { schema: 1, product: product.app_id, channel: product.update_channel,
    version: version!, arch: info.arch, artifact: {
      url: `${product.repository}/releases/download/gpui-v${version}/${basename(asset)}`,
      size: file.size, sha256: file.sha256,
    }, files };
  const signed = signedManifest(manifest, key);
  verifyManifest(signed.payload, signed.signature, info.arch, version);
  writeFileSync(path, signed.payload);
  writeFileSync(`${path}.sig`, signed.signature);
  const outputs = [
    `${assetBase(info.arch)}-portable.zip`,
    `gpui-${info.arch}.json`, `gpui-${info.arch}.json.sig`,
  ];
  writeFileSync(join(bundleDir(target), "GPUI-SHA256SUMS.txt"), outputs.sort().map(name =>
    `${digest(name, readFileSync(join(bundleDir(target), name))).sha256}  ${name}\n`).join(""));
  return path;
}
if (import.meta.main) {
  const [command, target] = process.argv.slice(2);
  if (!target) throw new Error("Usage: bun tools/package.ts stage|portable|manifest|all TARGET");
  if (command === "stage" || command === "all") await stage(target);
  if (command === "portable" || command === "all") console.log(portable(target));
  if (command === "manifest" || command === "all") console.log(releaseManifest(target));
  if (!["stage", "portable", "manifest", "all"].includes(command)) throw new Error("Unknown packaging command");
}
