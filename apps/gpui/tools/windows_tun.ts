import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { unzipSync } from "fflate";
import manifest from "./windows-tun.json";

type Arch = "amd64" | "arm64";

async function downloadVerified(url: string, sha256: string, label: string): Promise<Uint8Array> {
  let bytes: Uint8Array | undefined;
  let lastError: unknown;
  for (let attempt = 1; attempt <= 3 && !bytes; attempt++) {
    try {
      const response = await fetch(url, { headers: { "User-Agent": "lanpath" } });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      bytes = new Uint8Array(await response.arrayBuffer());
    } catch (error) {
      lastError = error;
      if (attempt < 3) await Bun.sleep(attempt * 3000);
    }
  }
  if (!bytes) throw new Error(`${label} download failed: ${String(lastError)}`);
  if (createHash("sha256").update(bytes).digest("hex") !== sha256) {
    throw new Error(`${label} SHA256 mismatch`);
  }
  return bytes;
}

function findEntry(entries: Record<string, Uint8Array>, suffix: string): Uint8Array {
  const entry = Object.entries(entries).find(([name, data]) => name.endsWith(suffix) && data.length > 0);
  if (!entry) throw new Error(`Missing archive entry: ${suffix}`);
  return entry[1];
}

/** Pinned compatibility DLL and licenses used by the independent GPUI bundle. */
export async function fetchWindowsTunResources(arch: Arch, output: string): Promise<void> {
  if (arch !== "amd64" && arch !== "arm64") throw new Error(`Unsupported seed architecture: ${arch}`);
  const licenses = join(output, "licenses");
  mkdirSync(licenses, { recursive: true });
  const wt = manifest.wintun;
  const entries = unzipSync(await downloadVerified(wt.url, wt.sha256, `wintun ${wt.version}`));
  writeFileSync(join(output, "wintun.dll"), findEntry(entries, wt.dll_path[arch]));
  writeFileSync(join(licenses, "wintun-LICENSE.txt"), findEntry(entries, "/LICENSE.txt"));
  writeFileSync(join(licenses, "gpl-3.0.txt"), await downloadVerified(manifest.gpl3.url, manifest.gpl3.sha256, "GPL-3.0"));
}
