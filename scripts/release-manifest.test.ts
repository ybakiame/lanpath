import { test, expect } from "bun:test";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { releaseManifest } from "./release-manifest";

for (const version of ["0.1.1", "0.1.1-beta.1"]) {
  test(`release manifest isolates channel ${version} and checks complete signatures`, () => {
    const directory = mkdtempSync(join(tmpdir(), "lanpath-release-"));
    try {
      const add = (arch: string) => {
        const archive = `Lanpath_${version}_${arch}-setup.nsis.zip`;
        writeFileSync(join(directory, archive), "fixture archive");
        writeFileSync(join(directory, archive + ".sig"), "fixture signature");
      };
      add("x64");
      expect(() => releaseManifest(directory, `v${version}`, "ybakiame/lanpath", version)).toThrow("arm64");
      add("arm64");
      const name = releaseManifest(directory, `v${version}`, "ybakiame/lanpath", version);
      const manifest = JSON.parse(readFileSync(join(directory, name), "utf8"));
      expect(manifest.version).toBe(version);
      expect(manifest.platforms["windows-x86_64"].url).toContain(`/v${version}/`);
      expect(name).toBe(version.includes("-") ? "prerelease.json" : "latest.json");
      if (version.includes("-")) expect(existsSync(join(directory, "latest.json"))).toBe(false);
      expect(readFileSync(join(directory, "SHA256SUMS"), "utf8")).toContain(name);
      expect(() => releaseManifest(directory, "v9.9.9", "ybakiame/lanpath", version)).toThrow("tag");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
}
