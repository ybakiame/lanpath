import { test, expect } from "bun:test";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { verifySignature } from "./verify-release-signatures";

for (const algorithm of ["Ed", "ED"]) {
  test(`minisign ${algorithm} validates artifact and trusted comment, rejects tampering`, () => {
    const { publicKey, privateKey } = generateKeyPairSync("ed25519");
    const id = Buffer.from("0102030405060708", "hex");
    const rawKey = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
    const publicPacket = Buffer.concat([Buffer.from("Ed"), id, rawKey]);
    const pub = Buffer.from("untrusted comment: test\n" + publicPacket.toString("base64") + "\n").toString("base64");
    const data = Buffer.from("signed release fixture");
    const payload = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
    const signature = sign(null, payload, privateKey);
    const packet = Buffer.concat([Buffer.from(algorithm), id, signature]);
    const comment = "timestamp:123456";
    const global = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
    const text =
      "untrusted comment: test\n" +
      packet.toString("base64") +
      "\ntrusted comment: " +
      comment +
      "\n" +
      global.toString("base64");
    const encoded = Buffer.from(text).toString("base64");
    expect(() => verifySignature(data, pub, encoded)).not.toThrow();
    expect(() => verifySignature(Buffer.from("modified"), pub, encoded)).toThrow("artifact signature");
    const altered = Buffer.from(text.replace("timestamp:123456", "timestamp:999999")).toString("base64");
    expect(() => verifySignature(data, pub, altered)).toThrow("trusted comment");
    const wrongKey = Buffer.from(publicPacket);
    wrongKey[2] ^= 1;
    const wrongPublic = Buffer.from("untrusted comment: test\n" + wrongKey.toString("base64")).toString("base64");
    expect(() => verifySignature(data, wrongPublic, encoded)).toThrow("key identifier");
  });
}
