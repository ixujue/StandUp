//! WebDAV 密码存取(D13)。
//! Windows:DPAPI 当前用户域加密后落盘(`sync-cred.bin`),不依赖凭据管理器的
//! 目标名路由行为;其他平台:系统凭证管理器(keyring)。
//! `username` 仅作存取标识,真实 WebDAV 账号在同步设置里。

use std::fs;
use std::path::Path;

const CRED_FILE: &str = "sync-cred.bin";

#[cfg(windows)]
pub fn store_password(dir: &Path, _username: &str, password: &str) -> Result<(), String> {
    if password.is_empty() {
        let _ = fs::remove_file(dir.join(CRED_FILE));
        return Ok(());
    }
    let blob = dpapi_protect(password.as_bytes())?;
    fs::write(dir.join(CRED_FILE), hex_encode(&blob)).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
pub fn read_password(dir: &Path, _username: &str) -> Result<String, String> {
    let text = match fs::read_to_string(dir.join(CRED_FILE)) {
        Ok(t) => t,
        Err(_) => return Ok(String::new()),
    };
    let blob = hex_decode(&text).ok_or("凭据文件损坏")?;
    let plain = dpapi_unprotect(&blob)?;
    String::from_utf8(plain).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: plain.len() as u32,
            pbData: plain.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, None, None, None, None, 0, &mut output)
            .map_err(|e| format!("DPAPI 加密失败: {e}"))?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFreeBlob(output.pbData);
        Ok(bytes)
    }
}

#[cfg(windows)]
fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, None, None, None, 0, &mut output)
            .map_err(|e| format!("DPAPI 解密失败: {e}"))?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFreeBlob(output.pbData);
        Ok(bytes)
    }
}

#[cfg(windows)]
fn LocalFreeBlob(ptr: *mut u8) {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    if !ptr.is_null() {
        unsafe {
            let _ = LocalFree(HLOCAL(ptr as *mut _));
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

#[cfg(target_os = "android")]
pub fn store_password(dir: &Path, _username: &str, password: &str) -> Result<(), String> {
    // Android 无系统级用户凭证服务:暂存应用私有目录(/data/data,仅本应用可读),
    // Phase 2 接 Android Keystore 加密。
    if password.is_empty() {
        let _ = fs::remove_file(dir.join(CRED_FILE));
        return Ok(());
    }
    fs::write(dir.join(CRED_FILE), hex_encode(password.as_bytes())).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
pub fn read_password(dir: &Path, _username: &str) -> Result<String, String> {
    match fs::read_to_string(dir.join(CRED_FILE)) {
        Ok(text) => {
            let bytes = hex_decode(&text).ok_or("凭据文件损坏")?;
            String::from_utf8(bytes).map_err(|e| e.to_string())
        }
        Err(_) => Ok(String::new()),
    }
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "ios")))]
pub fn store_password(_dir: &Path, username: &str, password: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("standup-webdav", username).map_err(|e| e.to_string())?;
    if password.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(password).map_err(|e| e.to_string())
    }
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "ios")))]
pub fn read_password(_dir: &Path, username: &str) -> Result<String, String> {
    let entry = keyring::Entry::new("standup-webdav", username).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(p) => Ok(p),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}
