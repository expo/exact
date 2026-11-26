//! Extended `crypto.subtle` algorithms on Hermes (LLP 1195.000 D5 P0b):
//! HMAC (SHA-256/384/512), AES-GCM (128/256), PBKDF2, HKDF, RSASSA-PKCS1-v1_5
//! verification (SPKI & JWK), Ed25519 (raw/SPKI/PKCS#8/JWK), and ECDSA P-256
//! verification, backed by `ring` (already in the workspace lockfile) and
//! `exact_data::crypto`.

use std::num::NonZeroU32;

use exact_data::crypto::{base64url, unbase64url, EcKey, Sha};
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::hkdf::{self, KeyType};
use ring::hmac;
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{self, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

use crate::crypto::hex;
use crate::HostState;

pub(crate) const MAX_KEYS: usize = 4096;

/// Native key stored behind an opaque handle in `HostState::keys`.
#[derive(Clone)]
pub(crate) enum NativeKey {
    Ec(EcKey),
    Hmac {
        hash: Sha,
        bytes: Vec<u8>,
        extractable: bool,
    },
    AesGcm {
        bytes: Vec<u8>,
        extractable: bool,
    },
    Pbkdf2 {
        bytes: Vec<u8>,
    },
    Hkdf {
        bytes: Vec<u8>,
    },
    RsaVerify {
        hash: Sha,
        rsa_der: Vec<u8>,
        n: Vec<u8>,
        e: Vec<u8>,
    },
    Ed25519 {
        seed: Option<[u8; 32]>,
        public: [u8; 32],
        extractable: bool,
    },
}

fn parse_sha(name: &str) -> Result<Sha, String> {
    match name {
        "SHA-256" => Ok(Sha::Sha256),
        "SHA-384" => Ok(Sha::Sha384),
        "SHA-512" => Ok(Sha::Sha512),
        other => Err(format!("NotSupportedError: unsupported hash {other}")),
    }
}

fn sha_name(sha: Sha) -> &'static str {
    match sha {
        Sha::Sha256 => "SHA-256",
        Sha::Sha384 => "SHA-384",
        Sha::Sha512 => "SHA-512",
    }
}

fn sha_len(sha: Sha) -> usize {
    match sha {
        Sha::Sha256 => 32,
        Sha::Sha384 => 48,
        Sha::Sha512 => 64,
    }
}

fn hmac_alg(sha: Sha) -> hmac::Algorithm {
    match sha {
        Sha::Sha256 => hmac::HMAC_SHA256,
        Sha::Sha384 => hmac::HMAC_SHA384,
        Sha::Sha512 => hmac::HMAC_SHA512,
    }
}

fn pbkdf2_alg(sha: Sha) -> pbkdf2::Algorithm {
    match sha {
        Sha::Sha256 => pbkdf2::PBKDF2_HMAC_SHA256,
        Sha::Sha384 => pbkdf2::PBKDF2_HMAC_SHA384,
        Sha::Sha512 => pbkdf2::PBKDF2_HMAC_SHA512,
    }
}

fn hkdf_alg(sha: Sha) -> hkdf::Algorithm {
    match sha {
        Sha::Sha256 => hkdf::HKDF_SHA256,
        Sha::Sha384 => hkdf::HKDF_SHA384,
        Sha::Sha512 => hkdf::HKDF_SHA512,
    }
}

fn rsa_alg(sha: Sha) -> &'static signature::RsaParameters {
    match sha {
        Sha::Sha256 => &signature::RSA_PKCS1_2048_8192_SHA256,
        Sha::Sha384 => &signature::RSA_PKCS1_2048_8192_SHA384,
        Sha::Sha512 => &signature::RSA_PKCS1_2048_8192_SHA512,
    }
}

pub(crate) fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("DataError: invalid hex length".into());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let b = s.as_bytes();
    for i in (0..b.len()).step_by(2) {
        let hi = hex_nibble(b[i]).ok_or_else(|| "DataError: invalid hex".to_string())?;
        let lo = hex_nibble(b[i + 1]).ok_or_else(|| "DataError: invalid hex".to_string())?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn push_key(state: &mut HostState, key: NativeKey) -> Result<u64, String> {
    if state.keys.len() >= MAX_KEYS {
        return Err("QuotaExceededError: too many live CryptoKeys".into());
    }
    let handle = state.next_key;
    state.next_key += 1;
    state.keys.insert(handle, key);
    Ok(handle)
}

fn get_key(state: &HostState, handle: u64) -> Result<&NativeKey, String> {
    state
        .keys
        .get(&handle)
        .ok_or_else(|| "InvalidAccessError: not a live CryptoKey".to_string())
}

fn fill_os_random(state: &mut HostState, out: &mut [u8]) -> Result<(), String> {
    SystemRandom::new()
        .fill(out)
        .map_err(|_| "OperationError: OS randomness unavailable".to_string())?;
    if let Some(store) = state.store {
        // SAFETY: the store the seam handed this call outlives the call.
        unsafe { (*store).observe_entropy() };
    }
    Ok(())
}

// --- Minimal ASN.1 DER helpers for RSA SPKI & Ed25519 SPKI/PKCS#8 -----------

fn der_len(len: usize, out: &mut Vec<u8>) {
    if len < 0x80 {
        out.push(len as u8);
    } else if len <= 0xff {
        out.push(0x81);
        out.push(len as u8);
    } else if len <= 0xffff {
        out.push(0x82);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    } else {
        out.push(0x83);
        out.push((len >> 16) as u8);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    }
}

fn der_tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 5);
    out.push(tag);
    der_len(body.len(), &mut out);
    out.extend_from_slice(body);
    out
}

fn der_integer_unsigned(val: &[u8]) -> Vec<u8> {
    let trimmed = trim_leading_zeros(val);
    let mut body = Vec::with_capacity(trimmed.len() + 1);
    if trimmed.is_empty() || trimmed[0] >= 0x80 {
        body.push(0);
    }
    body.extend_from_slice(trimmed);
    der_tlv(0x02, &body)
}

fn trim_leading_zeros(bytes: &[u8]) -> &[u8] {
    let i = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    &bytes[i..]
}

fn read_der_tlv(input: &[u8], expected_tag: u8) -> Result<(&[u8], &[u8]), String> {
    if input.is_empty() || input[0] != expected_tag {
        return Err("DataError: malformed DER structure".into());
    }
    let (len, hdr) = read_der_len(&input[1..])?;
    let start = 1 + hdr;
    let end = start
        .checked_add(len)
        .ok_or_else(|| "DataError: DER length overflow".to_string())?;
    if end > input.len() {
        return Err("DataError: truncated DER element".into());
    }
    Ok((&input[start..end], &input[end..]))
}

fn read_der_len(input: &[u8]) -> Result<(usize, usize), String> {
    let &first = input
        .first()
        .ok_or_else(|| "DataError: truncated DER length".to_string())?;
    if first < 0x80 {
        return Ok((first as usize, 1));
    }
    let count = (first & 0x7f) as usize;
    if count == 0 || count > 4 || input.len() < 1 + count {
        return Err("DataError: unsupported DER length".into());
    }
    let mut len = 0usize;
    for &b in &input[1..=count] {
        len = (len << 8) | (b as usize);
    }
    Ok((len, 1 + count))
}

const RSA_ALGID_DER: &[u8] = &[
    0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00,
];
const ED25519_OID_DER: &[u8] = &[0x06, 0x03, 0x2b, 0x65, 0x70];
const ED25519_SPKI_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];
const ED25519_PKCS8_PREFIX: &[u8] = &[
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

fn encode_rsa_der(n: &[u8], e: &[u8]) -> Vec<u8> {
    let mut seq = der_integer_unsigned(n);
    seq.extend_from_slice(&der_integer_unsigned(e));
    der_tlv(0x30, &seq)
}

fn encode_rsa_spki(rsa_der: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(rsa_der.len() + 1);
    bits.push(0);
    bits.extend_from_slice(rsa_der);
    let mut body = Vec::with_capacity(RSA_ALGID_DER.len() + bits.len() + 5);
    body.extend_from_slice(RSA_ALGID_DER);
    body.extend_from_slice(&der_tlv(0x03, &bits));
    der_tlv(0x30, &body)
}

type RsaPublicParts = (Vec<u8>, Vec<u8>, Vec<u8>);

fn parse_rsa_spki(spki: &[u8]) -> Result<RsaPublicParts, String> {
    let (body, rest) = read_der_tlv(spki, 0x30)?;
    if !rest.is_empty() {
        return Err("DataError: trailing bytes in SPKI".into());
    }
    let (alg_id, after_alg) = read_der_tlv(body, 0x30)?;
    // OID 1.2.840.113549.1.1.1 (rsaEncryption)
    let (oid, alg_rest) = read_der_tlv(alg_id, 0x06)?;
    if oid != [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01] {
        return Err("DataError: SPKI is not rsaEncryption".into());
    }
    if !alg_rest.is_empty() && alg_rest != [0x05, 0x00] {
        return Err("DataError: invalid RSA AlgorithmIdentifier parameters".into());
    }
    let (bit_str, tail) = read_der_tlv(after_alg, 0x03)?;
    if !tail.is_empty() || bit_str.is_empty() || bit_str[0] != 0 {
        return Err("DataError: malformed RSA SPKI BIT STRING".into());
    }
    let rsa_der = &bit_str[1..];
    let (inner, inner_tail) = read_der_tlv(rsa_der, 0x30)?;
    if !inner_tail.is_empty() {
        return Err("DataError: trailing bytes in RSAPublicKey".into());
    }
    let (n_raw, after_n) = read_der_tlv(inner, 0x02)?;
    let (e_raw, after_e) = read_der_tlv(after_n, 0x02)?;
    if !after_e.is_empty() || n_raw.is_empty() || e_raw.is_empty() {
        return Err("DataError: invalid RSAPublicKey integers".into());
    }
    if n_raw[0] & 0x80 != 0 || e_raw[0] & 0x80 != 0 {
        return Err("DataError: negative RSA key parameter".into());
    }
    let n = trim_leading_zeros(n_raw).to_vec();
    let e = trim_leading_zeros(e_raw).to_vec();
    if n.is_empty() || e.is_empty() {
        return Err("DataError: zero RSA key parameter".into());
    }
    Ok((rsa_der.to_vec(), n, e))
}

fn bit_len(big_endian: &[u8]) -> usize {
    let t = trim_leading_zeros(big_endian);
    if t.is_empty() {
        return 0;
    }
    (t.len() - 1) * 8 + (8 - t[0].leading_zeros() as usize)
}

fn parse_ed25519_spki(spki: &[u8]) -> Result<[u8; 32], String> {
    if spki.len() == 44 && spki.starts_with(ED25519_SPKI_PREFIX) {
        let mut pub_key = [0u8; 32];
        pub_key.copy_from_slice(&spki[12..]);
        return Ok(pub_key);
    }
    Err("DataError: invalid Ed25519 SPKI".into())
}

fn parse_ed25519_pkcs8(pkcs8: &[u8]) -> Result<[u8; 32], String> {
    let (body, rest) = read_der_tlv(pkcs8, 0x30)?;
    if !rest.is_empty() {
        return Err("DataError: trailing bytes in Ed25519 PKCS#8".into());
    }
    let (ver, after_ver) = read_der_tlv(body, 0x02)?;
    if ver != [0] && ver != [1] {
        return Err("DataError: unsupported PKCS#8 version".into());
    }
    let (alg_id, after_alg) = read_der_tlv(after_ver, 0x30)?;
    if alg_id != ED25519_OID_DER {
        return Err("DataError: PKCS#8 is not Ed25519".into());
    }
    let (priv_octets, _) = read_der_tlv(after_alg, 0x04)?;
    let (seed_bytes, inner_tail) = read_der_tlv(priv_octets, 0x04)?;
    if !inner_tail.is_empty() || seed_bytes.len() != 32 {
        return Err("DataError: Ed25519 PKCS#8 seed must be 32 bytes".into());
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(seed_bytes);
    Ok(seed)
}

fn ed25519_public_from_seed(seed: &[u8; 32]) -> Result<[u8; 32], String> {
    let pair = Ed25519KeyPair::from_seed_unchecked(seed)
        .map_err(|_| "DataError: invalid Ed25519 seed".to_string())?;
    let mut public = [0u8; 32];
    public.copy_from_slice(pair.public_key().as_ref());
    Ok(public)
}

// --- Bytes-door entry point (ops 16..=22) -----------------------------------

pub(crate) fn door(state: &mut HostState, op: u32, a: &str, data: &[u8]) -> Result<String, String> {
    match op {
        16 => op_generate(state, a),
        17 => op_import(state, a, data),
        18 => op_export(state, a),
        19 => op_sign(state, a, data),
        20 => op_verify(state, a, data),
        21 => op_cipher(state, a, data),
        22 => op_derive(state, a),
        _ => Err(format!("__exact_bytes: no subtle op {op}")),
    }
}

fn op_generate(state: &mut HostState, spec: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("DataError: {e}"))?;
    let alg = v["alg"].as_str().unwrap_or("");
    let extractable = v["extractable"].as_bool().unwrap_or(false);
    match alg {
        "HMAC" => {
            let hash = parse_sha(v["hash"].as_str().unwrap_or(""))?;
            let default_bits = match hash {
                Sha::Sha256 => 512usize,
                Sha::Sha384 | Sha::Sha512 => 1024usize,
            };
            let bits = v["length"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or(default_bits);
            if bits == 0 || !bits.is_multiple_of(8) || bits > 16_384 {
                return Err(
                    "OperationError: HMAC key length must be a positive multiple of 8 bits".into(),
                );
            }
            let mut bytes = vec![0u8; bits / 8];
            fill_os_random(state, &mut bytes)?;
            let handle = push_key(
                state,
                NativeKey::Hmac {
                    hash,
                    bytes,
                    extractable,
                },
            )?;
            Ok(serde_json::json!({"handle": handle, "length": bits}).to_string())
        }
        "AES-GCM" => {
            let bits = v["length"].as_u64().unwrap_or(0) as usize;
            if bits != 128 && bits != 256 {
                return Err(format!(
                    "OperationError: AES-GCM length must be 128 or 256 bits, not {bits}"
                ));
            }
            let mut bytes = vec![0u8; bits / 8];
            fill_os_random(state, &mut bytes)?;
            let handle = push_key(state, NativeKey::AesGcm { bytes, extractable })?;
            Ok(serde_json::json!({"handle": handle, "length": bits}).to_string())
        }
        "Ed25519" => {
            if state.keys.len() + 2 > MAX_KEYS {
                return Err("QuotaExceededError: too many live CryptoKeys".into());
            }
            let mut seed = [0u8; 32];
            fill_os_random(state, &mut seed)?;
            let public = ed25519_public_from_seed(&seed)?;
            let priv_h = push_key(
                state,
                NativeKey::Ed25519 {
                    seed: Some(seed),
                    public,
                    extractable,
                },
            )?;
            let pub_h = push_key(
                state,
                NativeKey::Ed25519 {
                    seed: None,
                    public,
                    extractable: true,
                },
            )?;
            Ok(format!("{{\"private\":{priv_h},\"public\":{pub_h}}}"))
        }
        other => Err(format!("NotSupportedError: generateKey: {other}")),
    }
}

fn op_import(state: &mut HostState, spec: &str, data: &[u8]) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("DataError: {e}"))?;
    let alg = v["alg"].as_str().unwrap_or("");
    let format = v["format"].as_str().unwrap_or("");
    let extractable = v["extractable"].as_bool().unwrap_or(false);
    match alg {
        "HMAC" => {
            let hash = parse_sha(v["hash"].as_str().unwrap_or(""))?;
            let bytes = match format {
                "raw" => data.to_vec(),
                "jwk" => {
                    let jwk = &v["jwk"];
                    if jwk["kty"].as_str() != Some("oct") {
                        return Err("DataError: HMAC JWK requires kty 'oct'".into());
                    }
                    if let Some(jalg) = jwk["alg"].as_str() {
                        let expected = match hash {
                            Sha::Sha256 => "HS256",
                            Sha::Sha384 => "HS384",
                            Sha::Sha512 => "HS512",
                        };
                        if jalg != expected {
                            return Err(format!(
                                "DataError: HMAC JWK alg {jalg} mismatches {expected}"
                            ));
                        }
                    }
                    let k = jwk["k"]
                        .as_str()
                        .ok_or_else(|| "DataError: HMAC JWK missing 'k'".to_string())?;
                    unbase64url(k)
                        .ok_or_else(|| "DataError: invalid base64url in JWK 'k'".to_string())?
                }
                other => return Err(format!("NotSupportedError: HMAC importKey format {other}")),
            };
            if bytes.is_empty() {
                return Err("DataError: HMAC key data cannot be empty".into());
            }
            let length = bytes.len() * 8;
            let handle = push_key(
                state,
                NativeKey::Hmac {
                    hash,
                    bytes,
                    extractable,
                },
            )?;
            Ok(serde_json::json!({"handle": handle, "length": length}).to_string())
        }
        "AES-GCM" => {
            let bytes = match format {
                "raw" => data.to_vec(),
                "jwk" => {
                    let jwk = &v["jwk"];
                    if jwk["kty"].as_str() != Some("oct") {
                        return Err("DataError: AES-GCM JWK requires kty 'oct'".into());
                    }
                    let k = jwk["k"]
                        .as_str()
                        .ok_or_else(|| "DataError: AES-GCM JWK missing 'k'".to_string())?;
                    let decoded = unbase64url(k)
                        .ok_or_else(|| "DataError: invalid base64url in JWK 'k'".to_string())?;
                    if let Some(jalg) = jwk["alg"].as_str() {
                        let expected = match decoded.len() {
                            16 => "A128GCM",
                            32 => "A256GCM",
                            _ => "",
                        };
                        if !expected.is_empty() && jalg != expected {
                            return Err(format!(
                                "DataError: AES-GCM JWK alg {jalg} mismatches key length"
                            ));
                        }
                    }
                    decoded
                }
                other => {
                    return Err(format!(
                        "NotSupportedError: AES-GCM importKey format {other}"
                    ))
                }
            };
            if bytes.len() != 16 && bytes.len() != 32 {
                return Err(format!(
                    "DataError: AES-GCM key must be 128 or 256 bits, got {} bits",
                    bytes.len() * 8
                ));
            }
            let length = bytes.len() * 8;
            let handle = push_key(state, NativeKey::AesGcm { bytes, extractable })?;
            Ok(serde_json::json!({"handle": handle, "length": length}).to_string())
        }
        "PBKDF2" => {
            if format != "raw" {
                return Err(format!(
                    "NotSupportedError: PBKDF2 importKey format {format}"
                ));
            }
            if extractable {
                return Err("SyntaxError: PBKDF2 keys cannot be extractable".into());
            }
            let handle = push_key(
                state,
                NativeKey::Pbkdf2 {
                    bytes: data.to_vec(),
                },
            )?;
            Ok(serde_json::json!({"handle": handle}).to_string())
        }
        "HKDF" => {
            if format != "raw" {
                return Err(format!("NotSupportedError: HKDF importKey format {format}"));
            }
            if extractable {
                return Err("SyntaxError: HKDF keys cannot be extractable".into());
            }
            let handle = push_key(
                state,
                NativeKey::Hkdf {
                    bytes: data.to_vec(),
                },
            )?;
            Ok(serde_json::json!({"handle": handle}).to_string())
        }
        "RSASSA-PKCS1-v1_5" => {
            let hash = parse_sha(v["hash"].as_str().unwrap_or(""))?;
            let (rsa_der, n, e) = match format {
                "spki" => parse_rsa_spki(data)?,
                "jwk" => {
                    let jwk = &v["jwk"];
                    if jwk["kty"].as_str() != Some("RSA") {
                        return Err("DataError: RSA JWK requires kty 'RSA'".into());
                    }
                    if jwk.get("d").and_then(|d| d.as_str()).is_some() {
                        return Err(
                            "NotSupportedError: RSASSA-PKCS1-v1_5 supports public verification keys only".into(),
                        );
                    }
                    let n_b64 = jwk["n"]
                        .as_str()
                        .ok_or_else(|| "DataError: RSA JWK missing 'n'".to_string())?;
                    let e_b64 = jwk["e"]
                        .as_str()
                        .ok_or_else(|| "DataError: RSA JWK missing 'e'".to_string())?;
                    let n = trim_leading_zeros(&unbase64url(n_b64).ok_or_else(|| {
                        "DataError: invalid base64url in RSA JWK 'n'".to_string()
                    })?)
                    .to_vec();
                    let e = trim_leading_zeros(&unbase64url(e_b64).ok_or_else(|| {
                        "DataError: invalid base64url in RSA JWK 'e'".to_string()
                    })?)
                    .to_vec();
                    if n.is_empty() || e.is_empty() {
                        return Err("DataError: invalid RSA JWK modulus or exponent".into());
                    }
                    (encode_rsa_der(&n, &e), n, e)
                }
                other => {
                    return Err(format!(
                        "NotSupportedError: RSASSA-PKCS1-v1_5 importKey format {other}"
                    ))
                }
            };
            let modulus_length = bit_len(&n);
            if !(2048..=8192).contains(&modulus_length) {
                return Err(format!(
                    "DataError: RSA modulus length {modulus_length} outside supported 2048..8192 bits"
                ));
            }
            let e_hex = hex(&e);
            let handle = push_key(
                state,
                NativeKey::RsaVerify {
                    hash,
                    rsa_der,
                    n,
                    e,
                },
            )?;
            Ok(serde_json::json!({
                "handle": handle,
                "modulusLength": modulus_length,
                "publicExponentHex": e_hex,
                "hash": sha_name(hash),
            })
            .to_string())
        }
        "Ed25519" => {
            let (seed, public, is_private) = match format {
                "raw" => {
                    let public: [u8; 32] = data.try_into().map_err(|_| {
                        "DataError: Ed25519 raw public key must be 32 bytes".to_string()
                    })?;
                    (None, public, false)
                }
                "spki" => (None, parse_ed25519_spki(data)?, false),
                "pkcs8" => {
                    let seed = parse_ed25519_pkcs8(data)?;
                    let public = ed25519_public_from_seed(&seed)?;
                    (Some(seed), public, true)
                }
                "jwk" => {
                    let jwk = &v["jwk"];
                    if jwk["kty"].as_str() != Some("OKP") || jwk["crv"].as_str() != Some("Ed25519")
                    {
                        return Err(
                            "DataError: Ed25519 JWK requires kty 'OKP' and crv 'Ed25519'".into(),
                        );
                    }
                    let x_bytes = unbase64url(
                        jwk["x"]
                            .as_str()
                            .ok_or_else(|| "DataError: Ed25519 JWK missing 'x'".to_string())?,
                    )
                    .ok_or_else(|| "DataError: invalid base64url in Ed25519 JWK 'x'".to_string())?;
                    let public: [u8; 32] = x_bytes
                        .try_into()
                        .map_err(|_| "DataError: Ed25519 JWK 'x' must be 32 bytes".to_string())?;
                    if let Some(d_str) = jwk["d"].as_str() {
                        let d_bytes = unbase64url(d_str).ok_or_else(|| {
                            "DataError: invalid base64url in Ed25519 JWK 'd'".to_string()
                        })?;
                        let seed: [u8; 32] = d_bytes.try_into().map_err(|_| {
                            "DataError: Ed25519 JWK 'd' must be 32 bytes".to_string()
                        })?;
                        let derived = ed25519_public_from_seed(&seed)?;
                        if derived != public {
                            return Err("DataError: Ed25519 JWK 'd' does not match 'x'".into());
                        }
                        (Some(seed), public, true)
                    } else {
                        (None, public, false)
                    }
                }
                other => {
                    return Err(format!(
                        "NotSupportedError: Ed25519 importKey format {other}"
                    ))
                }
            };
            let handle = push_key(
                state,
                NativeKey::Ed25519 {
                    seed,
                    public,
                    extractable: if is_private { extractable } else { true },
                },
            )?;
            Ok(serde_json::json!({
                "handle": handle,
                "type": if is_private { "private" } else { "public" },
            })
            .to_string())
        }
        other => Err(format!("NotSupportedError: importKey: {other}")),
    }
}

fn op_export(state: &HostState, spec: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("DataError: {e}"))?;
    let handle = v["handle"]
        .as_u64()
        .ok_or_else(|| "InvalidAccessError: missing key handle".to_string())?;
    let format = v["format"].as_str().unwrap_or("");
    let key = get_key(state, handle)?;
    match key {
        NativeKey::Hmac {
            hash,
            bytes,
            extractable,
        } => {
            if !extractable {
                return Err("InvalidAccessError: exportKey: the key is not extractable".into());
            }
            match format {
                "raw" => Ok(hex(bytes)),
                "jwk" => {
                    let alg = match hash {
                        Sha::Sha256 => "HS256",
                        Sha::Sha384 => "HS384",
                        Sha::Sha512 => "HS512",
                    };
                    Ok(serde_json::json!({
                        "kty": "oct",
                        "k": base64url(bytes),
                        "alg": alg,
                    })
                    .to_string())
                }
                other => Err(format!("NotSupportedError: HMAC exportKey format {other}")),
            }
        }
        NativeKey::AesGcm { bytes, extractable } => {
            if !extractable {
                return Err("InvalidAccessError: exportKey: the key is not extractable".into());
            }
            match format {
                "raw" => Ok(hex(bytes)),
                "jwk" => {
                    let alg = if bytes.len() == 16 {
                        "A128GCM"
                    } else {
                        "A256GCM"
                    };
                    Ok(serde_json::json!({
                        "kty": "oct",
                        "k": base64url(bytes),
                        "alg": alg,
                    })
                    .to_string())
                }
                other => Err(format!(
                    "NotSupportedError: AES-GCM exportKey format {other}"
                )),
            }
        }
        NativeKey::RsaVerify {
            hash,
            rsa_der,
            n,
            e,
        } => match format {
            "spki" => Ok(hex(&encode_rsa_spki(rsa_der))),
            "jwk" => {
                let alg = match hash {
                    Sha::Sha256 => "RS256",
                    Sha::Sha384 => "RS384",
                    Sha::Sha512 => "RS512",
                };
                Ok(serde_json::json!({
                    "kty": "RSA",
                    "n": base64url(n),
                    "e": base64url(e),
                    "alg": alg,
                })
                .to_string())
            }
            other => Err(format!("NotSupportedError: RSA exportKey format {other}")),
        },
        NativeKey::Ed25519 {
            seed,
            public,
            extractable,
        } => {
            if seed.is_some() && !extractable {
                return Err("InvalidAccessError: exportKey: the key is not extractable".into());
            }
            match (format, seed) {
                ("raw", None) => Ok(hex(public)),
                ("spki", None) => {
                    let mut out = Vec::with_capacity(44);
                    out.extend_from_slice(ED25519_SPKI_PREFIX);
                    out.extend_from_slice(public);
                    Ok(hex(&out))
                }
                ("pkcs8", Some(s)) => {
                    let mut out = Vec::with_capacity(48);
                    out.extend_from_slice(ED25519_PKCS8_PREFIX);
                    out.extend_from_slice(s);
                    Ok(hex(&out))
                }
                ("jwk", Some(s)) => Ok(serde_json::json!({
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "x": base64url(public),
                    "d": base64url(s),
                })
                .to_string()),
                ("jwk", None) => Ok(serde_json::json!({
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "x": base64url(public),
                })
                .to_string()),
                ("raw" | "spki", Some(_)) => {
                    Err("InvalidAccessError: cannot export a private key as raw or spki".into())
                }
                ("pkcs8", None) => {
                    Err("InvalidAccessError: cannot export a public key as pkcs8".into())
                }
                (other, _) => Err(format!(
                    "NotSupportedError: Ed25519 exportKey format {other}"
                )),
            }
        }
        NativeKey::Ec(_) => Err("InvalidAccessError: use EC export op".into()),
        NativeKey::Pbkdf2 { .. } | NativeKey::Hkdf { .. } => {
            Err("InvalidAccessError: derivation keys are not extractable".into())
        }
    }
}

fn op_sign(state: &HostState, spec: &str, data: &[u8]) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("DataError: {e}"))?;
    let handle = v["handle"]
        .as_u64()
        .ok_or_else(|| "InvalidAccessError: missing key handle".to_string())?;
    match get_key(state, handle)? {
        NativeKey::Hmac { hash, bytes, .. } => {
            let k = hmac::Key::new(hmac_alg(*hash), bytes);
            let tag = hmac::sign(&k, data);
            Ok(hex(tag.as_ref()))
        }
        NativeKey::Ed25519 { seed: Some(s), .. } => {
            let pair = Ed25519KeyPair::from_seed_unchecked(s)
                .map_err(|_| "OperationError: invalid Ed25519 key".to_string())?;
            Ok(hex(pair.sign(data).as_ref()))
        }
        NativeKey::Ed25519 { seed: None, .. } => {
            Err("InvalidAccessError: sign: a public Ed25519 key does not sign".into())
        }
        _ => Err("InvalidAccessError: key cannot be used for this sign operation".into()),
    }
}

fn op_verify(state: &HostState, spec: &str, data: &[u8]) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("DataError: {e}"))?;
    let handle = v["handle"]
        .as_u64()
        .ok_or_else(|| "InvalidAccessError: missing key handle".to_string())?;
    let sig = unhex(v["sig"].as_str().unwrap_or(""))?;
    let ok = match get_key(state, handle)? {
        NativeKey::Hmac { hash, bytes, .. } => {
            let k = hmac::Key::new(hmac_alg(*hash), bytes);
            hmac::verify(&k, data, &sig).is_ok()
        }
        NativeKey::Ed25519 { public, .. } => UnparsedPublicKey::new(&signature::ED25519, public)
            .verify(data, &sig)
            .is_ok(),
        NativeKey::RsaVerify { hash, rsa_der, .. } => {
            UnparsedPublicKey::new(rsa_alg(*hash), rsa_der)
                .verify(data, &sig)
                .is_ok()
        }
        NativeKey::Ec(ec) => ec.verify(data, &sig),
        _ => return Err("InvalidAccessError: key cannot be used for verify".into()),
    };
    Ok(if ok { "1" } else { "0" }.into())
}

fn op_cipher(state: &HostState, spec: &str, data: &[u8]) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("DataError: {e}"))?;
    let handle = v["handle"]
        .as_u64()
        .ok_or_else(|| "InvalidAccessError: missing key handle".to_string())?;
    let encrypt = v["op"].as_str() == Some("encrypt");
    let iv = unhex(v["iv"].as_str().unwrap_or(""))?;
    if iv.len() != 12 {
        return Err(
            "NotSupportedError: AES-GCM requires a 12-byte (96-bit) iv on this host".into(),
        );
    }
    let tag_len = v["tagLength"].as_u64().unwrap_or(128);
    if tag_len != 128 {
        return Err("NotSupportedError: AES-GCM supports 128-bit tagLength on this host".into());
    }
    let aad = match v["aad"].as_str() {
        Some(h) if !h.is_empty() => unhex(h)?,
        _ => Vec::new(),
    };
    let NativeKey::AesGcm { bytes, .. } = get_key(state, handle)? else {
        return Err("InvalidAccessError: key is not AES-GCM".into());
    };
    let alg = if bytes.len() == 16 {
        &aead::AES_128_GCM
    } else {
        &aead::AES_256_GCM
    };
    let unbound = UnboundKey::new(alg, bytes)
        .map_err(|_| "OperationError: invalid AES-GCM key".to_string())?;
    let key = LessSafeKey::new(unbound);
    let nonce = Nonce::try_assume_unique_for_key(&iv)
        .map_err(|_| "OperationError: invalid AES-GCM iv".to_string())?;
    let mut buf = data.to_vec();
    if encrypt {
        key.seal_in_place_append_tag(nonce, Aad::from(&aad), &mut buf)
            .map_err(|_| "OperationError: AES-GCM encryption failed".to_string())?;
        Ok(hex(&buf))
    } else {
        if buf.len() < 16 {
            return Err(
                "OperationError: AES-GCM ciphertext shorter than authentication tag".into(),
            );
        }
        let plain = key
            .open_in_place(nonce, Aad::from(&aad), &mut buf)
            .map_err(|_| "OperationError: AES-GCM decryption failed".to_string())?;
        Ok(hex(plain))
    }
}

struct ArbitraryLen(usize);
impl KeyType for ArbitraryLen {
    fn len(&self) -> usize {
        self.0
    }
}

fn op_derive(state: &mut HostState, spec: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(spec).map_err(|e| format!("DataError: {e}"))?;
    let alg = v["alg"].as_str().unwrap_or("");
    let handle = v["handle"]
        .as_u64()
        .ok_or_else(|| "InvalidAccessError: missing key handle".to_string())?;
    let hash = parse_sha(v["hash"].as_str().unwrap_or(""))?;
    let bits = v["length"]
        .as_u64()
        .ok_or_else(|| "OperationError: length is required".to_string())? as usize;
    if !bits.is_multiple_of(8) {
        return Err("OperationError: derived length must be a multiple of 8 bits".into());
    }
    let byte_len = bits / 8;
    let derived = match alg {
        "PBKDF2" => {
            let iterations = v["iterations"].as_u64().unwrap_or(0);
            let iters = u32::try_from(iterations)
                .ok()
                .and_then(NonZeroU32::new)
                .ok_or_else(|| {
                    "OperationError: PBKDF2 iterations must be at least 1".to_string()
                })?;
            let salt = unhex(v["salt"].as_str().unwrap_or(""))?;
            let NativeKey::Pbkdf2 { bytes: password } = get_key(state, handle)? else {
                return Err("InvalidAccessError: key is not a PBKDF2 base key".into());
            };
            if byte_len == 0 {
                Vec::new()
            } else {
                let mut out = vec![0u8; byte_len];
                pbkdf2::derive(pbkdf2_alg(hash), iters, &salt, password, &mut out);
                out
            }
        }
        "HKDF" => {
            let salt = unhex(v["salt"].as_str().unwrap_or(""))?;
            let info = unhex(v["info"].as_str().unwrap_or(""))?;
            let NativeKey::Hkdf { bytes: ikm } = get_key(state, handle)? else {
                return Err("InvalidAccessError: key is not an HKDF base key".into());
            };
            let max_bytes = 255 * sha_len(hash);
            if byte_len > max_bytes {
                return Err(format!(
                    "OperationError: HKDF derived length exceeds {max_bytes} bytes"
                ));
            }
            if byte_len == 0 {
                Vec::new()
            } else {
                let prk = hkdf::Salt::new(hkdf_alg(hash), &salt).extract(ikm);
                let info_slices = [info.as_slice()];
                let okm = prk
                    .expand(&info_slices, ArbitraryLen(byte_len))
                    .map_err(|_| "OperationError: HKDF expansion failed".to_string())?;
                let mut out = vec![0u8; byte_len];
                okm.fill(&mut out)
                    .map_err(|_| "OperationError: HKDF fill failed".to_string())?;
                out
            }
        }
        other => return Err(format!("NotSupportedError: derive: {other}")),
    };
    // If `target` is absent, this is `deriveBits`: return hex bytes.
    let Some(target) = v.get("target").filter(|t| !t.is_null()) else {
        return Ok(hex(&derived));
    };
    let target_alg = target["alg"].as_str().unwrap_or("");
    let extractable = target["extractable"].as_bool().unwrap_or(false);
    match target_alg {
        "HMAC" => {
            let target_hash = parse_sha(target["hash"].as_str().unwrap_or(""))?;
            if derived.is_empty() {
                return Err("OperationError: derived HMAC key cannot be empty".into());
            }
            let length = derived.len() * 8;
            let h = push_key(
                state,
                NativeKey::Hmac {
                    hash: target_hash,
                    bytes: derived,
                    extractable,
                },
            )?;
            Ok(serde_json::json!({"handle": h, "length": length}).to_string())
        }
        "AES-GCM" => {
            if derived.len() != 16 && derived.len() != 32 {
                return Err(format!(
                    "OperationError: derived AES-GCM key must be 128 or 256 bits, got {} bits",
                    derived.len() * 8
                ));
            }
            let length = derived.len() * 8;
            let h = push_key(
                state,
                NativeKey::AesGcm {
                    bytes: derived,
                    extractable,
                },
            )?;
            Ok(serde_json::json!({"handle": h, "length": length}).to_string())
        }
        other => Err(format!("NotSupportedError: deriveKey target {other}")),
    }
}
