//! age encryption backend: a seed-phrase-derived X25519 identity as an
//! alternative to GPG + YubiKey.
//!
//! The private key is derived from a 12-word BIP-39 mnemonic (see
//! `docs/AGE-DESIGN.md`, derivation v1 — FROZEN). Only the 32 derived key
//! bytes are retained, in an mlock'd [`SecureBuffer`]; the transient
//! `age::x25519::Identity` is rebuilt for a single operation and dropped
//! immediately. The mnemonic is never stored; callers zeroize it after
//! deriving.
//!
//! Documented residual exposure (same class as the existing GPG-pipe and
//! egui one-frame notes): reconstructing the identity briefly materializes
//! the age secret-key string in an ordinary `String`, which we zeroize as
//! soon as the identity is parsed.

use std::io::{Read, Write};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use bip39::Mnemonic;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::crypto::secure_buf::{SecureBuffer, SecureString};

// ── Frozen derivation constants (v1) — see docs/AGE-DESIGN.md ─────────
const HKDF_SALT: &[u8] = b"schl8.age.identity.v1";
const HKDF_INFO: &[u8] = b"x25519-secret-key";
/// age encodes X25519 secret keys as uppercase bech32; its HRP includes
/// the trailing hyphen (`age-secret-key-`).
const AGE_SECRET_HRP: &str = "age-secret-key-";

/// A seed-phrase-derived age identity. Holds only the 32-byte key
/// material (mlock'd, zeroize-on-drop); the age identity is transient.
pub struct AgeIdentity {
    /// 32 bytes of HKDF output (the X25519 secret scalar).
    key: SecureBuffer,
    /// The public recipient (`age1…`) — public, safe to keep as a String.
    recipient: String,
}

impl AgeIdentity {
    /// Derive an identity from a BIP-39 mnemonic and optional passphrase.
    /// Validates the mnemonic (checksum) — invalid input is rejected.
    pub fn from_mnemonic(mnemonic: &str, passphrase: &str) -> Result<Self> {
        let key = derive_key(mnemonic, passphrase)?;
        let recipient = recipient_of(key.as_bytes())?;
        Ok(Self { key, recipient })
    }

    /// The public recipient string (`age1…`).
    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    /// Duplicate the identity (copies the 32 key bytes into a fresh mlock'd
    /// buffer). Used to hand a copy to a background thread (e.g. the
    /// quicknote append) without sharing the app's held identity.
    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            key: SecureBuffer::from_bytes(self.key.as_bytes().to_vec()),
            recipient: self.recipient.clone(),
        })
    }

    /// Rebuild the transient age identity for a single operation. The
    /// intermediate secret string is zeroized before returning.
    fn to_age(&self) -> Result<age::x25519::Identity> {
        let mut secret = secret_string(self.key.as_bytes())?;
        let identity = age::x25519::Identity::from_str(&secret)
            .map_err(|e| anyhow!("invalid derived identity: {e}"));
        secret.zeroize();
        identity
    }

    /// Decrypt age ciphertext with this identity. Plaintext lands in a
    /// `SecureBuffer` (mlock'd), like the GPG path.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<SecureBuffer> {
        let identity = self.to_age()?;
        let decryptor = age::Decryptor::new(ciphertext).context("not a valid age file")?;
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| anyhow!("decryption failed (wrong seed phrase?): {e}"))?;
        let mut plaintext = Vec::new();
        reader
            .read_to_end(&mut plaintext)
            .context("reading decrypted age stream")?;
        Ok(SecureBuffer::from_bytes(plaintext))
    }
}

/// Derive the 32-byte X25519 secret from a mnemonic (+ passphrase) into a
/// `SecureBuffer`. The transient BIP-39 seed is zeroized.
fn derive_key(mnemonic: &str, passphrase: &str) -> Result<SecureBuffer> {
    let m = Mnemonic::from_str(mnemonic.trim())
        .map_err(|e| anyhow!("not a valid BIP-39 phrase: {e}"))?;
    if m.word_count() != 12 {
        anyhow::bail!("expected a 12-word phrase, got {} words", m.word_count());
    }
    let mut seed = m.to_seed(passphrase); // [u8; 64]
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), &seed);
    let mut okm = vec![0u8; 32];
    let expand = hk.expand(HKDF_INFO, &mut okm);
    seed.zeroize();
    expand.map_err(|e| anyhow!("hkdf expand: {e}"))?;
    Ok(SecureBuffer::from_bytes(okm)) // okm moved in; from_bytes zeroizes the source
}

/// The uppercase age secret-key string for 32 key bytes.
fn secret_string(key: &[u8]) -> Result<String> {
    let hrp = bech32::Hrp::parse(AGE_SECRET_HRP).map_err(|e| anyhow!("hrp: {e}"))?;
    let lower =
        bech32::encode::<bech32::Bech32>(hrp, key).map_err(|e| anyhow!("bech32 encode: {e}"))?;
    Ok(lower.to_uppercase())
}

/// The public recipient (`age1…`) for 32 key bytes.
fn recipient_of(key: &[u8]) -> Result<String> {
    let mut secret = secret_string(key)?;
    let identity = age::x25519::Identity::from_str(&secret);
    secret.zeroize();
    let identity = identity.map_err(|e| anyhow!("invalid derived identity: {e}"))?;
    Ok(identity.to_public().to_string())
}

/// Derive just the public recipient from a mnemonic — the "export public
/// key" path. The private key material is dropped before returning.
pub fn recipient_from_mnemonic(mnemonic: &str, passphrase: &str) -> Result<String> {
    let key = derive_key(mnemonic, passphrase)?;
    recipient_of(key.as_bytes())
}

/// Validate a 12-word BIP-39 mnemonic (checksum) without deriving.
// Wired in the generate-new-phrase flow (see docs/AGE-DESIGN.md); kept
// tested now so the frozen validation lands with the rest of v1.
#[allow(dead_code)]
pub fn validate_mnemonic(mnemonic: &str) -> Result<()> {
    let m = Mnemonic::from_str(mnemonic.trim())
        .map_err(|e| anyhow!("not a valid BIP-39 phrase: {e}"))?;
    if m.word_count() != 12 {
        anyhow::bail!("expected a 12-word phrase, got {} words", m.word_count());
    }
    Ok(())
}

/// Generate a fresh 12-word BIP-39 mnemonic, mixing the OS CSPRNG with
/// optional user-supplied entropy.
///
/// The 128-bit BIP-39 entropy is `HKDF-SHA256(salt = domain tag,
/// ikm = 32 OS-random bytes ‖ user_entropy)`. Because a full 32 bytes of
/// `getrandom` are always mixed in, the result is never weaker than the
/// CSPRNG alone — user entropy is pure defense-in-depth and cannot lower
/// the strength, even if empty or low-quality.
///
/// The mnemonic is returned in a mlock'd, zeroizing [`SecureString`] and
/// never touches disk. Residual exposure: `Mnemonic::to_string` briefly
/// materializes the phrase in an ordinary `String`, which is moved (not
/// copied) into the SecureString and the source buffer consumed.
pub fn generate_mnemonic_with_entropy(user_entropy: &[u8]) -> Result<SecureString> {
    let mut os_seed = [0u8; 32];
    getrandom::getrandom(&mut os_seed).map_err(|e| anyhow!("OS randomness unavailable: {e}"))?;

    let mut ikm = Vec::with_capacity(os_seed.len() + user_entropy.len());
    ikm.extend_from_slice(&os_seed);
    ikm.extend_from_slice(user_entropy);

    let hk = Hkdf::<Sha256>::new(Some(b"schl8.age.mnemonic.v1"), &ikm);
    let mut entropy = [0u8; 16];
    hk.expand(b"bip39-entropy", &mut entropy)
        .map_err(|_| anyhow!("HKDF expand failed"))?;

    let mnemonic =
        Mnemonic::from_entropy(&entropy).map_err(|e| anyhow!("mnemonic construction: {e}"))?;
    // Move the phrase bytes straight into an mlock'd buffer; `into_bytes`
    // consumes the String so no unlocked copy survives.
    let phrase = mnemonic.to_string();
    let buf = SecureBuffer::from_bytes(phrase.into_bytes());
    let secure = SecureString::from_secure_buffer(&buf)?;

    os_seed.zeroize();
    ikm.zeroize();
    entropy.zeroize();
    // `mnemonic` and `buf` are dropped here; bip39's zeroize feature wipes
    // the Mnemonic and SecureBuffer zeroizes on drop.
    Ok(secure)
}

/// Estimate the entropy (in bits) of user-typed randomness: the per-byte
/// Shannon entropy of the sample multiplied by its length. Repetitive
/// input (`"aaaa"`) scores near zero; diverse input scores higher. This is
/// a rough UI guide only — the generated key always also draws 256 bits
/// from the OS CSPRNG, so the true key strength is never below 128 bits.
pub fn estimate_entropy_bits(input: &[u8]) -> f64 {
    if input.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for &b in input {
        counts[b as usize] += 1;
    }
    let len = input.len() as f64;
    let mut per_byte = 0.0;
    for &c in counts.iter() {
        if c == 0 {
            continue;
        }
        let p = f64::from(c) / len;
        per_byte -= p * p.log2();
    }
    per_byte * len
}

/// Validate an age recipient string (`age1…`).
pub fn validate_recipient(s: &str) -> Result<()> {
    age::x25519::Recipient::from_str(s.trim())
        .map(|_| ())
        .map_err(|e| anyhow!("not a valid age recipient: {e}"))
}

/// Encrypt `plaintext` to one or more age recipients (`age1…` strings).
pub fn encrypt_to_recipients(plaintext: &[u8], recipients: &[&str]) -> Result<Vec<u8>> {
    if recipients.is_empty() {
        anyhow::bail!("no age recipients given");
    }
    let parsed: Vec<age::x25519::Recipient> = recipients
        .iter()
        .map(|r| {
            age::x25519::Recipient::from_str(r.trim())
                .map_err(|e| anyhow!("invalid age recipient {r}: {e}"))
        })
        .collect::<Result<_>>()?;
    let recips = parsed.iter().map(|r| r as &dyn age::Recipient);
    let encryptor = age::Encryptor::with_recipients(recips).context("building age encryptor")?;
    let mut out = Vec::new();
    let mut w = encryptor
        .wrap_output(&mut out)
        .context("starting age encryption")?;
    w.write_all(plaintext).context("writing age plaintext")?;
    w.finish().context("finishing age encryption")?;
    Ok(out)
}

/// Whether bytes look like an age file (binary header or ASCII armor).
pub fn looks_like_age(bytes: &[u8]) -> bool {
    bytes.starts_with(b"age-encryption.org/v1")
        || bytes.starts_with(b"-----BEGIN AGE ENCRYPTED FILE-----")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MNEMONIC_A: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const MNEMONIC_C: &str =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";

    // Vectors from docs/AGE-DESIGN.md §3.1 (round-trip-verified). If the
    // derivation ever changes, these fail — which is the point. They are
    // Schl8's own: the salt differs from Schlate's, so the same phrase
    // yields a different identity and files are not interchangeable.
    const RECIPIENT_A: &str = "age14zee60n7ltlay00sn00h5z93wk7c8d90eu42v7hydmp90acl69wqj909lk";
    const RECIPIENT_A_TREZOR: &str =
        "age1rz7d8m40mr8va447gumdu5k0rdsu7q4rpwp7gawqayzzs5j4lelshll4ys";
    const RECIPIENT_C: &str = "age1qtuv8xp8k75vqjf9z7cgs0tg6ulm2hhte2f3h438mkmw8jkkmexsae4n3k";

    #[test]
    fn derivation_matches_frozen_vectors() {
        assert_eq!(
            recipient_from_mnemonic(MNEMONIC_A, "").unwrap(),
            RECIPIENT_A
        );
        assert_eq!(
            recipient_from_mnemonic(MNEMONIC_A, "TREZOR").unwrap(),
            RECIPIENT_A_TREZOR
        );
        assert_eq!(
            recipient_from_mnemonic(MNEMONIC_C, "").unwrap(),
            RECIPIENT_C
        );
    }

    #[test]
    fn identity_recipient_is_stable() {
        let id = AgeIdentity::from_mnemonic(MNEMONIC_A, "").unwrap();
        assert_eq!(id.recipient(), RECIPIENT_A);
        // Same phrase → same recipient, every time.
        let id2 = AgeIdentity::from_mnemonic(MNEMONIC_A, "").unwrap();
        assert_eq!(id.recipient(), id2.recipient());
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let id = AgeIdentity::from_mnemonic(MNEMONIC_A, "").unwrap();
        let msg = b"secret note: the vault code is 1234";
        let ct = encrypt_to_recipients(msg, &[id.recipient()]).unwrap();
        assert!(looks_like_age(&ct), "output should be an age file");
        let pt = id.decrypt(&ct).unwrap();
        assert_eq!(pt.as_bytes(), msg);
    }

    #[test]
    fn wrong_phrase_cannot_decrypt() {
        let owner = AgeIdentity::from_mnemonic(MNEMONIC_A, "").unwrap();
        let ct = encrypt_to_recipients(b"for owner only", &[owner.recipient()]).unwrap();
        let stranger = AgeIdentity::from_mnemonic(MNEMONIC_C, "").unwrap();
        assert!(stranger.decrypt(&ct).is_err(), "stranger must not decrypt");
        // The passphrase variant is a different identity too.
        let with_pass = AgeIdentity::from_mnemonic(MNEMONIC_A, "TREZOR").unwrap();
        assert!(with_pass.decrypt(&ct).is_err());
    }

    #[test]
    fn encrypt_to_multiple_recipients() {
        let a = AgeIdentity::from_mnemonic(MNEMONIC_A, "").unwrap();
        let c = AgeIdentity::from_mnemonic(MNEMONIC_C, "").unwrap();
        let ct = encrypt_to_recipients(b"shared", &[a.recipient(), c.recipient()]).unwrap();
        assert_eq!(a.decrypt(&ct).unwrap().as_bytes(), b"shared");
        assert_eq!(c.decrypt(&ct).unwrap().as_bytes(), b"shared");
    }

    #[test]
    fn rejects_invalid_mnemonic() {
        assert!(validate_mnemonic("zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo").is_err());
        assert!(validate_mnemonic("only three words here").is_err());
        assert!(AgeIdentity::from_mnemonic("not a phrase", "").is_err());
    }

    #[test]
    fn generated_mnemonic_is_valid_and_usable() {
        let m = generate_mnemonic_with_entropy(b"some user entropy here").unwrap();
        assert_eq!(m.as_str().split_whitespace().count(), 12);
        validate_mnemonic(m.as_str()).unwrap();
        let id = AgeIdentity::from_mnemonic(m.as_str(), "").unwrap();
        assert!(id.recipient().starts_with("age1"));
    }

    #[test]
    fn generated_mnemonics_are_unique_and_work_with_empty_entropy() {
        // Even with no user entropy, the OS CSPRNG yields a full-strength,
        // distinct 12-word phrase each call.
        let a = generate_mnemonic_with_entropy(&[]).unwrap();
        let b = generate_mnemonic_with_entropy(&[]).unwrap();
        assert_eq!(a.as_str().split_whitespace().count(), 12);
        assert_ne!(a.as_str(), b.as_str());
        validate_mnemonic(a.as_str()).unwrap();
    }

    #[test]
    fn entropy_estimate_rewards_diversity_and_penalizes_repetition() {
        assert_eq!(estimate_entropy_bits(b""), 0.0);
        // A single repeated byte carries no per-byte entropy.
        assert_eq!(estimate_entropy_bits(b"aaaaaaaa"), 0.0);
        // Diverse input scores meaningfully higher than repetitive input.
        let diverse = estimate_entropy_bits(b"correct horse battery staple 7#z");
        let repeat = estimate_entropy_bits(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert!(diverse > repeat);
        assert!(diverse > 64.0);
    }

    #[test]
    fn recipient_validation() {
        assert!(validate_recipient(RECIPIENT_A).is_ok());
        assert!(validate_recipient("age1notreal").is_err());
        assert!(validate_recipient("").is_err());
    }
}
