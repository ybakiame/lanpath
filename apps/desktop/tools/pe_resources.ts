/** Extract one numeric resource without loading or executing the image. */
export function uniqueResource(data: Buffer, type: number, id: number, maximum: number): Buffer {
  const invalid = () => new Error(`PE must contain exactly one bounded resource type ${type}, id ${id}, language`);
  function range(offset: number, size: number) {
    if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(size) || offset < 0 || size < 0 || offset > data.length - size) throw invalid();
  }
  const u16 = (offset: number) => { range(offset, 2); return data.readUInt16LE(offset); };
  const u32 = (offset: number) => { range(offset, 4); return data.readUInt32LE(offset); };
  if (u16(0) !== 0x5a4d) throw invalid();
  const pe = u32(0x3c);
  if (u32(pe) !== 0x4550) throw invalid();
  const sections = u16(pe + 6);
  const optional = pe + 24;
  const optionalSize = u16(pe + 20);
  if (!sections || sections > 96 || optionalSize < 136 || u16(optional) !== 0x20b || u32(optional + 108) < 3) throw invalid();
  range(optional, optionalSize);
  const sectionTable = optional + optionalSize;
  range(sectionTable, sections * 40);
  function rvaOffset(rva: number, size: number): number {
    for (let index = 0; index < sections; index++) {
      const section = sectionTable + index * 40;
      const address = u32(section + 12);
      const rawSize = u32(section + 16);
      const delta = rva - address;
      if (delta < 0 || size > rawSize || delta > rawSize - size) continue;
      const offset = u32(section + 20) + delta;
      range(offset, size);
      return offset;
    }
    throw invalid();
  }
  const resourceRva = u32(optional + 128);
  const resourceSize = u32(optional + 132);
  if (!resourceRva || resourceSize < 16) throw invalid();
  const resource = rvaOffset(resourceRva, resourceSize);
  function resourceRange(relative: number, size: number) {
    if (relative < 0 || size < 0 || relative > resourceSize - size) throw invalid();
    return resource + relative;
  }
  function entries(relative: number) {
    const header = resourceRange(relative, 16);
    const count = u16(header + 12) + u16(header + 14);
    if (count > 8192) throw invalid();
    return { start: resourceRange(relative + 16, count * 8), count };
  }
  const types = entries(0);
  let directory: number | undefined;
  for (let index = 0; index < types.count; index++) {
    const entry = types.start + index * 8;
    if (u32(entry) !== type) continue;
    const child = u32(entry + 4);
    if (directory !== undefined || !(child & 0x80000000)) throw invalid();
    directory = child & 0x7fffffff;
  }
  if (directory === undefined) throw invalid();
  const names = entries(directory);
  if (names.count !== 1 || u32(names.start) !== id) throw invalid();
  const nameChild = u32(names.start + 4);
  if (!(nameChild & 0x80000000)) throw invalid();
  const languages = entries(nameChild & 0x7fffffff);
  if (languages.count !== 1 || u32(languages.start) > 0xffff) throw invalid();
  const leaf = u32(languages.start + 4);
  if (leaf & 0x80000000) throw invalid();
  const descriptor = resourceRange(leaf, 16);
  const length = u32(descriptor + 4);
  if (!length || length > maximum) throw invalid();
  const offset = rvaOffset(u32(descriptor), length);
  return data.subarray(offset, offset + length);
}
