use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as ExchangePublicKey, StaticSecret};
use zeroize::Zeroizing;

use super::SyncError;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvelopeHeader {
    pub protocol_version: u32,
    pub group_id: String,
    pub device_id: String,
    pub sequence: u64,
    pub key_epoch: u32,
    pub payload_kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub header: EnvelopeHeader,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
    pub ciphertext_sha256: String,
    pub signature_b64: String,
}

#[derive(Debug)]
pub struct DeviceKeyMaterial {
    pub signing_secret: Zeroizing<Vec<u8>>,
    pub signing_public_b64: String,
    pub exchange_secret: Zeroizing<Vec<u8>>,
    pub exchange_public_b64: String,
    pub fingerprint: String,
}

pub fn generate_device_keys() -> DeviceKeyMaterial {
    let signing = SigningKey::generate(&mut OsRng);
    let exchange = StaticSecret::random_from_rng(OsRng);
    let exchange_public = ExchangePublicKey::from(&exchange);
    let exchange_secret = exchange.to_bytes();
    let signing_public = signing.verifying_key().to_bytes();
    let fingerprint = fingerprint(&signing_public, exchange_public.as_bytes());
    DeviceKeyMaterial {
        signing_secret: Zeroizing::new(signing.to_bytes().to_vec()),
        signing_public_b64: B64.encode(signing_public),
        exchange_secret: Zeroizing::new(exchange_secret.to_vec()),
        exchange_public_b64: B64.encode(exchange_public.as_bytes()),
        fingerprint,
    }
}

pub fn generate_group_key() -> Zeroizing<Vec<u8>> {
    let mut key = vec![0_u8; 32];
    OsRng.fill_bytes(&mut key);
    Zeroizing::new(key)
}

pub fn sign_detached(signing_secret: &[u8], message: &[u8]) -> Result<String, SyncError> {
    let secret: [u8; 32] = signing_secret
        .try_into()
        .map_err(|_| SyncError::Crypto("设备签名私钥长度错误".to_string()))?;
    let signing = SigningKey::from_bytes(&secret);
    Ok(B64.encode(signing.sign(message).to_bytes()))
}

pub fn verify_detached(
    signing_public_b64: &str,
    message: &[u8],
    signature_b64: &str,
) -> Result<(), SyncError> {
    let public = decode_fixed::<32>(signing_public_b64, "签名公钥")?;
    let signature = decode_fixed::<64>(signature_b64, "签名")?;
    let verifying = VerifyingKey::from_bytes(&public)
        .map_err(|_| SyncError::Crypto("设备签名公钥无效".to_string()))?;
    verifying
        .verify(message, &Signature::from_bytes(&signature))
        .map_err(|_| SyncError::Integrity("签名验证失败".to_string()))
}

pub fn derive_exchange_key(
    exchange_secret: &[u8],
    peer_public_b64: &str,
    context: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SyncError> {
    let secret = decode_slice_fixed::<32>(exchange_secret, "交换私钥")?;
    let peer = decode_fixed::<32>(peer_public_b64, "交换公钥")?;
    let secret = StaticSecret::from(secret);
    let peer = ExchangePublicKey::from(peer);
    let shared = secret.diffie_hellman(&peer);
    let mut digest = Sha256::new();
    digest.update(b"fanglv-caseboard-device-sync-x25519-v1");
    digest.update(context);
    digest.update(shared.as_bytes());
    Ok(Zeroizing::new(digest.finalize().to_vec()))
}

pub fn seal(
    header: EnvelopeHeader,
    plaintext: &[u8],
    group_key: &[u8],
    signing_secret: &[u8],
) -> Result<EncryptedEnvelope, SyncError> {
    validate_symmetric_key(group_key)?;
    let signing_bytes: [u8; 32] = signing_secret
        .try_into()
        .map_err(|_| SyncError::Crypto("设备签名私钥长度错误".to_string()))?;
    let signing = SigningKey::from_bytes(&signing_bytes);
    let cipher = Aes256Gcm::new_from_slice(group_key)
        .map_err(|_| SyncError::Crypto("无法初始化 AES-256-GCM".to_string()))?;
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| SyncError::Crypto("信封加密失败".to_string()))?;
    let ciphertext_sha256 = sha256_hex(&ciphertext);
    let signing_bytes = signing_payload(&header, &nonce_bytes, &ciphertext)?;
    let signature = signing.sign(&signing_bytes);
    Ok(EncryptedEnvelope {
        header,
        nonce_b64: B64.encode(nonce_bytes),
        ciphertext_b64: B64.encode(ciphertext),
        ciphertext_sha256,
        signature_b64: B64.encode(signature.to_bytes()),
    })
}

pub fn open(
    envelope: &EncryptedEnvelope,
    group_key: &[u8],
    expected_signing_public_b64: &str,
) -> Result<Vec<u8>, SyncError> {
    if envelope.header.protocol_version != PROTOCOL_VERSION {
        return Err(SyncError::Protocol(format!(
            "不支持协议版本 {}",
            envelope.header.protocol_version
        )));
    }
    validate_symmetric_key(group_key)?;
    let nonce = decode_fixed::<12>(&envelope.nonce_b64, "nonce")?;
    let ciphertext = B64
        .decode(&envelope.ciphertext_b64)
        .map_err(|_| SyncError::Crypto("信封密文不是有效 Base64".to_string()))?;
    if sha256_hex(&ciphertext) != envelope.ciphertext_sha256 {
        return Err(SyncError::Integrity("密文哈希不匹配".to_string()));
    }
    let public = decode_fixed::<32>(expected_signing_public_b64, "签名公钥")?;
    let verifying = VerifyingKey::from_bytes(&public)
        .map_err(|_| SyncError::Crypto("设备签名公钥无效".to_string()))?;
    let signature_bytes = decode_fixed::<64>(&envelope.signature_b64, "签名")?;
    let signature = Signature::from_bytes(&signature_bytes);
    let signed = signing_payload(&envelope.header, &nonce, &ciphertext)?;
    verifying
        .verify(&signed, &signature)
        .map_err(|_| SyncError::Integrity("信封签名验证失败".to_string()))?;
    let cipher = Aes256Gcm::new_from_slice(group_key)
        .map_err(|_| SyncError::Crypto("无法初始化 AES-256-GCM".to_string()))?;
    cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| SyncError::Integrity("信封认证或解密失败".to_string()))
}

pub fn sha256_hex(input: &[u8]) -> String {
    Sha256::digest(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_symmetric_key(key: &[u8]) -> Result<(), SyncError> {
    if key.len() != 32 {
        return Err(SyncError::Crypto(
            "同步组数据密钥必须为 32 字节".to_string(),
        ));
    }
    Ok(())
}

fn decode_fixed<const N: usize>(value: &str, label: &str) -> Result<[u8; N], SyncError> {
    let decoded = B64
        .decode(value)
        .map_err(|_| SyncError::Crypto(format!("{label} 不是有效 Base64")))?;
    decoded
        .try_into()
        .map_err(|_| SyncError::Crypto(format!("{label} 长度错误")))
}

fn decode_slice_fixed<const N: usize>(value: &[u8], label: &str) -> Result<[u8; N], SyncError> {
    value
        .try_into()
        .map_err(|_| SyncError::Crypto(format!("{label} 长度错误")))
}

fn signing_payload(
    header: &EnvelopeHeader,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, SyncError> {
    let mut bytes =
        serde_json::to_vec(header).map_err(|error| SyncError::Serialization(error.to_string()))?;
    bytes.extend_from_slice(nonce);
    bytes.extend_from_slice(ciphertext);
    Ok(bytes)
}

fn fingerprint(signing_public: &[u8], exchange_public: &[u8]) -> String {
    let mut input = Vec::with_capacity(signing_public.len() + exchange_public.len());
    input.extend_from_slice(signing_public);
    input.extend_from_slice(exchange_public);
    let digest = sha256_hex(&input);
    digest
        .as_bytes()
        .chunks(4)
        .take(8)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> EnvelopeHeader {
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: "g1".to_string(),
            device_id: "d1".to_string(),
            sequence: 1,
            key_epoch: 1,
            payload_kind: "operations".to_string(),
            created_at: "2026-07-29T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn encrypted_envelope_roundtrip_and_tamper_rejection() {
        let device = generate_device_keys();
        let key = generate_group_key();
        let mut envelope = seal(
            header(),
            b"sensitive-case-data",
            &key,
            &device.signing_secret,
        )
        .unwrap();
        assert_eq!(
            open(&envelope, &key, &device.signing_public_b64).unwrap(),
            b"sensitive-case-data"
        );

        envelope.ciphertext_sha256.replace_range(0..1, "f");
        assert!(open(&envelope, &key, &device.signing_public_b64).is_err());
    }

    #[test]
    fn x25519_exchange_derives_the_same_wrapping_key() {
        let first = generate_device_keys();
        let second = generate_device_keys();
        let first_key = derive_exchange_key(
            &first.exchange_secret,
            &second.exchange_public_b64,
            b"invite",
        )
        .unwrap();
        let second_key = derive_exchange_key(
            &second.exchange_secret,
            &first.exchange_public_b64,
            b"invite",
        )
        .unwrap();
        assert_eq!(first_key.as_slice(), second_key.as_slice());
    }
}
