//! 剪贴板历史的本地 AES-256-GCM 加密（v0.1.14 隐私 P3）
//!
//! ## 威胁模型
//! 主要防护：备份盘 / 同步盘 / 备份服务（Time Machine、iCloud Documents 同步）意外
//! 把剪贴板历史泄露给非 owner 用户。FileVault 已经在磁盘层加密，但用户可能：
//!   - 关闭 FileVault
//!   - 把 ~ 目录通过 iCloud Drive 同步
//!   - 备份盘没加密
//!   - rsync 到 NAS
//!
//! ## 做法
//! 1. 启动时检查 Keychain 里有没有 com.mouseclaw.clipkey 这个 generic password
//! 2. 没有 → 生成 32 字节随机 key，存进 Keychain（仅当前用户可读）
//! 3. 加密：AES-256-GCM，每次写文件用新随机 12-byte nonce
//!    格式：[12B nonce][密文 + 16B GCM tag][换行]
//!    一条 ClipItem 一行，逐行加密，跟原 jsonl 行结构对齐
//! 4. 解密：逐行读，每行先取 nonce 再 decrypt → 反序列化 ClipItem
//!
//! ## 旧 plain-text jsonl 兼容
//! 文件第一行若是 `__mc_v1\n` 视为加密格式；否则当老 plain JSONL 读，
//! 一次性升级（下次 save_to_disk 会以加密格式重写）。

use anyhow::{anyhow, Context, Result};
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, AeadCore},
    Aes256Gcm, Key, Nonce,
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use std::sync::OnceLock;

/// ⚠️ v0.1.14 修：cipher 用 OnceLock 缓存，**不要**每次写盘都打开 Keychain。
/// 之前每次复制都调一次 Keychain → 量级太大可能崩，或被 macOS 限速。
static CIPHER: OnceLock<Aes256Gcm> = OnceLock::new();

const KEYCHAIN_SERVICE: &str = "com.mouseclaw.clipboard";
const KEYCHAIN_ACCOUNT: &str = "encryption-key-v1";
const FILE_MAGIC: &str = "__mc_v1";

/// 拿 / 生成 256-bit master key。存储后端按平台分（见 Cargo.toml 注释）：
///   macOS → Keychain（security-framework）
///   Win/Linux → OS keystore（keyring）；keyring 不可用时退回 0600 文件
fn ensure_key() -> Result<[u8; 32]> {
    if let Some(raw) = load_key_b64().and_then(|s| B64.decode(s.trim()).ok()) {
        if raw.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&raw);
            return Ok(out);
        }
        println!("[mouseclaw] 🔐 clipboard key 损坏，重生成");
    }
    // 新生成
    let mut key = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut key);
    let b64 = B64.encode(key);
    store_key_b64(&b64)?;
    println!("[mouseclaw] 🔐 clipboard key 已生成 + 存入安全存储");
    Ok(key)
}

// ───────────────────── 平台密钥存储后端 ─────────────────────

/// macOS：Keychain（保留 v0.1.14 起的条目语义，不迁移现有用户）。
#[cfg(target_os = "macos")]
fn load_key_b64() -> Option<String> {
    use security_framework::passwords::get_generic_password;
    let bytes = get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).ok()?;
    String::from_utf8(bytes).ok()
}
#[cfg(target_os = "macos")]
fn store_key_b64(b64: &str) -> Result<()> {
    use security_framework::passwords::set_generic_password;
    set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, b64.as_bytes())
        .map_err(|e| anyhow!("write Keychain: {e}"))
}

/// Win/Linux：keyring（Win=Credential Manager / Linux=Secret Service）。
/// keyring 不可用（如无 Secret Service daemon 的 headless Linux）→ 退回 0600 文件，
/// 并打日志告警（威胁模型见本文件顶部；文件存储弱于 OS keystore，属已知降级）。
#[cfg(not(target_os = "macos"))]
fn keyring_entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).ok()
}
#[cfg(not(target_os = "macos"))]
fn load_key_b64() -> Option<String> {
    if let Some(s) = keyring_entry().and_then(|e| e.get_password().ok()) {
        return Some(s);
    }
    std::fs::read_to_string(key_file_path()?).ok()
}
#[cfg(not(target_os = "macos"))]
fn store_key_b64(b64: &str) -> Result<()> {
    if let Some(entry) = keyring_entry() {
        if entry.set_password(b64).is_ok() {
            return Ok(());
        }
    }
    // keyring 不可用 → 0600 文件兜底
    let path = key_file_path().ok_or_else(|| anyhow!("no home dir for clipboard key file"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create key dir")?;
    }
    std::fs::write(&path, b64).context("write key file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    eprintln!(
        "[mouseclaw] ⚠️ OS keystore 不可用，clipboard key 退回文件 {} (0600)",
        path.display()
    );
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn key_file_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::PathBuf::from(home).join(".mouseclaw").join("clipkey"))
}

fn cipher() -> Result<&'static Aes256Gcm> {
    if let Some(c) = CIPHER.get() { return Ok(c); }
    let key_bytes = ensure_key()?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let c = Aes256Gcm::new(key);
    // 失败说明已经被别的线程 set 了 —— 取那个
    let _ = CIPHER.set(c);
    Ok(CIPHER.get().expect("cipher just set"))
}

/// 加密一行 JSON 字符串 → base64 字符串（nonce + ciphertext）
pub fn encrypt_line(plain: &str) -> Result<String> {
    let c = cipher()?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = c.encrypt(&nonce, plain.as_bytes())
        .map_err(|e| anyhow!("AES-GCM encrypt: {e}"))?;
    // 输出 base64(nonce + ct)，单行不带换行
    let mut buf = Vec::with_capacity(nonce.len() + ct.len());
    buf.extend_from_slice(&nonce);
    buf.extend_from_slice(&ct);
    Ok(B64.encode(buf))
}

/// 解密一行 base64 → 原始 JSON 字符串
pub fn decrypt_line(b64: &str) -> Result<String> {
    let raw = B64.decode(b64.trim()).context("b64 decode")?;
    if raw.len() < 12 + 16 { anyhow::bail!("ciphertext too short"); }
    let (nonce_bytes, ct) = raw.split_at(12);
    let c = cipher()?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let pt = c.decrypt(nonce, ct).map_err(|e| anyhow!("AES-GCM decrypt: {e}"))?;
    Ok(String::from_utf8(pt).context("decrypted not utf8")?)
}

/// 文件第一行是否是加密 magic
pub fn is_encrypted_format(first_line: &str) -> bool {
    first_line.trim() == FILE_MAGIC
}

pub fn file_magic() -> &'static str { FILE_MAGIC }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        // 需要 Keychain 访问 —— 单测环境可能没有钥匙串 daemon，跳过实际加密
        // 仅验证 magic 检测正确
        assert!(is_encrypted_format("__mc_v1"));
        assert!(is_encrypted_format("__mc_v1\n"));
        assert!(!is_encrypted_format(r#"{"id":1,"text":"hi"}"#));
    }
}
