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
use std::{fs, path::PathBuf};
use uuid::Uuid;

use crate::{config::Config, db::Database, domain::SecretRecord};

const ENC_DPAPI: &str = "windows-dpapi-v1";
const ENC_ENV_AES: &str = "env-aes-gcm-v1";
const ENC_LOCAL_AES: &str = "local-aes-gcm-v1";

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
        self.store_with_metadata(db, name, provider, kind, plaintext, serde_json::json!({}))
            .await
    }

    pub async fn store_with_metadata(
        &self,
        db: &Database,
        name: &str,
        provider: &str,
        kind: &str,
        plaintext: &str,
        metadata: serde_json::Value,
    ) -> Result<SecretRecord> {
        let encrypted = encrypt_secret(&self.config, plaintext.as_bytes())?;
        let metadata = merge_secret_metadata(metadata, plaintext.len());
        let record = db
            .upsert_secret_record(
                name,
                provider,
                kind,
                encrypted.bytes,
                encrypted.scheme,
                metadata,
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
                scheme: ENC_LOCAL_AES.to_string(),
                encrypted_at_rest: true,
                note: format!(
                    "AES-GCM uses Librarian's local master key at {}",
                    local_master_key_path(&self.config).display()
                ),
            }
        }
    }
}

fn merge_secret_metadata(
    mut metadata: serde_json::Value,
    plaintext_len: usize,
) -> serde_json::Value {
    if !metadata.is_object() {
        metadata = serde_json::json!({});
    }
    if let Some(object) = metadata.as_object_mut() {
        object.insert("stored_by".to_string(), serde_json::json!("secret-vault"));
        object.insert(
            "plaintext_len".to_string(),
            serde_json::json!(plaintext_len),
        );
    }
    metadata
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
        if std::env::var("LIBRARIAN_SECRET_KEY").is_ok() {
            encrypt_with_key(config, plaintext, ENC_ENV_AES, MasterKeySource::Env)
        } else {
            encrypt_with_key(config, plaintext, ENC_LOCAL_AES, MasterKeySource::LocalFile)
        }
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
        ENC_ENV_AES => decrypt_with_key(config, ciphertext, MasterKeySource::Env),
        ENC_LOCAL_AES => decrypt_with_key(config, ciphertext, MasterKeySource::LocalFile),
        _ => bail!("Unsupported secret encryption scheme `{scheme}`"),
    }
}

#[cfg(not(windows))]
fn encrypt_with_key(
    config: &Config,
    plaintext: &[u8],
    scheme: &'static str,
    source: MasterKeySource,
) -> Result<EncryptedSecret> {
    use aes_gcm::aead::OsRng;
    use rand::RngCore;

    let key = master_key(config, source)?;
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
    Ok(EncryptedSecret { scheme, bytes: out })
}

fn decrypt_with_key(
    config: &Config,
    ciphertext: &[u8],
    source: MasterKeySource,
) -> Result<Vec<u8>> {
    if ciphertext.len() < 12 {
        bail!("Ciphertext is too short");
    }
    let key = master_key(config, source)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let (nonce_bytes, body) = ciphertext.split_at(12);
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), body)
        .map_err(|_| anyhow::anyhow!("Failed to decrypt secret"))
}

#[derive(Clone, Copy)]
enum MasterKeySource {
    Env,
    LocalFile,
}

fn master_key(config: &Config, source: MasterKeySource) -> Result<[u8; 32]> {
    let raw = match source {
        MasterKeySource::Env => std::env::var("LIBRARIAN_SECRET_KEY")
            .context("LIBRARIAN_SECRET_KEY is required to decrypt env-key secrets")?,
        MasterKeySource::LocalFile => read_or_create_local_master_key(config)?,
    };
    let mut hasher = Sha256::new();
    hasher.update(config.home.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(raw.as_bytes());
    Ok(hasher.finalize().into())
}

fn local_master_key_path(config: &Config) -> PathBuf {
    config.home.join(".cfg").join("secret.key")
}

fn read_or_create_local_master_key(config: &Config) -> Result<String> {
    let path = local_master_key_path(config);
    if path.exists() {
        return Ok(fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?
            .trim()
            .to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let key = generate_local_master_key();
    fs::write(&path, format!("{key}\n"))
        .with_context(|| format!("Failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to chmod {}", path.display()))?;
    }
    Ok(key)
}

fn generate_local_master_key() -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use rand::RngCore;

    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn local_master_key_is_created_under_cfg() {
        let home = std::env::current_dir()
            .expect("cwd")
            .join(format!(".librarian-test-secret-key-{}", Uuid::new_v4()));
        let config = Config::load_or_default(Some(home.clone())).expect("config");
        config.ensure_layout().expect("layout");

        let key = read_or_create_local_master_key(&config).expect("key");
        assert!(!key.is_empty());
        assert!(home.join(".cfg").join("secret.key").is_file());
        assert_eq!(
            read_or_create_local_master_key(&config).expect("same key"),
            key
        );

        std::fs::remove_dir_all(home).ok();
    }
}
