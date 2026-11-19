import { createHash } from "node:crypto";
import { copyFileSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { unzipSync } from "fflate";
import { fetchWindowsTunResources } from "./windows_tun";
import { desktop, semanticVersion, sha256, targetInfo, verifyPe } from "./protocol";

interface ModulePin { path: string; version: string; commit: string; sum: string; zip_sha256: string }
interface SourceSpec {
  schema: number; version: string; upstream_version: string; managed_protocol: number;
  go_version: string; tags: string[]; managed_files: string[];
  modules: { core: ModulePin; tun: ModulePin; pipe: ModulePin };
}
const sourceRoot = join(desktop, "managed-core");
const sourceFile = join(sourceRoot, "source.json");
const sourceMetadata = lstatSync(sourceFile);
if (!sourceMetadata.isFile() || sourceMetadata.isSymbolicLink() || sourceMetadata.nlink !== 1 || sourceMetadata.size > 128 * 1024) {
  throw new Error("Managed source identity must be a bounded ordinary unshared file");
}
const sourceBytes = readFileSync(sourceFile);
export const managedSpec: SourceSpec = JSON.parse(sourceBytes.toString("utf8"));
if (managedSpec.schema !== 1 || managedSpec.managed_protocol !== 1 || !semanticVersion.test(managedSpec.version)
    || !semanticVersion.test(managedSpec.upstream_version) || !/^go\d+\.\d+\.\d+$/.test(managedSpec.go_version)
    || !Array.isArray(managedSpec.tags) || managedSpec.tags.some(tag => !/^[a-z_]+$/.test(tag))
    || !Array.isArray(managedSpec.managed_files) || new Set(managedSpec.managed_files).size !== managedSpec.managed_files.length
    || managedSpec.managed_files.some(path => !/^(?:core|tun)\/[a-z_]+\.go$|^patches\/[a-z_]+\.patch$/.test(path))) {
  throw new Error("Invalid managed core source identity");
}

export function managedSourceDigest(): string {
  const hash = createHash("sha256");
  for (const path of ["source.json", ...managedSpec.managed_files].sort()) {
    const file = join(sourceRoot, path);
    const metadata = lstatSync(file);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.nlink !== 1 || metadata.size > 128 * 1024) {
      throw new Error("Managed core inputs must be bounded ordinary unshared files");
    }
    const bytes = readFileSync(file);
    if (path === "source.json" && !bytes.equals(sourceBytes)) throw new Error("Managed source identity changed after initialization");
    hash.update(path).update("\0").update(bytes).update("\0");
  }
  return hash.digest("hex");
}

function buildEnvironment(arch: string): NodeJS.ProcessEnv {
  // Do not pass signing identities, CI secrets or Go user configuration to builds.
  const environment: NodeJS.ProcessEnv = {};
  for (const key of ["PATH", "HOME", "TMPDIR", "TEMP", "TMP", "GOROOT", "GOPATH", "GOCACHE", "GOMODCACHE",
    "SSL_CERT_FILE", "SSL_CERT_DIR", "NIX_SSL_CERT_FILE", "http_proxy", "https_proxy", "all_proxy", "no_proxy",
    "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY", "SystemRoot", "WINDIR"]) {
    if (process.env[key] !== undefined) environment[key] = process.env[key];
  }
  return { ...environment, GOENV: "off", GOWORK: "off", GOTOOLCHAIN: "local", GOFLAGS: "", GOEXPERIMENT: "",
    GOPROXY: "https://proxy.golang.org,direct", GOSUMDB: "sum.golang.org", GONOSUMDB: "", GONOPROXY: "", GOPRIVATE: "", GOINSECURE: "",
    CGO_ENABLED: "0", GOOS: "windows", GOARCH: arch, GOAMD64: "v1", GOARM64: "v8.0" };
}

function run(executable: string, args: string[], cwd: string, env: NodeJS.ProcessEnv, inherit = false): string {
  const result = spawnSync(executable, args, { cwd, env, encoding: "utf8", stdio: inherit ? "inherit" : "pipe", maxBuffer: 8 * 1024 * 1024 });
  if (result.error || result.status !== 0) throw new Error(`Managed core command failed (${result.status ?? "launch"}): ${result.stderr || result.stdout || result.error?.message || executable}`);
  return result.stdout ?? "";
}

function downloadModule(pin: ModulePin, scratch: string, env: NodeJS.ProcessEnv): string {
  if (!/^github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(pin.path) || !/^[a-f0-9]{40}$/.test(pin.commit)
      || !/^v[0-9A-Za-z.+-]+$/.test(pin.version) || !/^h1:[A-Za-z0-9+/]{43}=$/.test(pin.sum) || !/^[a-f0-9]{64}$/.test(pin.zip_sha256)) {
    throw new Error("Invalid Go module source pin");
  }
  const module = JSON.parse(run("go", ["mod", "download", "-json", `${pin.path}@${pin.version}`], scratch, env));
  if (module.Path !== pin.path || module.Version !== pin.version || module.Sum !== pin.sum
      || (module.Origin?.Hash !== undefined && module.Origin.Hash !== pin.commit) || typeof module.Zip !== "string") {
    throw new Error("Go module origin/checksum differs from the managed source pin");
  }
  const metadata = lstatSync(module.Zip);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.nlink !== 1 || metadata.size > 64 * 1024 * 1024) {
    throw new Error("Managed source ZIP is not a bounded ordinary unshared file");
  }
  if (sha256(readFileSync(module.Zip)) !== pin.zip_sha256) throw new Error("Managed source ZIP digest mismatch");
  return module.Zip;
}

function extractModule(archive: string, pin: ModulePin, destination: string): void {
  const prefix = `${pin.path}@${pin.version}/`;
  const names = new Set<string>();
  let expanded = 0;
  const entries = unzipSync(readFileSync(archive), { filter(entry) {
    const name = entry.name.slice(prefix.length);
    if (!entry.name.startsWith(prefix) || !name || name.startsWith("/") || name.includes("\\") || name.includes("\0")
        || name.includes(":") || name.split("/").some(part => part === "." || part === "..") || names.has(name)) {
      throw new Error("Unsafe or duplicate managed source ZIP path");
    }
    names.add(name);
    expanded += entry.originalSize;
    if (names.size > 16_384 || expanded > 128 * 1024 * 1024) throw new Error("Managed source expansion exceeds bound");
    return !entry.name.endsWith("/");
  } });
  mkdirSync(destination, { recursive: true });
  for (const [path, bytes] of Object.entries(entries)) {
    const output = join(destination, path.slice(prefix.length));
    mkdirSync(dirname(output), { recursive: true });
    writeFileSync(output, bytes, { flag: "wx" });
  }
}

/** Build the real Windows managed core from authenticated source, never a stock fallback. */
export async function buildManagedSeed(target: string, output?: string): Promise<string> {
  const info = targetInfo(target);
  const destination = resolve(output ?? join(desktop, "target", target, "managed-core", "resources", "seed"));
  const digest = managedSourceDigest();
  const env = buildEnvironment(info.seed);
  const scratch = mkdtempSync(join(tmpdir(), "lanpath-managed-core-"));
  try {
    if (run("go", ["env", "GOVERSION"], scratch, env).trim() !== managedSpec.go_version) {
      throw new Error(`Managed core requires the repository's pinned ${managedSpec.go_version}`);
    }
    const archives = new Map<string, string>();
    for (const pin of Object.values(managedSpec.modules)) archives.set(pin.path, downloadModule(pin, scratch, env));
    const core = join(scratch, "core");
    const tun = join(core, "lanpath-sing-tun");
    extractModule(archives.get(managedSpec.modules.core.path)!, managedSpec.modules.core, core);
    extractModule(archives.get(managedSpec.modules.tun.path)!, managedSpec.modules.tun, tun);
    for (const [root, patch] of [[core, "core.patch"], [tun, "tun.patch"]]) {
      const input = join(sourceRoot, "patches", patch!);
      run("git", ["apply", "--check", input], root!, env);
      run("git", ["apply", input], root!, env);
    }
    for (const path of managedSpec.managed_files.filter(path => path.endsWith(".go"))) {
      const [kind, name] = path.split("/");
      copyFileSync(join(sourceRoot, path), join(kind === "core" ? join(core, "cmd", "sing-box") : tun, name!));
    }
    run("go", ["mod", "edit", `-replace=${managedSpec.modules.tun.path}=./lanpath-sing-tun`], core, env);
    const image = join(scratch, "sing-box.exe");
    run("go", ["build", "-mod=readonly", "-trimpath", "-buildvcs=false", "-tags", managedSpec.tags.join(","),
      "-ldflags", `-s -w -buildid= -X github.com/sagernet/sing-box/constant.Version=${managedSpec.version}`,
      "-o", image, "./cmd/sing-box"], core, env, true);
    verifyPe(readFileSync(image), target);
    if (managedSourceDigest() !== digest) throw new Error("Managed source inputs changed during compilation");
    mkdirSync(join(destination, "licenses"), { recursive: true });
    await fetchWindowsTunResources(info.seed, destination);
    copyFileSync(image, join(destination, "sing-box.exe"));
    copyFileSync(join(core, "LICENSE"), join(destination, "licenses", "sing-box-LICENSE.txt"));
    const manifest = { schema: 1, sing_box_version: managedSpec.version, upstream_version: managedSpec.upstream_version,
      managed_protocol: managedSpec.managed_protocol, managed_source_digest: digest, go_version: managedSpec.go_version,
      arch: info.seed, core_sha256: sha256(readFileSync(image)) };
    writeFileSync(join(destination, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`Managed core built: ${managedSpec.version} / ${info.seed} / ${manifest.core_sha256}`);
    return destination;
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  const [target, output] = process.argv.slice(2);
  if (!target) throw new Error("Usage: bun apps/gpui/tools/managed_core.ts <Windows target> [output directory]");
  await buildManagedSeed(target, output);
}
