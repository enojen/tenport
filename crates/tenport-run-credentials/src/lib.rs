use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

#[cfg(test)]
mod tests;

pub const PLACEHOLDER_PREFIX: &str = "tbxph_";

pub fn run_placeholder(key: &str, job_uid: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC takes a key of any length");
    mac.update(job_uid.as_bytes());
    let digest = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{PLACEHOLDER_PREFIX}{digest}")
}
