//! Ed25519 verification of the exact PE32+ kernel file before it is loaded.
use ed25519_dalek::{Signature, VerifyingKey, Verifier};

pub fn verify_kernel_signature(image: &[u8]) -> bool {
    let (Some(public_key), Some(signature)) = (
        crate::EXPECTED_SIGNING_PUBLIC_KEY,
        crate::EXPECTED_KERNEL_SIGNATURE,
    ) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&public_key) else {
        return false;
    };
    key.verify_strict(image, &Signature::from_bytes(&signature)).is_ok()
}
