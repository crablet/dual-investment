//! 本地文件日志。
//!
//! 日志按本机日期写入应用数据目录下的 `logs/YYYY-MM-DD.log`，每行一条 JSON。
//! 写入内容必须是脱敏摘要，不能包含 API Secret、签名 query 或 Binance 原始响应。

use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{Local, Utc};
use uuid::Uuid;

use crate::domain::{AppError, AppResult, LogEntry, LogLevel, LogSnapshot};

const RECENT_ENTRY_LIMIT: usize = 120;

pub struct LogStore {
    directory: PathBuf,
    write_lock: Mutex<()>,
}

impl LogStore {
    /// 打开应用数据目录中的日志目录。
    pub fn open_app_data(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        let directory = data_dir.as_ref().join("logs");
        fs::create_dir_all(&directory).map_err(storage_error)?;
        Ok(Self {
            directory,
            write_lock: Mutex::new(()),
        })
    }

    /// 写入一条脱敏日志。
    pub fn append(&self, level: LogLevel, event: &str, message: &str) -> AppResult<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?;
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
        })
    }

    /// 确保今日日志文件存在，并返回文件路径。
    pub fn ensure_current_file(&self) -> AppResult<PathBuf> {
        let _guard = self
            .write_lock
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
            .write_lock
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
        // 文件名倒序读取，再把单文件内容反转，得到最新日志优先的设置页列表。
        let mut entries = Vec::new();
        for path in paths {
            let mut file_entries = read_entries(path)?;
            file_entries.reverse();
            for entry in file_entries {
                entries.push(entry);
                if entries.len() >= limit {
                    return Ok(entries);
                }
            }
        }
        Ok(entries)
    }
}

fn read_entries(path: &Path) -> AppResult<Vec<LogEntry>> {
    let file = File::open(path).map_err(storage_error)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(storage_error)?;
        if line.trim().is_empty() {
            continue;
        }
        // 单行 JSON 损坏时跳过该行，避免一个坏日志条目让设置页整体打不开。
        if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[allow(clippy::needless_pass_by_value)]
fn storage_error(error: std::io::Error) -> AppError {
    AppError::Storage(error.to_string())
}
