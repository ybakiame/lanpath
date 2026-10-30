import type { Arch } from "./protocol";

export const fixtureSourceDigest = "d1".repeat(32);
const align = (value: number) => Math.ceil(value / 4) * 4;
function versionBlock(key: string, type: number, value: Buffer, children: Buffer[] = []): Buffer {
  const name = Buffer.from(`${key}\0`, "utf16le");
  const start = align(6 + name.length);
  const childStart = align(start + value.length);
  const length = children.length ? childStart + children.reduce((size, child, index) => size + (index === children.length - 1 ? child.length : align(child.length)), 0) : start + value.length;
  const result = Buffer.alloc(length);
  result.writeUInt16LE(length);
  result.writeUInt16LE(type ? value.length / 2 : value.length, 2);
  result.writeUInt16LE(type, 4);
  name.copy(result, 6);
  value.copy(result, start);
  let offset = childStart;
  for (const child of children) { child.copy(result, offset); offset += align(child.length); }
  return result;
}

/** Structural PE fixtures for parsers only; never loaded or presented as runnable applications. */
export function pe(arch: Arch, source?: string): Buffer {
  let resource: Buffer | undefined;
  if (source !== undefined) {
    const binding = versionBlock("LanpathManagedCoreSource", 1, Buffer.from(`${source}\0`, "utf16le"));
    const table = versionBlock("040904b0", 1, Buffer.alloc(0), [binding]);
    const strings = versionBlock("StringFileInfo", 1, Buffer.alloc(0), [table]);
    const fixed = Buffer.alloc(52); fixed.writeUInt32LE(0xfeef04bd);
    const version = versionBlock("VS_VERSION_INFO", 0, fixed, [strings]);
    resource = Buffer.alloc(88 + version.length);
    resource.writeUInt16LE(1, 14);
    resource.writeUInt32LE(16, 16);
    resource.writeUInt32LE(0x80000000 + 24, 20);
    resource.writeUInt16LE(1, 24 + 14);
    resource.writeUInt32LE(1, 40);
    resource.writeUInt32LE(0x80000000 + 48, 44);
    resource.writeUInt16LE(1, 48 + 14);
    resource.writeUInt32LE(0x409, 64);
    resource.writeUInt32LE(72, 68);
    resource.writeUInt32LE(0x1000 + 88, 72);
    resource.writeUInt32LE(version.length, 76);
    version.copy(resource, 88);
  }
  const result = Buffer.alloc(resource ? 512 + resource.length : 128);
  result.writeUInt16LE(0x5a4d, 0);
  result.writeUInt32LE(64, 0x3c);
  result.writeUInt32LE(0x4550, 64);
  result.writeUInt16LE(arch === "x86_64" ? 0x8664 : 0xaa64, 68);
  if (resource) {
    result.writeUInt16LE(1, 70);
    result.writeUInt16LE(240, 84);
    result.writeUInt16LE(0x20b, 88);
    result.writeUInt32LE(3, 88 + 108);
    result.writeUInt32LE(0x1000, 88 + 128);
    result.writeUInt32LE(resource.length, 88 + 132);
    result.writeUInt32LE(0x1000, 328 + 12);
    result.writeUInt32LE(resource.length, 328 + 16);
    result.writeUInt32LE(512, 328 + 20);
    resource.copy(result, 512);
  }
  return result;
}
