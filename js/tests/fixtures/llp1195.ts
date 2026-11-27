// LLP 1195.000 Stage 1 integration fixture covering D1, D2, D5 P0a, and D5 P0b.
declare const process: any;
declare const WebSocket: any;
declare const XMLHttpRequest: any;
declare const EventSource: any;

const enc = (s: string) => new TextEncoder().encode(s);
const hex = (buf: ArrayBuffer | Uint8Array) =>
  Array.from(buf instanceof Uint8Array ? buf : new Uint8Array(buf), b => b.toString(16).padStart(2, "0")).join("");
const unhex = (s: string) => {
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
};

let cachedNow: number | null = null;

function d1Globals(): string {
  return JSON.stringify({
    nodeEnv: process?.env?.NODE_ENV,
    frozen: Object.isFrozen(process),
    envFrozen: Object.isFrozen(process?.env),
    hasCwd: typeof process?.cwd,
    ws: typeof WebSocket,
    xhr: typeof XMLHttpRequest,
    es: typeof EventSource,
  });
}

function clockNow(): string {
  return JSON.stringify({
    now: Date.now(),
    newDate: new Date().getTime(),
    dateStr: Date(),
    perf: performance.now(),
    intl: new Intl.DateTimeFormat("en-US", {
      timeZone: "UTC",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(),
  });
}

function dateEpochUtc(): string {
  const d = new Date(0);
  return `${d.getTimezoneOffset()}/${d.getHours()}/${d.toString()}`;
}

function randomSeq(): string {
  return JSON.stringify([Math.random(), Math.random(), Math.random()]);
}

function uuidThenRandom(): string {
  const u1 = crypto.randomUUID();
  const r1 = Math.random();
  const u2 = crypto.randomUUID();
  return JSON.stringify({ u1, r1, u2 });
}

function lazyCacheSeed(): string {
  cachedNow = Date.now();
  return String(cachedNow);
}

function lazyCacheRead(): string {
  return cachedNow === null ? "empty" : String(cachedNow);
}

async function clockFetch(): Promise<string> {
  const t = Date.now();
  const r = await fetch("https://fixture.exact.test/ping");
  return `${t}:${await r.text()}`;
}

function eventsTest(): string {
  const target = new EventTarget();
  let calls = 0;
  let detailN = 0;
  const onPing = (e: Event) => {
    calls++;
    detailN = (e as CustomEvent).detail.n;
    e.preventDefault();
  };
  target.addEventListener("ping", onPing);
  const ev = new CustomEvent("ping", { detail: { n: 7 }, cancelable: true });
  const dispatched = target.dispatchEvent(ev);
  target.removeEventListener("ping", onPing);
  target.dispatchEvent(new Event("ping"));
  return JSON.stringify({
    calls,
    detailN,
    dispatched,
    defaultPrevented: ev.defaultPrevented,
    type: ev.type,
  });
}

async function blobFileTest(): Promise<string> {
  const blob = new Blob(["hello ", new Uint8Array([119, 111, 114, 108, 100])], { type: "TEXT/Plain" });
  const slice = blob.slice(6, 11, "text/plain");
  const reader = blob.stream().getReader();
  const chunks: number[] = [];
  for (;;) {
    const step = await reader.read();
    if (step.done) break;
    chunks.push(...step.value);
  }
  const clonedBlob = structuredClone(blob);
  const nowBefore = Date.now();
  const file = new File(["abc"], "note.txt", { type: "text/plain" });
  const clonedFile = structuredClone(file);
  return JSON.stringify({
    size: blob.size,
    type: blob.type,
    text: await blob.text(),
    sliceText: await slice.text(),
    abLen: (await blob.arrayBuffer()).byteLength,
    bytesLen: (await blob.bytes()).byteLength,
    streamText: new TextDecoder().decode(new Uint8Array(chunks)),
    clonedBlobText: await clonedBlob.text(),
    fileName: file.name,
    fileType: file.type,
    fileLastModifiedMatchesNow: file.lastModified === nowBefore,
    clonedFileName: clonedFile.name,
    clonedFileText: await clonedFile.text(),
  });
}

async function formDataTest(): Promise<string> {
  const fd = new FormData();
  fd.append("field", "alpha");
  fd.append("field", "beta");
  fd.append("drop", "bye");
  fd.delete("drop");
  fd.set("single", "one");
  fd.set("single", "two");
  fd.append("doc", new Blob(["hello-file"], { type: "text/plain" }), "hello.txt");
  const entries: string[] = [];
  fd.forEach((v, k) => entries.push(`${k}=${typeof v === "string" ? v : (v as File).name}`));
  const localResp = new Response(fd);
  const localCt = localResp.headers.get("content-type") || "";
  const localBody = await localResp.text();
  const uploaded = await fetch("https://fixture.exact.test/upload", { method: "POST", body: fd });
  return JSON.stringify({
    getField: fd.get("field"),
    getAllField: fd.getAll("field"),
    hasDrop: fd.has("drop"),
    getSingle: fd.get("single"),
    keys: Array.from(fd.keys()),
    entries,
    localCtHasMultipart: localCt.startsWith("multipart/form-data; boundary="),
    localHasFile: localBody.includes('filename="hello.txt"') && localBody.includes("hello-file"),
    uploadStatus: uploaded.status,
    uploadAck: await uploaded.text(),
  });
}

async function requestResponseTest(): Promise<string> {
  const req = new Request("https://fixture.exact.test/req", {
    method: "post",
    headers: { "x-tag": "stage1" },
    body: new Blob(["payload-bytes"], { type: "application/octet-stream" }),
  });
  const copy = req.clone();
  const copyText = await copy.text();
  const copyUsed = copy.bodyUsed;
  const reqUsedBefore = req.bodyUsed;
  const res = await fetch(req);
  const reqUsedAfter = req.bodyUsed;
  const resClone = res.clone();
  const resBlob = await res.blob();
  const resBytes = await resClone.bytes();
  return JSON.stringify({
    method: req.method,
    header: req.headers.get("x-tag"),
    copyText,
    copyUsed,
    reqUsedBefore,
    reqUsedAfter,
    blobSize: resBlob.size,
    blobType: resBlob.type,
    blobText: await resBlob.text(),
    bytesHex: hex(resBytes),
  });
}

async function cryptoHmac(): Promise<string> {
  // RFC 4231 Test Case 1
  const rawKey = new Uint8Array(20).fill(0x0b);
  const key = await crypto.subtle.importKey("raw", rawKey, { name: "HMAC", hash: "SHA-256" }, true, ["sign", "verify"]);
  const msg = enc("Hi There");
  const sig = await crypto.subtle.sign("HMAC", key, msg);
  const ok = await crypto.subtle.verify("HMAC", key, sig, msg);
  const bad = await crypto.subtle.verify("HMAC", key, sig, enc("Hi There!"));
  const exportedRaw = hex(await crypto.subtle.exportKey("raw", key));
  const jwk = await crypto.subtle.exportKey("jwk", key);
  const fromJwk = await crypto.subtle.importKey("jwk", jwk, { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]);
  const jwkSig = hex(await crypto.subtle.sign("HMAC", fromJwk, msg));
  let nonExtErr = "";
  try { await crypto.subtle.exportKey("raw", fromJwk); } catch (e: any) { nonExtErr = e.name; }
  return JSON.stringify({
    sig: hex(sig),
    ok,
    bad,
    exportedRaw,
    jwkKty: jwk.kty,
    jwkAlg: jwk.alg,
    jwkSig,
    nonExtErr,
  });
}

async function cryptoAesGcm(): Promise<string> {
  const keyBytes = unhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
  const iv = unhex("0102030405060708090a0b0c");
  const aad = enc("header-aad");
  const plain = enc("secret-message-llp1195");
  const key = await crypto.subtle.importKey("raw", keyBytes, "AES-GCM", true, ["encrypt", "decrypt"]);
  const jwk = await crypto.subtle.exportKey("jwk", key);
  const key2 = await crypto.subtle.importKey("jwk", jwk, "AES-GCM", true, ["encrypt", "decrypt"]);
  const ct = await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: aad, tagLength: 128 }, key, plain);
  const pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv, additionalData: aad, tagLength: 128 }, key2, ct);
  const tampered = new Uint8Array(ct.slice(0));
  tampered[tampered.length - 1] ^= 0xff;
  let tamperErr = "";
  try { await crypto.subtle.decrypt({ name: "AES-GCM", iv, additionalData: aad }, key2, tampered); } catch (e: any) { tamperErr = e.name; }
  // 128-bit raw round-trip
  const key128 = await crypto.subtle.importKey("raw", keyBytes.subarray(0, 16), "AES-GCM", true, ["encrypt", "decrypt"]);
  const ct128 = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key128, enc("aes128"));
  const pt128 = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key128, ct128);
  return JSON.stringify({
    roundTrip: new TextDecoder().decode(pt),
    roundTrip128: new TextDecoder().decode(pt128),
    jwkAlg: jwk.alg,
    rawBack: hex(await crypto.subtle.exportKey("raw", key2)),
    tamperErr,
  });
}

async function cryptoPbkdf2(): Promise<string> {
  // RFC 6070 SHA-256 vector ("password", "salt", c=1, dkLen=32)
  const base = await crypto.subtle.importKey("raw", enc("password"), "PBKDF2", false, ["deriveBits", "deriveKey"]);
  const bits = await crypto.subtle.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt: enc("salt"), iterations: 1 }, base, 256);
  const aesKey = await crypto.subtle.deriveKey(
    { name: "PBKDF2", hash: "SHA-256", salt: enc("salt"), iterations: 1 },
    base,
    { name: "AES-GCM", length: 256 },
    true,
    ["encrypt", "decrypt"],
  );
  const iv = new Uint8Array(12);
  const ct = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, aesKey, enc("derived-ok"));
  const pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, aesKey, ct);
  return JSON.stringify({
    bits: hex(bits),
    derivedKeyMatchesBits: hex(await crypto.subtle.exportKey("raw", aesKey)) === hex(bits),
    roundTrip: new TextDecoder().decode(pt),
  });
}

async function cryptoHkdf(): Promise<string> {
  // RFC 5869 Test Case 1
  const ikm = new Uint8Array(22).fill(0x0b);
  const salt = unhex("000102030405060708090a0b0c");
  const info = unhex("f0f1f2f3f4f5f6f7f8f9");
  const base = await crypto.subtle.importKey("raw", ikm, "HKDF", false, ["deriveBits", "deriveKey"]);
  const okm = await crypto.subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt, info }, base, 42 * 8);
  const macKey = await crypto.subtle.deriveKey(
    { name: "HKDF", hash: "SHA-256", salt, info },
    base,
    { name: "HMAC", hash: "SHA-256", length: 256 },
    true,
    ["sign", "verify"],
  );
  const mac = await crypto.subtle.sign("HMAC", macKey, enc("hkdf-msg"));
  const verified = await crypto.subtle.verify("HMAC", macKey, mac, enc("hkdf-msg"));
  return JSON.stringify({
    okm: hex(okm),
    macKeyPrefix: hex(await crypto.subtle.exportKey("raw", macKey)),
    verified,
  });
}

async function cryptoEd25519(): Promise<string> {
  // RFC 8032 Test Vector 1 (empty message)
  const seedHex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
  const pubHex = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
  // Standard RFC 8410 Ed25519 PKCS#8 v1 prefix (16 bytes) + 32-byte seed
  const pkcs8 = unhex("302e020100300506032b657004220420" + seedHex);
  const priv = await crypto.subtle.importKey("pkcs8", pkcs8, "Ed25519", true, ["sign"]);
  const sig = await crypto.subtle.sign("Ed25519", priv, new Uint8Array(0));
  const privJwk = await crypto.subtle.exportKey("jwk", priv);
  const pubRaw = await crypto.subtle.importKey("raw", unhex(pubHex), "Ed25519", true, ["verify"]);
  const spki = await crypto.subtle.exportKey("spki", pubRaw);
  const pubSpki = await crypto.subtle.importKey("spki", spki, "Ed25519", true, ["verify"]);
  const pubJwk = await crypto.subtle.exportKey("jwk", pubSpki);
  const pubFromJwk = await crypto.subtle.importKey("jwk", pubJwk, "Ed25519", true, ["verify"]);
  return JSON.stringify({
    sig: hex(sig),
    verifyRaw: await crypto.subtle.verify("Ed25519", pubRaw, sig, new Uint8Array(0)),
    verifySpki: await crypto.subtle.verify("Ed25519", pubSpki, sig, new Uint8Array(0)),
    verifyJwk: await crypto.subtle.verify("Ed25519", pubFromJwk, sig, new Uint8Array(0)),
    verifyTampered: await crypto.subtle.verify("Ed25519", pubRaw, sig, new Uint8Array([1])),
    pkcs8RoundTrip: hex(await crypto.subtle.exportKey("pkcs8", priv)) === hex(pkcs8),
    rawRoundTrip: hex(await crypto.subtle.exportKey("raw", pubFromJwk)) === pubHex,
    privJwkCrv: privJwk.crv,
    pubJwkCrv: pubJwk.crv,
  });
}

async function cryptoRsaVerify(spkiHex: string, jwkJson: string, sigHex: string): Promise<string> {
  const msg = enc("llp-1195-rsa-vector");
  const sig = unhex(sigHex);
  const kSpki = await crypto.subtle.importKey("spki", unhex(spkiHex), { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, true, ["verify"]);
  const kJwk = await crypto.subtle.importKey("jwk", JSON.parse(jwkJson), { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, true, ["verify"]);
  const exportedSpki = hex(await crypto.subtle.exportKey("spki", kJwk));
  const exportedJwk = await crypto.subtle.exportKey("jwk", kSpki);
  const badSig = sig.slice();
  badSig[badSig.length - 1] ^= 1;
  return JSON.stringify({
    okSpki: await crypto.subtle.verify("RSASSA-PKCS1-v1_5", kSpki, sig, msg),
    okJwk: await crypto.subtle.verify("RSASSA-PKCS1-v1_5", kJwk, sig, msg),
    badSig: await crypto.subtle.verify("RSASSA-PKCS1-v1_5", kSpki, badSig, msg),
    badMsg: await crypto.subtle.verify("RSASSA-PKCS1-v1_5", kSpki, sig, enc("other")),
    spkiMatches: exportedSpki === spkiHex,
    jwkKty: exportedJwk.kty,
    jwkAlg: exportedJwk.alg,
  });
}

async function cryptoEcdsaVerify(pubJwkJson: string, sigHex: string, msgText: string): Promise<string> {
  const key = await crypto.subtle.importKey("jwk", JSON.parse(pubJwkJson), { name: "ECDSA", namedCurve: "P-256" }, true, ["verify"]);
  const ok = await crypto.subtle.verify({ name: "ECDSA", hash: "SHA-256" }, key, unhex(sigHex), enc(msgText));
  const bad = await crypto.subtle.verify({ name: "ECDSA", hash: "SHA-256" }, key, unhex(sigHex), enc(msgText + "!"));
  return JSON.stringify({ ok, bad });
}

async function cryptoGenKeys(): Promise<string> {
  const hm = await crypto.subtle.generateKey({ name: "HMAC", hash: "SHA-256" }, true, ["sign", "verify"]);
  const ae = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, true, ["encrypt", "decrypt"]);
  const ed = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]) as CryptoKeyPair;
  const sig = await crypto.subtle.sign("Ed25519", ed.privateKey, enc("gen-ed"));
  const edOk = await crypto.subtle.verify("Ed25519", ed.publicKey, sig, enc("gen-ed"));
  return JSON.stringify({
    hmLen: (await crypto.subtle.exportKey("raw", hm)).byteLength,
    aeLen: (await crypto.subtle.exportKey("raw", ae)).byteLength,
    edOk,
  });
}

function answer(source: string, args: any[]): unknown {
  switch (source) {
    case "d1Globals": return d1Globals();
    case "clockNow": return clockNow();
    case "dateEpochUtc": return dateEpochUtc();
    case "randomSeq": return randomSeq();
    case "uuidThenRandom": return uuidThenRandom();
    case "lazyCacheSeed": return lazyCacheSeed();
    case "lazyCacheRead": return lazyCacheRead();
    case "pureSource": return "pure-constant";
    case "clockFetch": return clockFetch();
    case "clockMutate": return String(Date.now());
    case "eventsTest": return eventsTest();
    case "blobFileTest": return blobFileTest();
    case "formDataTest": return formDataTest();
    case "requestResponseTest": return requestResponseTest();
    case "cryptoHmac": return cryptoHmac();
    case "cryptoAesGcm": return cryptoAesGcm();
    case "cryptoPbkdf2": return cryptoPbkdf2();
    case "cryptoHkdf": return cryptoHkdf();
    case "cryptoEd25519": return cryptoEd25519();
    case "cryptoRsaVerify": return cryptoRsaVerify(args[0], args[1], args[2]);
    case "cryptoEcdsaVerify": return cryptoEcdsaVerify(args[0], args[1], args[2]);
    case "cryptoGenKeys": return cryptoGenKeys();
    default: throw new Error("unknown source " + source);
  }
}

(globalThis as any).exact = {
  abi: 1,
  appId: "test.llp1195",
  grants: "net.fetch https://fixture.exact.test\n",
  answer,
};
