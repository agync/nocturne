//! The lock: one key per backup, kept in the account only locked with the person's phrase and,
//! when they made one, with a recovery key. A new phrase locks the same key again: an old copy of
//! `key-phrase` the account kept still opens with the old phrase, so a place removes earlier
//! versions of a file it replaces.
//!
//! The key is an age X25519 identity. Every piece and every night is its own age file to it, so
//! each has a fresh random file key and no nonce is ever used twice. Age does not prove who wrote
//! a file, so each night also carries an HMAC-SHA256 under a second key derived from the first,
//! over its name and body: a night that was not written with the key does not open. The phrase
//! and the recovery key lock the key with age's scrypt passphrase sealing, 0013's, whose salt and
//! work factor are written in each locked key file's header.

use std::io::Read;
use std::str::FromStr;

use age::secrecy::{ExposeSecret, SecretString};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;

use super::NightError;

/// The key locked with the phrase.
pub const KEY_PHRASE: &str = "key-phrase";
/// The key locked with the recovery key.
pub const KEY_RECOVERY: &str = "key-recovery";
/// How many words Nocturne's own phrase has.
pub const PHRASE_WORDS: usize = 6;
/// The letters and digits of a recovery key: no 0, 1, I, L, O or U, which read alike.
const RECOVERY_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const RECOVERY_GROUPS: usize = 6;
const RECOVERY_GROUP: usize = 4;
const KEY_FORMAT: u32 = 1;
/// The largest locked key file read.
const MAX_KEY_FILE: usize = 64 * 1024;

/// The key that locks one backup. Its debug form says nothing.
pub struct NightKey {
    identity: age::x25519::Identity,
    mac: [u8; 32],
    gear: [u8; 32],
}

impl std::fmt::Debug for NightKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NightKey(..)")
    }
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC takes any key");
    for p in parts {
        mac.update(p);
    }
    mac.finalize().into_bytes().into()
}

impl NightKey {
    /// A new random key.
    pub fn generate() -> Self {
        Self::from_identity(age::x25519::Identity::generate())
    }

    fn from_identity(identity: age::x25519::Identity) -> Self {
        let secret = identity.to_string();
        let bytes = secret.expose_secret().as_bytes();
        NightKey {
            mac: hmac(bytes, &[b"nocturne nightly index v1"]),
            gear: hmac(bytes, &[b"nocturne nightly pieces v1"]),
            identity,
        }
    }

    /// The key as text, for the phone's safe place only.
    pub fn secret(&self) -> SecretString {
        self.identity.to_string()
    }

    /// The key from its text in the phone's safe place.
    pub fn from_secret(text: &str) -> Result<Self, NightError> {
        age::x25519::Identity::from_str(text.trim())
            .map(Self::from_identity)
            .map_err(|_| NightError::Damaged)
    }

    /// Whether `other` is the same key.
    pub fn same_as(&self, other: &NightKey) -> bool {
        self.mac == other.mac
    }

    pub(crate) fn lock(&self, plain: &[u8]) -> Result<Vec<u8>, NightError> {
        age::encrypt(&self.identity.to_public(), plain)
            .map_err(|e| NightError::Failed(format!("a piece could not be locked: {e}")))
    }

    /// Open a piece or night locked with this key; anything else is damage.
    pub(crate) fn unlock(&self, locked: &[u8]) -> Result<Vec<u8>, NightError> {
        age::decrypt(&self.identity, locked).map_err(|_| NightError::Damaged)
    }

    pub(crate) fn mac(&self, parts: &[&[u8]]) -> [u8; 32] {
        hmac(&self.mac, parts)
    }

    /// The secret that places piece boundaries, so their sizes say nothing of a known file.
    pub(crate) fn gear_seed(&self) -> &[u8; 32] {
        &self.gear
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wrapped {
    format: u32,
    key: String,
}

/// A phrase as it is used: one Unicode form, lower case, single spaces, so the same words typed
/// on another keyboard, or with a capital the keyboard added, open the same backup.
pub fn normalise_phrase(phrase: &str) -> String {
    phrase
        .nfc()
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A recovery key as it is used, in its six groups, or `None` when it cannot be one.
pub fn normalise_recovery_key(text: &str) -> Option<String> {
    let chars: Vec<u8> = text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'-')
        .map(|b| b.to_ascii_uppercase())
        .collect();
    if chars.len() != RECOVERY_GROUPS * RECOVERY_GROUP
        || !chars.iter().all(|c| RECOVERY_ALPHABET.contains(c))
    {
        return None;
    }
    let groups: Vec<String> = chars
        .chunks(RECOVERY_GROUP)
        .map(|g| String::from_utf8_lossy(g).into_owned())
        .collect();
    Some(groups.join("-"))
}

/// A uniform random number below `n`.
fn below(n: usize) -> usize {
    let n = n as u32;
    let zone = u32::MAX - (u32::MAX % n);
    loop {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).expect("the operating system provides random bytes");
        let v = u32::from_le_bytes(b);
        if v < zone {
            return (v % n) as usize;
        }
    }
}

/// Nocturne's own phrase: six words drawn at random from `words`.
pub fn make_phrase(words: &[&str]) -> String {
    assert!(words.len() >= 2, "a phrase needs a list of words");
    (0..PHRASE_WORDS)
        .map(|_| words[below(words.len())])
        .collect::<Vec<_>>()
        .join(" ")
}

/// A new recovery key: six groups of four letters and digits, about 118 bits.
pub fn make_recovery_key() -> String {
    (0..RECOVERY_GROUPS)
        .map(|_| {
            (0..RECOVERY_GROUP)
                .map(|_| RECOVERY_ALPHABET[below(RECOVERY_ALPHABET.len())] as char)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// The key, locked with `secret` (a normalised phrase or recovery key).
pub(crate) fn wrap(key: &NightKey, secret: &str, work_factor: u8) -> Result<Vec<u8>, NightError> {
    let plain = serde_json::to_vec(&Wrapped {
        format: KEY_FORMAT,
        key: key.secret().expose_secret().to_string(),
    })
    .map_err(|e| NightError::Failed(e.to_string()))?;
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(secret.to_string()));
    recipient.set_work_factor(work_factor);
    age::encrypt(&recipient, &plain)
        .map_err(|e| NightError::Failed(format!("the key could not be locked: {e}")))
}

/// The key from a locked key file and the `secret` that locked it. A wrong secret is
/// [`NightError::WrongPhrase`], never a key.
pub(crate) fn unwrap(locked: &[u8], secret: &str) -> Result<NightKey, NightError> {
    if locked.len() > MAX_KEY_FILE {
        return Err(NightError::Damaged);
    }
    let decryptor = age::Decryptor::new_buffered(locked).map_err(|_| NightError::Damaged)?;
    if !decryptor.is_scrypt() {
        return Err(NightError::Damaged);
    }
    let mut identity = age::scrypt::Identity::new(SecretString::from(secret.to_string()));
    // Nocturne writes this work and asks no more of a phone, whose memory is small.
    identity.set_max_work_factor(crate::backup::WORK_FACTOR);
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| match e {
            age::DecryptError::DecryptionFailed
            | age::DecryptError::NoMatchingKeys
            | age::DecryptError::KeyDecryptionFailed => NightError::WrongPhrase,
            _ => NightError::Damaged,
        })?;
    let mut plain = Vec::new();
    reader
        .read_to_end(&mut plain)
        .map_err(|_| NightError::Damaged)?;
    let wrapped: Wrapped = serde_json::from_slice(&plain).map_err(|_| NightError::Damaged)?;
    if wrapped.format > KEY_FORMAT {
        return Err(NightError::TooNew);
    }
    NightKey::from_secret(&wrapped.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phrase_reads_the_same_however_it_was_typed() {
        assert_eq!(
            normalise_phrase("  Harbour  violet\tpencil "),
            "harbour violet pencil"
        );
        assert_eq!(normalise_phrase("cafe\u{301}"), normalise_phrase("café"));
    }

    #[test]
    fn nocturnes_phrase_has_six_words_from_the_list() {
        let words = ["harbour", "violet", "pencil", "maple"];
        let phrase = make_phrase(&words);
        let got: Vec<&str> = phrase.split(' ').collect();
        assert_eq!(got.len(), PHRASE_WORDS);
        assert!(got.iter().all(|w| words.contains(w)));
    }

    #[test]
    fn a_recovery_key_is_six_groups_and_reads_back_however_typed() {
        let key = make_recovery_key();
        assert_eq!(key.len(), 6 * 4 + 5);
        assert_eq!(normalise_recovery_key(&key).as_deref(), Some(key.as_str()));
        let typed = key.to_lowercase().replace('-', " ");
        assert_eq!(
            normalise_recovery_key(&typed).as_deref(),
            Some(key.as_str())
        );
        assert_eq!(normalise_recovery_key("ABCD-EFGH"), None);
        assert_eq!(
            normalise_recovery_key("0000-0000-0000-0000-0000-0000"),
            None
        );
    }

    #[test]
    fn a_locked_key_opens_with_its_phrase_only() {
        let key = NightKey::generate();
        let locked = wrap(&key, "harbour violet pencil", 4).unwrap();
        assert!(
            unwrap(&locked, "harbour violet pencil")
                .unwrap()
                .same_as(&key)
        );
        assert!(matches!(
            unwrap(&locked, "harbour violet pencils"),
            Err(NightError::WrongPhrase)
        ));
        let mut cut = locked.clone();
        cut.truncate(cut.len() - 1);
        assert!(unwrap(&cut, "harbour violet pencil").is_err());
    }

    #[test]
    fn the_key_text_round_trips_and_its_debug_form_says_nothing() {
        let key = NightKey::generate();
        let again = NightKey::from_secret(key.secret().expose_secret()).unwrap();
        assert!(again.same_as(&key));
        assert_eq!(format!("{key:?}"), "NightKey(..)");
        assert!(NightKey::from_secret("not a key").is_err());
    }
}
