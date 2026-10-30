import { test, expect } from "bun:test";
import { generateKeyPairSync, sign } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { zipSync } from "fflate";
import { assetBase, digest, product, publicKey, requiredFiles, shippedFiles, signedManifest, targets, verifyBytes, verifyManifest, verifyPe, version, type Arch, type Manifest } from "./protocol";
import { stage, verifyStage } from "./package";
import { parseCommand, portableEntries, verifyArtifact } from "./verify";
import { fixtureSourceDigest, pe } from "./protocol.fixtures";
import { verifyManagedImageSource } from "./managed_image";

// Test-only generated keys never use or read the independently provisioned product private key.
const signer = generateKeyPairSync("ed25519");
const pem = signer.privateKey.export({ type: "pkcs8", format: "pem" }).toString();
const signed = (manifest: Manifest) => signedManifest(manifest, pem, signer.publicKey);
function fixture(release = version, arch: Arch = "x86_64") {
  const bytes: Record<string, Uint8Array> = {};
  for (const path of requiredFiles) bytes[path] = Buffer.from(`license ${path} ${release}`);
  for (const path of [product.binary_name, "resources/seed/sing-box.exe", "resources/seed/wintun.dll"]) bytes[path] = pe(arch);
  bytes[product.binary_name] = pe(arch, fixtureSourceDigest);
  const core = digest("resources/seed/sing-box.exe", bytes["resources/seed/sing-box.exe"]!);
  bytes["resources/seed/manifest.json"] = Buffer.from(JSON.stringify({
    schema: 1, sing_box_version: "1.14.2-lanpath.1", upstream_version: "1.14.2",
    managed_protocol: 1, managed_source_digest: fixtureSourceDigest, go_version: "go1.26.8",
    arch: arch === "x86_64" ? "amd64" : "arm64", core_sha256: core.sha256,
  }));
  const files = requiredFiles.map(path => path === core.path ? core : digest(path, bytes[path]));
  const app = files.find(file => file.path === product.binary_name)!;
  const manifest: Manifest = {
    schema: 1, product: product.app_id, channel: product.update_channel, version: release, arch,
    artifact: { url: `${product.repository}/releases/download/gpui-v${release}/${product.binary_name}`, size: app.size, sha256: app.sha256 }, files,
  };
  const bundle = signed(manifest);
  bytes["bundle.json"] = bundle.payload;
  bytes["bundle.json.sig"] = Buffer.from(bundle.signature);
  return { manifest, bytes };
}
function archiveFixture(release = version, arch: Arch = "x86_64", change?: (entries: Record<string, Uint8Array>) => void) {
  const { manifest: bundle, bytes } = fixture(release, arch);
  const prefix = `${assetBase(release, arch)}-portable/`;
  const entries = Object.fromEntries(Object.entries(bytes).map(([path, data]) => [`${prefix}${path}`, data]));
  entries[`${prefix}${product.portable_marker}`] = new Uint8Array();
  entries[`${prefix}data/`] = new Uint8Array();
  change?.(entries);
  const archive = zipSync(entries);
  const hash = digest("archive", archive);
  const manifest: Manifest = { ...bundle, artifact: { url: `${product.repository}/releases/download/gpui-v${release}/${assetBase(release, arch)}-portable.zip`, size: hash.size, sha256: hash.sha256 } };
  return { archive, manifest, entries };
}
function writeTree(root: string, entries: Record<string, Uint8Array>) {
  for (const [path, bytes] of Object.entries(entries)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), bytes);
  }
}
function temporary(run: (root: string) => void) {
  const root = mkdtempSync(join(tmpdir(), "lanpath-gpui-package-"));
  try { run(root); } finally { rmSync(root, { recursive: true, force: true }); }
}

test("PE architecture verification rejects a binary for the other Windows target", () => {
  for (const [target, info] of Object.entries(targets)) {
    expect(() => verifyPe(pe(info.arch), target)).not.toThrow();
    expect(() => verifyPe(pe(info.arch === "x86_64" ? "aarch64" : "x86_64"), target)).toThrow("architecture mismatch");
  }
});

test("compiled source binding rejects a different inventory, missing metadata and ambiguous resource languages", () => {
  const image = pe("x86_64", fixtureSourceDigest);
  verifyManagedImageSource(image, fixtureSourceDigest);
  expect(() => verifyManagedImageSource(image, "e2".repeat(32))).toThrow();
  expect(() => verifyManagedImageSource(pe("x86_64"), fixtureSourceDigest)).toThrow();
  // The language table is structurally parsed, not found by scanning hash bytes.
  image.writeUInt16LE(2, 512 + 48 + 14);
  expect(() => verifyManagedImageSource(image, fixtureSourceDigest)).toThrow();
});

test("signature covers exact JSON bytes and rejects the wrong product signing key", () => {
  const { manifest } = fixture();
  const descriptor = signed(manifest);
  expect(verifyManifest(descriptor.payload, descriptor.signature, manifest.arch, version, signer.publicKey)).toEqual(manifest);
  expect(() => verifyManifest(Buffer.concat([descriptor.payload, Buffer.from(" ")]), descriptor.signature, manifest.arch, version, signer.publicKey)).toThrow("signature");
  expect(() => verifyManifest(descriptor.payload, descriptor.signature, manifest.arch)).toThrow("signature");
  expect(() => signedManifest(manifest, pem)).toThrow("does not match");
  expect(() => publicKey("placeholder")).toThrow();
});

test("even correctly signed descriptors reject Tauri identity, wrong channel, architecture, namespace and incomplete allowlists", () => {
  const original = fixture().manifest;
  const variants: Manifest[] = [
    { ...original, product: "com.ybakiame.lanpath" },
    { ...original, channel: "stable" },
    { ...original, arch: "aarch64" },
    { ...original, version: "01.0.0" },
    { ...original, artifact: { ...original.artifact, url: `${product.repository}/releases/download/v${version}/lanpath.exe` } },
    { ...original, artifact: { ...original.artifact, url: `${original.artifact.url}?redirect=evil` } },
    { ...original, artifact: { ...original.artifact, url: `${product.repository}/releases/download/gpui-v${version}/${assetBase(version, "x86_64")}-setup.exe` } },
    { ...original, files: original.files.slice(1) },
    { ...original, files: [original.files[0], ...original.files.slice(0, -1)] },
    { ...original, files: original.files.map((file, i) => i ? file : { ...file, path: "../lanpath-gpui.exe" }) },
    { ...original, files: original.files.map((file, i) => i ? file : { ...file, sha256: "0".repeat(63) }) },
    { ...original, files: original.files.map((file, i) => i ? file : { ...file, size: 0 }) },
    { ...original, unexpected: true } as Manifest,
  ];
  for (const manifest of variants) {
    const descriptor = signed(manifest);
    expect(() => verifyManifest(descriptor.payload, descriptor.signature, "x86_64", undefined, signer.publicKey)).toThrow();
  }
  const payload = Buffer.alloc(128 * 1024 + 1);
  const signature = sign(null, payload, signer.privateKey).toString("base64");
  expect(() => verifyManifest(payload, signature, "x86_64", undefined, signer.publicKey)).toThrow("size limit");
});

test("stage verifies all allowlisted payloads, licenses and PE identities and rejects accidental user-data shipment", () => temporary(root => {
  const { bytes } = fixture();
  writeTree(root, bytes);
  expect(verifyStage("x86_64-pc-windows-msvc", root, signer.publicKey).files).toHaveLength(requiredFiles.length);
  writeFileSync(join(root, "data-private.json"), "must not ship");
  expect(() => verifyStage("x86_64-pc-windows-msvc", root, signer.publicKey)).toThrow("Non-shipped");
  rmSync(join(root, "data-private.json"));
  writeFileSync(join(root, "resources/seed/wintun.dll"), pe("aarch64"));
  expect(() => verifyStage("x86_64-pc-windows-msvc", root, signer.publicKey)).toThrow("integrity");
}));

test("packaging fails closed without an independent signing key before fetching or changing output", async () => {
  const previous = process.env.GPUI_SIGNING_PRIVATE_KEY_FILE;
  delete process.env.GPUI_SIGNING_PRIVATE_KEY_FILE;
  try { await expect(stage("x86_64-pc-windows-msvc")).rejects.toThrow("GPUI_SIGNING_PRIVATE_KEY_FILE"); }
  finally {
    if (previous !== undefined) process.env.GPUI_SIGNING_PRIVATE_KEY_FILE = previous;
  }
});

test("authenticated Portable artifacts include only signed payload plus empty marker and data directory", () => {
  for (const arch of ["x86_64", "aarch64"] as const) {
    const { archive, manifest } = archiveFixture(version, arch);
    const contents = portableEntries(archive, manifest, signer.publicKey);
    expect(contents.size).toBe(shippedFiles.length + 2);
    expect(contents.get(product.portable_marker)).toHaveLength(0);
    const corrupt = archive.slice(); corrupt[10] ^= 1;
    expect(() => portableEntries(corrupt, manifest, signer.publicKey)).toThrow("integrity");
    expect(() => verifyBytes(Buffer.from("tampered"), manifest.artifact)).toThrow("integrity");
  }
});

test("Portable parser rejects signed archives containing traversal, extra data, missing licenses or resource tampering", () => {
  for (const mutate of [
    (entries: Record<string, Uint8Array>) => { entries["../outside"] = Buffer.from("escape"); },
    (entries: Record<string, Uint8Array>) => { entries[`${assetBase(version, "x86_64")}-portable/data/private.json`] = Buffer.from("private"); },
    (entries: Record<string, Uint8Array>) => { delete entries[`${assetBase(version, "x86_64")}-portable/LICENSE.txt`]; },
    (entries: Record<string, Uint8Array>) => { entries[`${assetBase(version, "x86_64")}-portable/resources/seed/wintun.dll`] = pe("aarch64"); },
    (entries: Record<string, Uint8Array>) => { entries[`${assetBase(version, "x86_64")}-portable/${product.portable_marker}`] = Buffer.from("not empty"); },
    (entries: Record<string, Uint8Array>) => { delete entries[`${assetBase(version, "x86_64")}-portable/data/`]; },
  ]) {
    const { archive, manifest } = archiveFixture(version, "x86_64", mutate);
    expect(() => portableEntries(archive, manifest, signer.publicKey)).toThrow();
  }
});

test("duplicate ZIP names are rejected before decompression even when the entire ZIP is signed", () => {
  const duplicateName = `${assetBase(version, "x86_64")}-portable/LICENSe.txt`;
  const goodName = duplicateName.replace("LICENSe", "LICENSE");
  const fixture = archiveFixture(version, "x86_64", entries => { entries[duplicateName] = Buffer.from("duplicate"); });
  const archive = Buffer.from(fixture.archive);
  const needle = Buffer.from(duplicateName), replacement = Buffer.from(goodName);
  for (let position = archive.indexOf(needle); position !== -1; position = archive.indexOf(needle, position + needle.length)) replacement.copy(archive, position);
  const hash = digest("duplicate", archive);
  const manifest = { ...fixture.manifest, artifact: { ...fixture.manifest.artifact, size: hash.size, sha256: hash.sha256 } };
  expect(() => portableEntries(archive, manifest, signer.publicKey)).toThrow("duplicate");
});

test("offline artifact verification authenticates bytes, Portable contents and exact release filename", () => temporary(root => {
  const next = archiveFixture("0.0.2");
  const descriptor = signed(next.manifest);
  const manifestPath = join(root, "gpui-x86_64.json");
  const archivePath = join(root, `${assetBase("0.0.2", "x86_64")}-portable.zip`);
  writeFileSync(manifestPath, descriptor.payload);
  writeFileSync(`${manifestPath}.sig`, descriptor.signature);
  writeFileSync(archivePath, next.archive);
  expect(verifyArtifact(manifestPath, archivePath, "x86_64", signer.publicKey).version).toBe("0.0.2");
  const wrongName = join(root, "wrong-portable.zip");
  writeFileSync(wrongName, next.archive);
  expect(() => verifyArtifact(manifestPath, wrongName, "x86_64", signer.publicKey)).toThrow("filename");
  const alternate = join(root, "gpui-x86_64-portable.json");
  writeFileSync(alternate, descriptor.payload);
  writeFileSync(`${alternate}.sig`, descriptor.signature);
  expect(() => verifyArtifact(alternate, archivePath, "x86_64", signer.publicKey)).toThrow("filename");
  writeFileSync(archivePath, "tampered archive");
  expect(() => verifyArtifact(manifestPath, archivePath, "x86_64", signer.publicKey)).toThrow("integrity");
}));

test("staging never follows a resource symlink outside the allowlist", () => {
  if (process.platform === "win32") return; // Creating Unix symlinks has no Windows privilege prerequisite.
  temporary(root => {
    const stage = join(root, "stage"), outside = join(root, "outside");
    writeTree(stage, fixture().bytes);
    writeFileSync(outside, "private");
    rmSync(join(stage, "LICENSE.txt"));
    symlinkSync(outside, join(stage, "LICENSE.txt"));
    expect(() => verifyStage("x86_64-pc-windows-msvc", stage, signer.publicKey)).toThrow("Non-shipped");
    expect(readFileSync(outside, "utf8")).toBe("private");
  });
});

test("generic verification CLI accepts only exact public commands and argument counts", () => {
  expect(parseCommand(["verify", "x86_64", "gpui-x86_64.json", "payload.zip"])).toEqual({
    command: "verify", arch: "x86_64", manifest: "gpui-x86_64.json", artifact: "payload.zip",
  });
  expect(parseCommand(["upgrade", "aarch64", "gpui-aarch64.json", "payload.zip", "C:\\GPUI"])).toEqual({
    command: "upgrade", arch: "aarch64", manifest: "gpui-aarch64.json", artifact: "payload.zip", root: "C:\\GPUI",
  });
  expect(parseCommand(["recover", "C:\\GPUI"])).toEqual({ command: "recover", root: "C:\\GPUI" });
  for (const args of [
    [], ["install"], ["upgrade-portable", "x86_64", "manifest", "archive", "root"],
    ["recover-portable", "root"], ["recover", "root", "extra"],
    ["upgrade", "x86_64", "manifest", "archive"], ["verify", "x86_64", "manifest", "archive", "extra"],
    ["verify", "arm64", "manifest", "archive"],
  ]) expect(() => parseCommand(args)).toThrow();
});

