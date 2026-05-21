use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::io::Write;
use uuid::Uuid;

use crate::{config::Config, db::Database, domain::SecretRecord};

const ENC_DPAPI: &str = "windows-dpapi-v1";
const ENC_ENV_AES: &str = "env-aes-gcm-v1";

#[derive(Clone, Debug)]
pub struct SecretVault {
    config: Config,
}

impl SecretVault {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn store(
        &self,
        db: &Database,
        name: &str,
        provider: &str,
        kind: &str,
        plaintext: &str,
    ) -> Result<SecretRecord> {
        let encrypted = encrypt_secret(&self.config, plaintext.as_bytes())?;
        let record = db
            .upsert_secret_record(
                name,
                provider,
                kind,
                encrypted.bytes,
                encrypted.scheme,
                serde_json::json!({
                    "stored_by": "secret-vault",
                    "plaintext_len": plaintext.len(),
                }),
            )
            .await?;
        db.add_secret_audit_event(
            record.id,
            None,
            None,
            "secret_store",
            true,
            serde_json::json!({ "name": record.name, "provider": record.provider }),
        )
        .await?;
        Ok(record)
    }

    pub async fn grant(
        &self,
        db: &Database,
        secret_ref: &str,
        job_id: Option<Uuid>,
        provider: Option<&str>,
        capability: &str,
        ttl_seconds: i64,
        max_uses: i64,
    ) -> Result<Uuid> {
        let secret = db.get_secret_by_name_or_id(secret_ref).await?;
        let grant = db
            .create_secret_grant(
                secret.id,
                job_id,
                provider,
                capability,
                Utc::now() + Duration::seconds(ttl_seconds.max(1)),
                max_uses.max(1),
            )
            .await?;
        db.add_secret_audit_event(
            secret.id,
            Some(grant.id),
            job_id,
            "secret_grant",
            true,
            serde_json::json!({
                "capability": capability,
                "provider": provider,
                "ttl_seconds": ttl_seconds.max(1),
                "max_uses": max_uses.max(1),
            }),
        )
        .await?;
        Ok(grant.id)
    }

    pub async fn resolve_with_grant(
        &self,
        db: &Database,
        grant_id: Uuid,
        capability: &str,
        job_id: Option<Uuid>,
    ) -> Result<ResolvedSecret> {
        let grant = db.consume_secret_grant(grant_id).await?;
        if grant.capability != capability {
            db.add_secret_audit_event(
                grant.secret_id,
                Some(grant.id),
                job_id,
                "secret_resolve",
                false,
                serde_json::json!({ "reason": "capability_mismatch", "requested": capability }),
            )
            .await?;
            bail!("Grant capability mismatch");
        }
        if let Some(expected_job_id) = grant.job_id {
            if Some(expected_job_id) != job_id {
                db.add_secret_audit_event(
                    grant.secret_id,
                    Some(grant.id),
                    job_id,
                    "secret_resolve",
                    false,
                    serde_json::json!({ "reason": "job_mismatch" }),
                )
                .await?;
                bail!("Grant job mismatch");
            }
        }
        let secret = db
            .get_secret_by_name_or_id(&grant.secret_id.to_string())
            .await?;
        let plaintext = decrypt_secret(&self.config, &secret.encryption, &secret.ciphertext)?;
        db.add_secret_audit_event(
            secret.id,
            Some(grant.id),
            job_id,
            "secret_resolve",
            true,
            serde_json::json!({ "capability": capability, "provider": grant.provider }),
        )
        .await?;
        Ok(ResolvedSecret {
            name: secret.name,
            provider: secret.provider,
            kind: secret.kind,
            plaintext: String::from_utf8(plaintext).context("Secret is not valid UTF-8")?,
        })
    }

    pub fn decrypt_record(&self, record: &SecretRecord) -> Result<String> {
        let plaintext = decrypt_secret(&self.config, &record.encryption, &record.ciphertext)?;
        Ok(String::from_utf8(plaintext).context("Secret is not valid UTF-8")?)
    }

    pub fn encryption_status(&self) -> EncryptionStatus {
        if cfg!(windows) {
            EncryptionStatus {
                scheme: ENC_DPAPI.to_string(),
                encrypted_at_rest: true,
                note: "Windows DPAPI protects ciphertext for the current user".to_string(),
            }
        } else if std::env::var("LIBRARIAN_SECRET_KEY").is_ok() {
            EncryptionStatus {
                scheme: ENC_ENV_AES.to_string(),
                encrypted_at_rest: true,
                note: "AES-GCM uses LIBRARIAN_SECRET_KEY as the local master secret".to_string(),
            }
        } else {
            EncryptionStatus {
                scheme: ENC_ENV_AES.to_string(),
                encrypted_at_rest: false,
                note: "Set LIBRARIAN_SECRET_KEY on this platform before storing secrets"
                    .to_string(),
            }
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct EncryptionStatus {
    pub scheme: String,
    pub encrypted_at_rest: bool,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedSecret {
    pub name: String,
    pub provider: String,
    pub kind: String,
    pub plaintext: String,
}

struct EncryptedSecret {
    scheme: &'static str,
    bytes: Vec<u8>,
}

fn encrypt_secret(config: &Config, plaintext: &[u8]) -> Result<EncryptedSecret> {
    #[cfg(windows)]
    {
        let _ = config;
        let bytes = windows_dpapi_protect(plaintext)?;
        Ok(EncryptedSecret {
            scheme: ENC_DPAPI,
            bytes,
        })
    }
    #[cfg(not(windows))]
    {
        encrypt_with_env_key(config, plaintext)
    }
}

fn decrypt_secret(config: &Config, scheme: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
    match scheme {
        ENC_DPAPI => {
            #[cfg(windows)]
            {
                windows_dpapi_unprotect(ciphertext)
            }
            #[cfg(not(windows))]
            {
                let _ = config;
                bail!("Windows DPAPI secret cannot be decrypted on this platform")
            }
        }
        ENC_ENV_AES => decrypt_with_env_key(config, ciphertext),
        _ => bail!("Unsupported secret encryption scheme `{scheme}`"),
    }
}

#[cfg(not(windows))]
fn encrypt_with_env_key(config: &Config, plaintext: &[u8]) -> Result<EncryptedSecret> {
    use aes_gcm::aead::OsRng;
    use rand::RngCore;

    let key = env_key(config)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let mut out = nonce_bytes.to_vec();
    out.extend(
        cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| anyhow::anyhow!("Failed to encrypt secret"))?,
    );
    Ok(EncryptedSecret {
        scheme: ENC_ENV_AES,
        bytes: out,
    })
}

fn decrypt_with_env_key(config: &Config, ciphertext: &[u8]) -> Result<Vec<u8>> {
    if ciphertext.len() < 12 {
        bail!("Ciphertext is too short");
    }
    let key = env_key(config)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let (nonce_bytes, body) = ciphertext.split_at(12);
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), body)
        .map_err(|_| anyhow::anyhow!("Failed to decrypt secret"))
}

fn env_key(config: &Config) -> Result<[u8; 32]> {
    let raw = std::env::var("LIBRARIAN_SECRET_KEY")
        .context("LIBRARIAN_SECRET_KEY must be set before storing secrets on this platform")?;
    let mut hasher = Sha256::new();
    hasher.update(config.home.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(raw.as_bytes());
    Ok(hasher.finalize().into())
}

pub fn encode_grant_token(grant_id: Uuid) -> String {
    STANDARD.encode(grant_id.as_bytes())
}

pub fn decode_grant_token(token: &str) -> Result<Uuid> {
    let bytes = STANDARD.decode(token)?;
    Ok(Uuid::from_slice(&bytes)?)
}

#[cfg(windows)]
fn windows_dpapi_protect(plaintext: &[u8]) -> Result<Vec<u8>> {
    let script = r#"
$plain = [Console]::In.ReadToEnd()
$secure = ConvertTo-SecureString -String $plain -AsPlainText -Force
ConvertFrom-SecureString -SecureString $secure
"#;
    let output = run_powershell_with_stdin(script, plaintext)?;
    Ok(output.trim().as_bytes().to_vec())
}

#[cfg(windows)]
fn windows_dpapi_unprotect(ciphertext: &[u8]) -> Result<Vec<u8>> {
    let script = r#"
$encrypted = [Console]::In.ReadToEnd().Trim()
$secure = ConvertTo-SecureString -String $encrypted
$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
try {
  [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
} finally {
  [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
}
"#;
    let output = run_powershell_with_stdin(script, ciphertext)?;
    Ok(output.as_bytes().to_vec())
}

#[cfg(windows)]
fn run_powershell_with_stdin(script: &str, stdin: &[u8]) -> Result<String> {
    let mut child = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to start PowerShell for Windows DPAPI")?;
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin.write_all(stdin)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "PowerShell DPAPI command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}
