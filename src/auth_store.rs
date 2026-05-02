use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::Utc;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

const AUTH_PASSPHRASE_ENV: &str = "TENGU_AUTH_PASSPHRASE";
const PBKDF2_ITERATIONS: u32 = 210_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub provider: String,
    pub env_var: String,
    pub token_store: String,
    pub encrypted: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenStore {
    version: u8,
    kdf: String,
    cipher: String,
    updated_at: String,
    tokens: BTreeMap<String, EncryptedToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedToken {
    env_var: String,
    salt: String,
    nonce: String,
    ciphertext: String,
    updated_at: String,
}

pub fn passphrase_env_var() -> &'static str {
    AUTH_PASSPHRASE_ENV
}

pub fn auth_dir_from_home(home: &Path) -> PathBuf {
    home.join(".tengu").join("auth")
}

pub fn session_path_from_home(home: &Path) -> PathBuf {
    auth_dir_from_home(home).join("session.json")
}

pub fn token_store_path_from_home(home: &Path) -> PathBuf {
    auth_dir_from_home(home).join("tokens.json")
}

pub fn default_session_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| anyhow!("HOME not set"))?;
    Ok(session_path_from_home(Path::new(&home)))
}

pub fn default_token_store_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| anyhow!("HOME not set"))?;
    Ok(token_store_path_from_home(Path::new(&home)))
}

pub fn save_login(provider: &str, env_var: &str, token: &str) -> Result<AuthSession> {
    let session_path = default_session_path()?;
    let token_store_path = default_token_store_path()?;
    save_login_at(&session_path, &token_store_path, provider, env_var, token)
}

pub fn save_login_at(
    session_path: &Path,
    token_store_path: &Path,
    provider: &str,
    env_var: &str,
    token: &str,
) -> Result<AuthSession> {
    let passphrase = std::env::var(AUTH_PASSPHRASE_ENV)
        .map_err(|_| anyhow!("{AUTH_PASSPHRASE_ENV} is not set"))?;
    save_login_at_with_passphrase(
        session_path,
        token_store_path,
        provider,
        env_var,
        token,
        passphrase.as_bytes(),
    )
}

pub(crate) fn save_login_at_with_passphrase(
    session_path: &Path,
    token_store_path: &Path,
    provider: &str,
    env_var: &str,
    token: &str,
    passphrase: &[u8],
) -> Result<AuthSession> {
    let encrypted = encrypt_token(token.as_bytes(), passphrase)?;
    let now = Utc::now().to_rfc3339();

    let mut store = load_token_store(token_store_path).unwrap_or_else(|| TokenStore {
        version: 1,
        kdf: format!("PBKDF2-HMAC-SHA256:{PBKDF2_ITERATIONS}"),
        cipher: "AES-256-GCM".to_string(),
        updated_at: now.clone(),
        tokens: BTreeMap::new(),
    });
    store.updated_at = now.clone();
    store.tokens.insert(
        provider.to_string(),
        EncryptedToken {
            env_var: env_var.to_string(),
            salt: encrypted.salt,
            nonce: encrypted.nonce,
            ciphertext: encrypted.ciphertext,
            updated_at: now.clone(),
        },
    );
    write_private_file(token_store_path, &serde_json::to_vec_pretty(&store)?)?;

    let session = AuthSession {
        provider: provider.to_string(),
        env_var: env_var.to_string(),
        token_store: token_store_path.display().to_string(),
        encrypted: true,
        updated_at: now,
    };
    write_private_file(session_path, &serde_json::to_vec_pretty(&session)?)?;
    Ok(session)
}

pub fn load_session() -> Option<AuthSession> {
    let path = default_session_path().ok()?;
    load_session_from_path(&path)
}

pub fn load_session_from_path(path: &Path) -> Option<AuthSession> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

pub fn load_token(provider: &str) -> Result<Option<(String, String)>> {
    let path = default_token_store_path()?;
    load_token_at(&path, provider)
}

pub fn load_token_at(path: &Path, provider: &str) -> Result<Option<(String, String)>> {
    let passphrase = std::env::var(AUTH_PASSPHRASE_ENV)
        .map_err(|_| anyhow!("{AUTH_PASSPHRASE_ENV} is not set"))?;
    load_token_at_with_passphrase(path, provider, passphrase.as_bytes())
}

pub(crate) fn load_token_at_with_passphrase(
    path: &Path,
    provider: &str,
    passphrase: &[u8],
) -> Result<Option<(String, String)>> {
    let Some(store) = load_token_store(path) else {
        return Ok(None);
    };
    let Some(token) = store.tokens.get(provider) else {
        return Ok(None);
    };
    let plaintext = decrypt_token(token, passphrase)?;
    let token_value = String::from_utf8(plaintext).map_err(|_| anyhow!("token is not UTF-8"))?;
    Ok(Some((token.env_var.clone(), token_value)))
}

pub fn token_store_status(provider: &str) -> String {
    let Ok(path) = default_token_store_path() else {
        return "token_store=unavailable".to_string();
    };
    if !path.exists() {
        return "token_store=missing".to_string();
    }
    let Some(store) = load_token_store(&path) else {
        return "token_store=invalid".to_string();
    };
    if !store.tokens.contains_key(provider) {
        return "token_store=no-token".to_string();
    }
    if std::env::var(AUTH_PASSPHRASE_ENV).is_err() {
        return format!("token_store=locked passphrase_env={AUTH_PASSPHRASE_ENV}");
    }
    match load_token_at(&path, provider) {
        Ok(Some(_)) => "token_store=ready encrypted=true".to_string(),
        Ok(None) => "token_store=no-token".to_string(),
        Err(_) => "token_store=locked".to_string(),
    }
}

pub fn clear() -> Result<()> {
    let session_path = default_session_path()?;
    let token_store_path = default_token_store_path()?;
    clear_at(&session_path, &token_store_path)
}

pub fn clear_at(session_path: &Path, token_store_path: &Path) -> Result<()> {
    if session_path.exists() {
        fs::remove_file(session_path)?;
    }
    if token_store_path.exists() {
        fs::remove_file(token_store_path)?;
    }
    Ok(())
}

pub fn hydrate_env_for_provider(provider: &str) -> Result<bool> {
    let Some((env_var, token)) = load_token(provider)? else {
        return Ok(false);
    };
    if std::env::var(&env_var).is_err() {
        std::env::set_var(env_var, token);
        return Ok(true);
    }
    Ok(false)
}

fn load_token_store(path: &Path) -> Option<TokenStore> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

struct EncryptedPayload {
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn encrypt_token(plaintext: &[u8], passphrase: &[u8]) -> Result<EncryptedPayload> {
    let rng = SystemRandom::new();
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rng.fill(&mut salt)
        .map_err(|_| anyhow!("failed to generate salt"))?;
    rng.fill(&mut nonce)
        .map_err(|_| anyhow!("failed to generate nonce"))?;
    let key = derive_key(passphrase, &salt)?;
    let unbound = UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| anyhow!("invalid key"))?;
    let key = LessSafeKey::new(unbound);
    let mut in_out = plaintext.to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(b"tengu-auth-token-v1"),
        &mut in_out,
    )
    .map_err(|_| anyhow!("token encryption failed"))?;
    Ok(EncryptedPayload {
        salt: encode(&salt),
        nonce: encode(&nonce),
        ciphertext: encode(&in_out),
    })
}

fn decrypt_token(token: &EncryptedToken, passphrase: &[u8]) -> Result<Vec<u8>> {
    let salt = decode(&token.salt)?;
    let nonce = decode(&token.nonce)?;
    if nonce.len() != NONCE_LEN {
        return Err(anyhow!("invalid nonce length"));
    }
    let key = derive_key(passphrase, &salt)?;
    let unbound = UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| anyhow!("invalid key"))?;
    let key = LessSafeKey::new(unbound);
    let mut in_out = decode(&token.ciphertext)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&nonce);
    let plaintext = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(b"tengu-auth-token-v1"),
            &mut in_out,
        )
        .map_err(|_| anyhow!("token decryption failed"))?;
    Ok(plaintext.to_vec())
}

fn derive_key(passphrase: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    let iterations = NonZeroU32::new(PBKDF2_ITERATIONS).ok_or_else(|| anyhow!("invalid KDF"))?;
    let mut key = [0u8; KEY_LEN];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        salt,
        passphrase,
        &mut key,
    );
    Ok(key)
}

fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn decode(value: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|err| anyhow!("base64 decode failed: {err}"))
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    set_private_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_encrypted_token() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("session.json");
        let tokens = dir.path().join("tokens.json");

        save_login_at_with_passphrase(
            &session,
            &tokens,
            "anthropic",
            "ANTHROPIC_API_KEY",
            "sk-secret",
            b"test passphrase",
        )
        .unwrap();

        let raw = fs::read_to_string(&tokens).unwrap();
        assert!(!raw.contains("sk-secret"));
        let loaded = load_token_at_with_passphrase(&tokens, "anthropic", b"test passphrase")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.0, "ANTHROPIC_API_KEY");
        assert_eq!(loaded.1, "sk-secret");
        let session = load_session_from_path(&session).unwrap();
        assert!(session.encrypted);
    }

    #[test]
    fn rejects_wrong_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("session.json");
        let tokens = dir.path().join("tokens.json");
        save_login_at_with_passphrase(
            &session,
            &tokens,
            "openai",
            "OPENAI_API_KEY",
            "sk-secret",
            b"correct passphrase",
        )
        .unwrap();

        let err =
            load_token_at_with_passphrase(&tokens, "openai", b"wrong passphrase").unwrap_err();
        assert!(err.to_string().contains("decryption failed"));
    }

    #[test]
    fn clear_removes_session_and_token_store() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("session.json");
        let tokens = dir.path().join("tokens.json");
        save_login_at_with_passphrase(
            &session,
            &tokens,
            "google",
            "GOOGLE_API_KEY",
            "token",
            b"test passphrase",
        )
        .unwrap();

        clear_at(&session, &tokens).unwrap();
        assert!(!session.exists());
        assert!(!tokens.exists());
    }
}
