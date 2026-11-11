import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { unzipSync } from "fflate";
import { assetBase, product, publicKey, shippedFiles, targetInfo, verifyBytes, verifyManifest, verifyPe, type Arch, type Manifest } from "./protocol";
import { recoverNativePayload, replacePayload, requireNativeWindows } from "./payload_native";
import { localWindowsPath } from "./windows_manifest";
import { verifyManagedSeed } from "./managed_image";

export function verifyArtifact(manifestPath: string, artifactPath: string, arch: Arch, key = publicKey()): Manifest {
  const manifest = verifyManifest(readFileSync(manifestPath), readFileSync(`${manifestPath}.sig`, "utf8"), arch, undefined, key);
  const name = manifest.artifact.url.split("/").at(-1);
  const base = assetBase(manifest.version, arch);
  if (basename(manifestPath) !== `gpui-${arch}.json` || basename(artifactPath) !== name || name !== `${base}-portable.zip`) {
    throw new Error("Artifact filename or distribution kind mismatch");
  }
  const bytes = readFileSync(artifactPath);
  verifyBytes(bytes, manifest.artifact);
  portableEntries(bytes, manifest, key);
  return manifest;
}
export function portableEntries(archive: Uint8Array, manifest: Manifest, key = publicKey()) {
  // Authenticate the complete ZIP before parsing any compressed attacker-controlled metadata.
  verifyBytes(archive, manifest.artifact);
  if (manifest.artifact.url.split("/").at(-1) !== `${assetBase(manifest.version, manifest.arch)}-portable.zip`) {
    throw new Error("Expected a signed Portable artifact");
  }
  const prefix = `${assetBase(manifest.version, manifest.arch)}-portable/`;
  const allowed = new Set([...shippedFiles, product.portable_marker, "data/"]);
  const seen = new Set<string>();
  const entries = unzipSync(archive, { filter(entry) {
    const relative = entry.name.slice(prefix.length);
    if (!entry.name.startsWith(prefix) || !allowed.has(relative) || seen.has(relative)) {
      throw new Error("Portable archive contains a duplicate or non-shipped path");
    }
    seen.add(relative);
    const expected = manifest.files.find(file => file.path === relative)?.size;
    const maximum = relative === "bundle.json" ? 128 * 1024 : relative === "bundle.json.sig" ? 1024 : expected ?? 0;
    if (entry.originalSize > maximum || (expected !== undefined && entry.originalSize !== expected)) {
      throw new Error("Portable archive entry exceeds signed size");
    }
    return true;
  } });
  const contents = new Map<string, Uint8Array>();
  for (const [path, bytes] of Object.entries(entries)) {
    if (!path.startsWith(prefix)) throw new Error("Portable archive root mismatch");
    const relative = path.slice(prefix.length);
    if (!allowed.has(relative) || contents.has(relative)) throw new Error("Portable archive contains a non-shipped path");
    if ((relative === product.portable_marker || relative === "data/") && bytes.length) throw new Error("Portable archive contains user data");
    contents.set(relative, bytes);
  }
  for (const path of allowed) if (!contents.has(path)) throw new Error(`Missing Portable file: ${path}`);
  const bundle = verifyManifest(contents.get("bundle.json")!, new TextDecoder().decode(contents.get("bundle.json.sig")!),
    manifest.arch, manifest.version, key);
  const app = bundle.files.find(file => file.path === product.binary_name)!;
  if (bundle.artifact.url.split("/").at(-1) !== product.binary_name ||
      bundle.artifact.size !== app.size || bundle.artifact.sha256 !== app.sha256) throw new Error("Portable bundle descriptor mismatch");
  for (const file of bundle.files) verifyBytes(contents.get(file.path)!, file);
  for (const file of manifest.files) verifyBytes(contents.get(file.path)!, file);
  const target = `${manifest.arch}-pc-windows-msvc`;
  for (const path of [product.binary_name, "resources/seed/sing-box.exe", "resources/seed/wintun.dll"]) {
    verifyPe(contents.get(path)!, target);
  }
  const core = bundle.files.find(file => file.path === "resources/seed/sing-box.exe")!;
  const image = contents.get(product.binary_name)!;
  verifyManagedSeed(Buffer.from(image.buffer, image.byteOffset, image.byteLength),
    contents.get("resources/seed/manifest.json")!, core.sha256, targetInfo(target).seed);
  return contents;
}
export function upgradePayload(root: string, manifestPath: string, archivePath: string, arch: Arch) {
  requireNativeWindows();
  manifestPath = localWindowsPath(manifestPath);
  archivePath = localWindowsPath(archivePath);
  const next = verifyArtifact(manifestPath, archivePath, arch);
  const entries = portableEntries(readFileSync(archivePath), next);
  const bundle = verifyManifest(entries.get("bundle.json")!, new TextDecoder().decode(entries.get("bundle.json.sig")!),
    arch, next.version);
  replacePayload(root, entries, bundle);
}
export function recoverPayload(root: string) {
  recoverNativePayload(root);
}
export type VerifyCommand =
  | { command: "verify"; arch: Arch; manifest: string; artifact: string }
  | { command: "upgrade"; arch: Arch; manifest: string; artifact: string; root: string }
  | { command: "recover"; root: string };
export function parseCommand(argv: string[]): VerifyCommand {
  const [command, ...args] = argv;
  const usage = "Usage: bun verify.ts verify ARCH MANIFEST ARTIFACT | upgrade ARCH MANIFEST ZIP ROOT | recover ROOT";
  if (command === "recover" && args.length === 1) return { command, root: args[0] };
  if ((command !== "verify" || args.length !== 3) && (command !== "upgrade" || args.length !== 4)) throw new Error(usage);
  const [arch, manifest, artifact, root] = args;
  if (arch !== "x86_64" && arch !== "aarch64") throw new Error("Expected x86_64 or aarch64");
  return command === "verify" ? { command, arch, manifest, artifact } : { command, arch, manifest, artifact, root };
}
if (import.meta.main) {
  const args = parseCommand(process.argv.slice(2));
  if (args.command === "recover") recoverPayload(args.root);
  else if (args.command === "verify") console.log(verifyArtifact(args.manifest, args.artifact, args.arch));
  else upgradePayload(args.root, args.manifest, args.artifact, args.arch);
}
