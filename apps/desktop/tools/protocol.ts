import { createHash, createPrivateKey, createPublicKey, sign, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export const desktop = resolve(import.meta.dir, "..");
export interface Product {
  display_name: string; app_id: string; data_dir_name: string; binary_name: string;
  portable_marker: string; update_channel: string; update_endpoint: string;
  update_public_key: string; repository: string; bundle_files: string[];
}
export const product: Product = JSON.parse(readFileSync(resolve(desktop, "product.json"), "utf8"));
export const semanticVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const packageSection = readFileSync(resolve(desktop, "Cargo.toml"), "utf8").split("[package]")[1]?.split(/^\[/m)[0];
const packageVersion = packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!packageVersion || !semanticVersion.test(packageVersion)) {
  throw new Error("Independent desktop Cargo package must declare a semantic version");
}
export const version: string = packageVersion;
export const targets = {
  "x86_64-pc-windows-msvc": { arch: "x86_64", seed: "amd64", machine: 0x8664 },
  "aarch64-pc-windows-msvc": { arch: "aarch64", seed: "arm64", machine: 0xaa64 },
} as const;
export type Arch = "x86_64" | "aarch64";
export interface FileDigest { path: string; size: number; sha256: string }
export interface Artifact { url: string; size: number; sha256: string }
export interface Manifest {
  schema: 1; product: string; channel: string; version: string; arch: Arch;
  artifact: Artifact; files: FileDigest[];
}
export const requiredFiles: readonly string[] = product.bundle_files;
if (!Array.isArray(requiredFiles) || !requiredFiles.length || !requiredFiles.includes(product.binary_name) ||
    new Set(requiredFiles.map(path => path.toLowerCase())).size !== requiredFiles.length ||
    requiredFiles.some(path => !/^[A-Za-z0-9._/-]+$/.test(path) ||
      path.split("/").some(part => !part || part === "." || part === ".."))) {
  throw new Error("Invalid authoritative product bundle allowlist");
}
export const shippedFiles = [...requiredFiles, "bundle.json", "bundle.json.sig"];
export const assetBase = (release: string, arch: Arch) => `${product.data_dir_name}_${release}_windows_${arch}`;
export const sha256 = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
export function digest(path: string, bytes: Uint8Array): FileDigest {
  return { path, size: bytes.length, sha256: sha256(bytes) };
}
export function targetInfo(target: string) {
  if (!Object.hasOwn(targets, target)) throw new Error(`Unsupported Windows target: ${target}`);
  return targets[target as keyof typeof targets];
}
export function publicKey(raw = product.update_public_key) {
  if (typeof raw !== "string" || !/^[A-Za-z0-9+/]{43}=$/.test(raw)) {
    throw new Error("A genuine independent raw32 base64 GPUI update public key is required");
  }
  if (Buffer.from(raw, "base64").toString("base64") !== raw) throw new Error("Noncanonical GPUI public key");
  return createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(raw, "base64")]),
    format: "der", type: "spki",
  });
}
export function signedManifest(manifest: Manifest, privatePem: string, key = publicKey()) {
  const payload = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  const secret = createPrivateKey(privatePem);
  if (secret.asymmetricKeyType !== "ed25519") throw new Error("GPUI requires an Ed25519 PKCS8 PEM key");
  const signature = sign(null, payload, secret);
  if (!verify(null, payload, key, signature)) throw new Error("Signing key does not match GPUI product public key");
  return { payload, signature: `${signature.toString("base64")}\n` };
}
export function verifyManifest(payload: Uint8Array, signature: string, arch: Arch,
  expectedVersion?: string, key = publicKey()): Manifest {
  if (payload.length > 128 * 1024) throw new Error("GPUI manifest exceeds size limit");
  const detached = signature.trim();
  if (!/^[A-Za-z0-9+/]{86}==$/.test(detached) ||
      Buffer.from(detached, "base64").toString("base64") !== detached ||
      !verify(null, payload, key, Buffer.from(detached, "base64"))) throw new Error("Invalid GPUI signature");
  const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(payload)) as Manifest;
  if (!exactKeys(value, ["schema", "product", "channel", "version", "arch", "artifact", "files"]) ||
      value.schema !== 1 || value.product !== product.app_id || value.channel !== product.update_channel ||
      value.arch !== arch || typeof value.version !== "string" || !semanticVersion.test(value.version) ||
      (expectedVersion !== undefined && value.version !== expectedVersion)) throw new Error("GPUI manifest identity mismatch");
  const artifact = value.artifact;
  if (!exactKeys(artifact, ["url", "size", "sha256"]) || !validDigest(artifact) || typeof artifact.url !== "string") {
    throw new Error("Invalid GPUI artifact digest");
  }
  const url = new URL(artifact.url);
  const base = `${product.repository}/releases/download/gpui-v${value.version}/`;
  const names = [product.binary_name, `${assetBase(value.version, arch)}-portable.zip`];
  if (url.protocol !== "https:" || url.username || url.password || url.hash || url.search ||
      !names.some(name => artifact.url === `${base}${name}`)) {
    throw new Error("Artifact is outside the independent GPUI release namespace");
  }
  if (!Array.isArray(value.files) || value.files.length !== requiredFiles.length) throw new Error("Incomplete trusted GPUI bundle");
  const paths = new Set<string>();
  for (const file of value.files) {
    if (!exactKeys(file, ["path", "size", "sha256"]) || !requiredFiles.includes(file.path) || paths.has(file.path) || !validDigest(file)) {
      throw new Error("Invalid trusted GPUI bundle allowlist");
    }
    paths.add(file.path);
  }
  return value;
}
function exactKeys(value: unknown, keys: string[]): boolean {
  return value !== null && typeof value === "object" && !Array.isArray(value) &&
    Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
}
function validDigest(value: { size: number; sha256: string }) {
  return Number.isSafeInteger(value.size) && value.size > 0 && value.size <= 1024 ** 3 &&
    typeof value.sha256 === "string" && /^[0-9a-f]{64}$/.test(value.sha256);
}
export function verifyBytes(bytes: Uint8Array, expected: { size: number; sha256: string }) {
  if (bytes.length !== expected.size || sha256(bytes) !== expected.sha256) throw new Error("GPUI artifact integrity mismatch");
}
export function verifyPe(bytes: Uint8Array, target: string) {
  const info = targetInfo(target);
  const data = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (data.length < 64 || data.readUInt16LE(0) !== 0x5a4d) throw new Error("Missing Windows PE executable");
  const offset = data.readUInt32LE(0x3c);
  if (offset > data.length - 6 || data.readUInt32LE(offset) !== 0x4550 || data.readUInt16LE(offset + 4) !== info.machine) {
    throw new Error("Windows PE architecture mismatch");
  }
}
