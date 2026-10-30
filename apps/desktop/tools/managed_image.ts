import { uniqueResource } from "./pe_resources";
import { semanticVersion } from "./protocol";

interface Block { key: string; type: number; value: Buffer; children: number; end: number }
/** Read the build.rs VERSIONINFO binding, not an executable version probe or byte search. */
export function verifyManagedImageSource(image: Buffer, expected: string): void {
  if (!/^[a-f0-9]{64}$/.test(expected)) throw new Error("Invalid managed source digest");
  const data = uniqueResource(image, 16, 1, 128 * 1024);
  const invalid = () => new Error("Invalid or ambiguous compiled managed-core VERSIONINFO binding");
  const align = (offset: number) => Math.ceil(offset / 4) * 4;
  function block(offset: number, limit: number): Block {
    if (offset < 0 || offset > limit - 6) throw invalid();
    const length = data.readUInt16LE(offset);
    const valueLength = data.readUInt16LE(offset + 2);
    const type = data.readUInt16LE(offset + 4);
    const end = offset + length;
    if (length < 8 || end > limit || type > 1) throw invalid();
    let keyEnd = offset + 6;
    while (keyEnd <= end - 2 && data.readUInt16LE(keyEnd) !== 0) keyEnd += 2;
    if (keyEnd > end - 2 || keyEnd - offset > 1024) throw invalid();
    const key = data.toString("utf16le", offset + 6, keyEnd);
    const start = align(keyEnd + 2);
    const valueEnd = start + valueLength * (type === 1 ? 2 : 1);
    if (valueEnd > end) throw invalid();
    return { key, type, value: data.subarray(start, valueEnd), children: align(valueEnd), end };
  }
  function children(parent: Block): Block[] {
    const values: Block[] = [];
    for (let offset = parent.children; offset < parent.end;) {
      if (parent.end - offset < 6) {
        if (data.subarray(offset, parent.end).some(byte => byte !== 0)) throw invalid();
        break;
      }
      const child = block(offset, parent.end);
      values.push(child);
      offset = align(child.end);
    }
    return values;
  }
  const root = block(0, data.length);
  if (root.end !== data.length || root.key !== "VS_VERSION_INFO" || root.type !== 0 || root.value.length !== 52
      || root.value.readUInt32LE(0) !== 0xfeef04bd) throw invalid();
  const stringGroups = children(root).filter(child => child.key === "StringFileInfo");
  if (stringGroups.length !== 1) throw invalid();
  const tables = children(stringGroups[0]!);
  if (!tables.length || tables.length > 32) throw invalid();
  for (const table of tables) {
    if (!/^[a-f0-9]{8}$/i.test(table.key)) throw invalid();
    const bindings = children(table).filter(child => child.key === "LanpathManagedCoreSource");
    if (bindings.length !== 1) throw new Error("Executable predates managed-core source binding; rebuild the GPUI application before packaging");
    const binding = bindings[0]!;
    if (binding.type !== 1 || binding.value.length !== 130 || binding.value.readUInt16LE(128) !== 0
        || binding.value.toString("utf16le", 0, 128) !== expected) {
      throw new Error("Executable and managed core were built from different source/patch inventories; rebuild before signing");
    }
  }
}

/** Consumers bind the signed seed to its own image, including older valid releases. */
export function verifyManagedSeed(image: Buffer, metadata: Uint8Array, coreSha256: string, arch: string): void {
  const seed = JSON.parse(new TextDecoder("utf8", { fatal: true }).decode(metadata));
  if (!seed || typeof seed !== "object" || Array.isArray(seed)
      || seed.schema !== 1 || seed.arch !== arch || seed.managed_protocol !== 1
      || typeof seed.sing_box_version !== "string" || !semanticVersion.test(seed.sing_box_version)
      || typeof seed.upstream_version !== "string" || !semanticVersion.test(seed.upstream_version)
      || typeof seed.go_version !== "string" || !/^go\d+\.\d+\.\d+$/.test(seed.go_version)
      || typeof seed.managed_source_digest !== "string" || !/^[a-f0-9]{64}$/.test(seed.managed_source_digest)
      || typeof seed.core_sha256 !== "string" || seed.core_sha256.toLowerCase() !== coreSha256.toLowerCase()) {
    throw new Error("Managed seed source/protocol/compiler/architecture/digest identity mismatch");
  }
  verifyManagedImageSource(image, seed.managed_source_digest);
}
