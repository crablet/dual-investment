//! 本地文件日志。
//!
//! 日志按本机日期写入应用数据目录下的 `logs/YYYY-MM-DD.log`，每行一条 JSON。
//! 写入内容必须是脱敏摘要，不能包含 API Secret、签名 query 或 Binance 原始响应。

use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{Local, Utc};
use uuid::Uuid;

use crate::domain::{AppError, AppResult, LogEntry, LogLevel, LogSnapshot};

const RECENT_ENTRY_LIMIT: usize = 120;
const MAX_LINE_BYTES: usize = 64 * 1024;

pub struct LogStore {
    directory: PathBuf,
    write_error: Mutex<Option<String>>,
}

impl LogStore {
    /// 日志目录按需创建；日志不可写不能阻止本地订单加载或交易事实保存。
    pub fn open_app_data(data_dir: impl AsRef<Path>) -> Self {
        Self {
            directory: data_dir.as_ref().join("logs"),
            write_error: Mutex::new(None),
        }
    }

    /// 写入失败只更新日志面板的错误状态，不改变交易操作的返回结果。
    pub fn append(&self, level: LogLevel, event: &str, message: &str) {
        let mut write_error = self
            .write_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *write_error = self
            .write_entry(level, event, message)
            .err()
            .map(|error| error.to_string());
    }

    fn write_entry(&self, level: LogLevel, event: &str, message: &str) -> AppResult<()> {
        fs::create_dir_all(&self.directory).map_err(storage_error)?;
        let entry = LogEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            level,
            event: event.to_string(),
            message: message.to_string(),
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.current_file_path())
            .map_err(storage_error)?;
        serde_json::to_writer(&mut file, &entry)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        file.write_all(b"\n").map_err(storage_error)?;
        Ok(())
    }

    /// 返回设置页展示所需的日志快照。
    pub fn snapshot(&self) -> AppResult<LogSnapshot> {
        fs::create_dir_all(&self.directory).map_err(storage_error)?;
        let paths = self.log_file_paths()?;
        let recent_entries = Self::recent_entries(&paths, RECENT_ENTRY_LIMIT)?;
        Ok(LogSnapshot {
            has_files: !paths.is_empty(),
            recent_entries,
            write_error: self
                .write_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        })
    }

    /// 确保今日日志文件存在，并返回文件路径。
    pub fn ensure_current_file(&self) -> AppResult<PathBuf> {
        let _guard = self
            .write_error
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        fs::create_dir_all(&self.directory).map_err(storage_error)?;
        let path = self.current_file_path();
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(storage_error)?;
        Ok(path)
    }

    /// 确保日志目录存在，并返回目录路径。
    pub fn ensure_directory(&self) -> AppResult<PathBuf> {
        fs::create_dir_all(&self.directory).map_err(storage_error)?;
        Ok(self.directory.clone())
    }

    /// 删除日志目录下的所有 `.log` 文件。
    pub fn clear(&self) -> AppResult<()> {
        let _guard = self
            .write_error
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        fs::create_dir_all(&self.directory).map_err(storage_error)?;
        for path in self.log_file_paths()? {
            fs::remove_file(path).map_err(storage_error)?;
        }
        Ok(())
    }

    fn current_file_path(&self) -> PathBuf {
        let date = Local::now().format("%Y-%m-%d");
        self.directory.join(format!("{date}.log"))
    }

    fn log_file_paths(&self) -> AppResult<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(storage_error)? {
            let path = entry.map_err(storage_error)?.path();
            if path.extension() == Some(OsStr::new("log")) {
                paths.push(path);
            }
        }
        paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
        Ok(paths)
    }

    fn recent_entries(paths: &[PathBuf], limit: usize) -> AppResult<Vec<LogEntry>> {
        // 从文件尾按需读取，达到展示上限后不再扫描当天更早的内容。
        let mut entries = Vec::new();
        for path in paths {
            entries.extend(read_recent_entries(path, limit - entries.len())?);
            if entries.len() >= limit {
                break;
            }
        }
        Ok(entries)
    }
}

fn read_recent_entries(path: &Path, limit: usize) -> AppResult<Vec<LogEntry>> {
    let mut file = File::open(path).map_err(storage_error)?;
    let mut position = file.metadata().map_err(storage_error)?.len();
    let mut chunk = [0_u8; 8192];
    let mut line = Vec::new();
    let mut entries = Vec::new();
    // 脱敏摘要不会需要超长行；损坏文件中的无换行内容也不能无限占用内存。
    let mut oversized = false;
    while position > 0 && entries.len() < limit {
        let length = usize::try_from(position.min(chunk.len() as u64))
            .map_err(|error| AppError::Storage(error.to_string()))?;
        position -= length as u64;
        file.seek(SeekFrom::Start(position))
            .map_err(storage_error)?;
        file.read_exact(&mut chunk[..length])
            .map_err(storage_error)?;
        for byte in chunk[..length].iter().rev() {
            if *byte == b'\n' {
                if !oversized {
                    line.reverse();
                    if let Ok(entry) = serde_json::from_slice::<LogEntry>(&line) {
                        entries.push(entry);
                    }
                }
                line.clear();
                oversized = false;
                if entries.len() == limit {
                    break;
                }
            } else if !oversized {
                if line.len() == MAX_LINE_BYTES {
                    line.clear();
                    oversized = true;
                } else {
                    line.push(*byte);
                }
            }
        }
    }
    if entries.len() < limit && !oversized {
        line.reverse();
        if let Ok(entry) = serde_json::from_slice::<LogEntry>(&line) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[allow(clippy::needless_pass_by_value)]
fn storage_error(error: std::io::Error) -> AppError {
    AppError::Storage(error.to_string())
}
