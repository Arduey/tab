//! 密码哈希、会话、时间等安全工具。

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}

pub fn hash_password(pw: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    let h = argon2::Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(h.to_string())
}

pub fn verify_password(pw: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    argon2::Argon2::default()
        .verify_password(pw.as_bytes(), &parsed)
        .is_ok()
}

/// Argon2 是 CPU 密集的：直接在 async 上下文里跑会占住 tokio worker，
/// 登录/改密的那几十毫秒里其他请求都会被拖慢。丢给阻塞线程池。
pub async fn hash_password_async(pw: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&pw))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
}

pub async fn verify_password_async(pw: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&pw, &hash))
        .await
        .unwrap_or(false)
}