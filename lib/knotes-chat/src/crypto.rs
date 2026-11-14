//! SHA-256 (CommonCrypto) and random bytes (the Security framework), from macOS.

use std::ffi::c_void;

use crate::base64url;

unsafe extern "C" {
    fn CC_SHA256(data: *const c_void, len: u32, md: *mut u8) -> *mut u8;
}

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    let mut digest = [0; 32];
    // SAFETY: CommonCrypto reads `len` bytes of `data` and writes 32 into `digest`.
    unsafe { CC_SHA256(data.as_ptr().cast(), data.len() as u32, digest.as_mut_ptr()) };
    digest
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    security_framework::random::SecRandom::default().copy_bytes(&mut bytes).expect("the system's random numbers");
    bytes
}

/// 32 random bytes in base64url: a PKCE verifier, a `state`, a `nonce`.
pub(crate) fn random_token() -> String {
    base64url::encode(&random_bytes::<32>())
}

/// PKCE's S256 challenge of `verifier` (RFC 7636).
pub fn pkce_challenge(verifier: &str) -> String {
    base64url::encode(&sha256(verifier.as_bytes()))
}

/// A random (version 4) UUID as a URN.
pub(crate) fn uuid_urn() -> String {
    let mut b = random_bytes::<16>();
    b[6] = b[6] & 0x0f | 0x40;
    b[8] = b[8] & 0x3f | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("urn:uuid:{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}
