import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const directory = process.argv[2] ?? "dist";
const tag = process.env.RELEASE_TAG;
const repository = process.env.RELEASE_REPOSITORY;
if (!tag || !repository) throw new Error("RELEASE_TAG and RELEASE_REPOSITORY are required");
const config = JSON.parse(readFileSync("apps/client/src-tauri/tauri.conf.json", "utf8"));
if (tag !== "v" + config.version) throw new Error("Release tag must match application version");
const files = readdirSync(directory);
const platforms: Record<string, { signature: string; url: string }> = {};
for (const [architecture, key] of [
  ["x64", "windows-x86_64"],
  ["arm64", "windows-aarch64"],
]) {
  const suffix = "_" + architecture + "-setup.nsis.zip";
  const matches = files.filter((file) => file.startsWith("Lanpath_" + config.version + "_") && file.endsWith(suffix));
  if (matches.length !== 1) throw new Error("Expected one signed updater archive for " + architecture);
  const archive = matches[0];
  const signature = readFileSync(join(directory, archive + ".sig"), "utf8").trim();
  if (!signature) throw new Error("Missing updater signature for " + architecture);
  platforms[key] = { signature, url: "https://github.com/" + repository + "/releases/download/" + tag + "/" + archive };
}
if (!tag.includes("-"))
  writeFileSync(
    join(directory, "latest.json"),
    JSON.stringify(
      {
        version: config.version,
        notes: "https://github.com/" + repository + "/releases/tag/" + tag,
        pub_date: new Date().toISOString(),
        platforms,
      },
      null,
      2,
    ) + "\n",
  );
