//! 系统 keyring 凭据存储。
//!
//! Binance API Key/Secret 只允许通过这个模块读写系统安全凭据存储。其他模块只接收
//! 连接状态、脱敏 API Key 或内存中的客户端实例，不能把 Secret 写入 SQLite、审计日志、
//! 前端 store 或导出文件。

use crate::messages::Message;

use std::sync::{Mutex, OnceLock};

use keyring_core::{Entry, Error as KeyringError};

use crate::domain::{AppError, AppResult};

const SERVICE: &str = "com.dualinvestment.app";
const API_KEY_USER: &str = "binance-api-key";
const API_SECRET_USER: &str = "binance-api-secret";
static KEYRING_STORE_READY: OnceLock<()> = OnceLock::new();
static KEYRING_STORE_INIT_LOCK: Mutex<()> = Mutex::new(());

/// 保存 Binance 凭据到系统 keyring。
pub fn save_binance_credentials(api_key: &str, api_secret: &str) -> AppResult<()> {
    entry(API_KEY_USER)?
        .set_password(api_key)
        .map_err(|error| to_error(&error))?;
    entry(API_SECRET_USER)?
        .set_password(api_secret)
        .map_err(|error| to_error(&error))?;
    Ok(())
}

/// 从系统 keyring 读取 Binance 凭据。
///
/// Key 或 Secret 任意一个不存在时都视为没有可用凭据。
pub fn load_binance_credentials() -> AppResult<Option<(String, String)>> {
    let api_key = match entry(API_KEY_USER)?.get_password() {
        Ok(value) => value,
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(error) => return Err(to_error(&error)),
    };
    let api_secret = match entry(API_SECRET_USER)?.get_password() {
        Ok(value) => value,
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(error) => return Err(to_error(&error)),
    };
    Ok(Some((api_key, api_secret)))
}

/// 检查系统 keyring 中是否存在完整 Binance 凭据。
pub fn has_binance_credentials() -> bool {
    matches!(load_binance_credentials(), Ok(Some(_)))
}

/// 删除系统 keyring 中保存的 Binance 凭据。
pub fn delete_binance_credentials() -> AppResult<()> {
    delete_entry(API_KEY_USER)?;
    delete_entry(API_SECRET_USER)?;
    Ok(())
}

/// 删除单个 keyring 条目；不存在时按幂等成功处理。
fn delete_entry(user: &str) -> AppResult<()> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(to_error(&error)),
    }
}

/// 创建 keyring 条目，并确保当前平台的 store 已注册。
fn entry(user: &str) -> AppResult<Entry> {
    ensure_keyring_store()?;
    Entry::new(SERVICE, user).map_err(|error| to_error(&error))
}

/// 懒加载初始化 keyring store，避免每次读写凭据都重复注册平台实现。
fn ensure_keyring_store() -> AppResult<()> {
    if KEYRING_STORE_READY.get().is_some() {
        return Ok(());
    }
    let _guard = KEYRING_STORE_INIT_LOCK.lock().map_err(|error| {
        AppError::Config(crate::msg!(
            "backend.secureCredentialStorageInitializationLockUnavailable",
            p0 = error.to_string()
        ))
    })?;
    if KEYRING_STORE_READY.get().is_some() {
        return Ok(());
    }
    // 初始化失败不写入 OnceLock，后续连接或读取凭据时仍可重试系统 keyring。
    configure_keyring_store().map_err(AppError::Config)?;
    let _ = KEYRING_STORE_READY.set(());
    Ok(())
}

/// Linux 使用 Secret Service / `DBus` keyring。
#[cfg(target_os = "linux")]
fn configure_keyring_store() -> Result<(), Message> {
    let store = dbus_secret_service_keyring_store::Store::new().map_err(|error| {
        crate::msg!(
            "backend.systemSecureCredentialStorageUnavailable",
            p0 = error.to_string()
        )
    })?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// macOS 使用系统 Keychain。
#[cfg(target_os = "macos")]
fn configure_keyring_store() -> Result<(), Message> {
    let store = apple_native_keyring_store::keychain::Store::new().map_err(|error| {
        crate::msg!(
            "backend.systemSecureCredentialStorageUnavailable",
            p0 = error.to_string()
        )
    })?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// Windows 使用系统凭据管理器。
#[cfg(target_os = "windows")]
fn configure_keyring_store() -> Result<(), Message> {
    let store = windows_native_keyring_store::Store::new().map_err(|error| {
        crate::msg!(
            "backend.systemSecureCredentialStorageUnavailable",
            p0 = error.to_string()
        )
    })?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// 未显式支持的平台不降级为明文文件存储。
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn configure_keyring_store() -> Result<(), Message> {
    Err(crate::msg!(
        "backend.systemSecureCredentialStorageUnavailableNoKeyring"
    ))
}

/// 统一把 keyring 错误转成可展示的配置错误。
fn to_error(error: &KeyringError) -> AppError {
    AppError::Config(crate::msg!(
        "backend.systemSecureCredentialStorageUnavailable",
        p0 = error.to_string()
    ))
}
