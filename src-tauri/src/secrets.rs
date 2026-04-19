use std::sync::OnceLock;

use keyring_core::{Entry, Error as KeyringError};

use crate::domain::{AppError, AppResult};

const SERVICE: &str = "com.dualinvestment.app";
const API_KEY_USER: &str = "binance-api-key";
const API_SECRET_USER: &str = "binance-api-secret";
static KEYRING_STORE_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn save_binance_credentials(api_key: &str, api_secret: &str) -> AppResult<()> {
    entry(API_KEY_USER)?
        .set_password(api_key)
        .map_err(|error| to_error(&error))?;
    entry(API_SECRET_USER)?
        .set_password(api_secret)
        .map_err(|error| to_error(&error))?;
    Ok(())
}

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

pub fn has_binance_credentials() -> bool {
    matches!(load_binance_credentials(), Ok(Some(_)))
}

pub fn delete_binance_credentials() -> AppResult<()> {
    delete_entry(API_KEY_USER)?;
    delete_entry(API_SECRET_USER)?;
    Ok(())
}

fn delete_entry(user: &str) -> AppResult<()> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(to_error(&error)),
    }
}

fn entry(user: &str) -> AppResult<Entry> {
    ensure_keyring_store()?;
    Entry::new(SERVICE, user).map_err(|error| to_error(&error))
}

fn ensure_keyring_store() -> AppResult<()> {
    match KEYRING_STORE_INIT.get_or_init(configure_keyring_store) {
        Ok(()) => Ok(()),
        Err(message) => Err(AppError::Config(message.clone())),
    }
}

#[cfg(target_os = "linux")]
fn configure_keyring_store() -> Result<(), String> {
    let store = dbus_secret_service_keyring_store::Store::new()
        .map_err(|error| format!("系统安全凭据存储不可用：{error}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(target_os = "macos")]
fn configure_keyring_store() -> Result<(), String> {
    let store = apple_native_keyring_store::keychain::Store::new()
        .map_err(|error| format!("系统安全凭据存储不可用：{error}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_keyring_store() -> Result<(), String> {
    let store = windows_native_keyring_store::Store::new()
        .map_err(|error| format!("系统安全凭据存储不可用：{error}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn configure_keyring_store() -> Result<(), String> {
    Err("系统安全凭据存储不可用：当前平台没有配置 keyring-core store".to_string())
}

fn to_error(error: &KeyringError) -> AppError {
    AppError::Config(format!("系统安全凭据存储不可用：{error}"))
}
