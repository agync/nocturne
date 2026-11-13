// Test helpers: private keys from the reference vectors' scalars (extractable only so that a
// test can build them from JWK; the app never does this).

import { type Bytes, fromHex, toBase64Url } from "../bytes";

export async function privateKeyFrom(scalarHex: string, publicHex: string): Promise<CryptoKey> {
  const pub = fromHex(publicHex);
  return crypto.subtle.importKey(
    "jwk",
    {
      kty: "EC",
      crv: "P-256",
      d: toBase64Url(fromHex(scalarHex)),
      x: toBase64Url(pub.slice(1, 33)),
      y: toBase64Url(pub.slice(33)),
    },
    { name: "ECDH", namedCurve: "P-256" },
    false,
    ["deriveBits"],
  );
}

export interface TestKey {
  privateKey: CryptoKey;
  publicKey: Bytes;
}

export async function testKey(scalarHex: string, publicHex: string): Promise<TestKey> {
  return { privateKey: await privateKeyFrom(scalarHex, publicHex), publicKey: fromHex(publicHex) };
}
