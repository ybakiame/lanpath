import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

/** Decode minisign framing; Ed25519 and BLAKE2b primitives are provided by Node. */
export function verifySignature(data: Uint8Array, publicEncoded: string, signatureEncoded: string): void {
  const publicLines = Buffer.from(publicEncoded, "base64").toString().trim().split(/\r?\n/);
  const signatureLines = Buffer.from(signatureEncoded, "base64").toString().trim().split(/\r?\n/);
  if (publicLines.length !== 2 || signatureLines.length !== 4 || !signatureLines[2].startsWith("trusted comment: ")) {
    throw new Error("Invalid minisign framing");
  }
  const publicPacket = Buffer.from(publicLines[1], "base64");
  const signaturePacket = Buffer.from(signatureLines[1], "base64");
  const globalSignature = Buffer.from(signatureLines[3], "base64");
  if (
    publicPacket.length !== 42 ||
    signaturePacket.length !== 74 ||
    globalSignature.length !== 64 ||
    publicPacket.subarray(0, 2).toString() !== "Ed" ||
    !publicPacket.subarray(2, 10).equals(signaturePacket.subarray(2, 10))
  ) {
    throw new Error("Invalid minisign packet or key identifier");
  }
  const algorithm = signaturePacket.subarray(0, 2).toString();
  if (algorithm !== "Ed" && algorithm !== "ED") throw new Error("Unsupported minisign algorithm");
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), publicPacket.subarray(10)]),
    format: "der",
    type: "spki",
  });
  const signature = signaturePacket.subarray(10);
  const payload = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, payload, key, signature)) throw new Error("Invalid artifact signature");
  const trusted = Buffer.from(signatureLines[2].slice("trusted comment: ".length));
  if (!verify(null, Buffer.concat([signature, trusted]), key, globalSignature)) {
    throw new Error("Invalid trusted comment signature");
  }
}

if (import.meta.main) {
  const directory = process.argv[2] ?? "dist";
  const config = JSON.parse(readFileSync("apps/client/src-tauri/tauri.conf.json", "utf8"));
  const archives = readdirSync(directory).filter((file) => file.endsWith(".nsis.zip"));
  if (archives.length !== 2) throw new Error("Expected exactly two updater archives");
  for (const archive of archives) {
    verifySignature(
      readFileSync(join(directory, archive)),
      config.plugins.updater.pubkey,
      readFileSync(join(directory, archive + ".sig"), "utf8").trim(),
    );
    console.log(`Verified updater signature: ${archive}`);
  }
}
