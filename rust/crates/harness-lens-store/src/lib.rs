// SPDX-License-Identifier: MPL-2.0
// Copyright © 2026 Cristian Camargo Filho

#![doc = include_str!("../README.md")]

use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use harness_lens_core::AnalysisReport;

/// Default maximum serialized report size: 16 MiB.
pub const DEFAULT_MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;
/// Default maximum number of visible reports in one store.
pub const DEFAULT_MAX_REPORTS: usize = 10_000;

/// Validated identifier used as one immutable report filename.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReportKey(String);

impl ReportKey {
    /// Validates a portable, path-safe report key.
    pub fn parse(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        let valid_length = !value.is_empty() && value.len() <= 128;
        let valid_edges = value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            && value
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric);
        let valid_characters = value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));

        if valid_length && valid_edges && valid_characters {
            Ok(Self(value))
        } else {
            Err(StoreError::InvalidKey)
        }
    }

    /// Returns the validated key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Small backend-neutral contract for immutable report records.
pub trait ReportStore {
    /// Writes a new report. Existing keys are never overwritten.
    fn put(&self, key: &ReportKey, report: &AnalysisReport) -> Result<(), StoreError>;

    /// Loads one report by key.
    fn get(&self, key: &ReportKey) -> Result<AnalysisReport, StoreError>;

    /// Lists all report keys in deterministic order.
    fn list(&self) -> Result<Vec<ReportKey>, StoreError>;
}

/// Bounded directory-backed JSON report store.
#[derive(Clone, Debug)]
pub struct DirectoryReportStore {
    root: PathBuf,
    max_report_bytes: usize,
    max_reports: usize,
}

impl DirectoryReportStore {
    /// Creates a store with conservative default limits.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_report_bytes: DEFAULT_MAX_REPORT_BYTES,
            max_reports: DEFAULT_MAX_REPORTS,
        }
    }

    /// Creates a store with explicit nonzero byte and entry limits.
    pub fn with_limits(
        root: impl Into<PathBuf>,
        max_report_bytes: usize,
        max_reports: usize,
    ) -> Result<Self, StoreError> {
        if max_report_bytes == 0 || max_reports == 0 {
            return Err(StoreError::InvalidLimit);
        }
        Ok(Self {
            root: root.into(),
            max_report_bytes,
            max_reports,
        })
    }

    fn path_for(&self, key: &ReportKey) -> PathBuf {
        self.root.join(format!("{}.json", key.as_str()))
    }
}

impl ReportStore for DirectoryReportStore {
    fn put(&self, key: &ReportKey, report: &AnalysisReport) -> Result<(), StoreError> {
        let encoded = serde_json::to_vec(report).map_err(StoreError::InvalidReport)?;
        if encoded.len() > self.max_report_bytes {
            return Err(StoreError::ReportTooLarge {
                limit: self.max_report_bytes,
                actual: encoded.len(),
            });
        }

        fs::create_dir_all(&self.root).map_err(StoreError::Io)?;
        let path = self.path_for(key);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    StoreError::AlreadyExists
                } else {
                    StoreError::Io(error)
                }
            })?;

        if let Err(error) = file.write_all(&encoded).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(StoreError::Io(error));
        }
        Ok(())
    }

    fn get(&self, key: &ReportKey) -> Result<AnalysisReport, StoreError> {
        let path = self.path_for(key);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound
            } else {
                StoreError::Io(error)
            }
        })?;
        if !metadata.file_type().is_file() {
            return Err(StoreError::UnsupportedFileType);
        }

        let declared = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if declared > self.max_report_bytes {
            return Err(StoreError::ReportTooLarge {
                limit: self.max_report_bytes,
                actual: declared,
            });
        }

        let mut encoded = Vec::with_capacity(declared);
        OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(StoreError::Io)?
            .take(self.max_report_bytes as u64 + 1)
            .read_to_end(&mut encoded)
            .map_err(StoreError::Io)?;
        if encoded.len() > self.max_report_bytes {
            return Err(StoreError::ReportTooLarge {
                limit: self.max_report_bytes,
                actual: encoded.len(),
            });
        }

        serde_json::from_slice(&encoded).map_err(StoreError::InvalidReport)
    }

    fn list(&self) -> Result<Vec<ReportKey>, StoreError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StoreError::Io(error)),
        };
        let mut keys = Vec::new();
        for entry in entries {
            let entry = entry.map_err(StoreError::Io)?;
            if !entry.file_type().map_err(StoreError::Io)?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Ok(key) = ReportKey::parse(stem) else {
                continue;
            };
            keys.push(key);
            if keys.len() > self.max_reports {
                return Err(StoreError::TooManyReports {
                    limit: self.max_reports,
                });
            }
        }
        keys.sort();
        Ok(keys)
    }
}

/// Stable failure classes for local report persistence.
#[derive(Debug)]
pub enum StoreError {
    /// Key is empty, too long, nonportable, or unsafe for a filename.
    InvalidKey,
    /// Configured byte or entry limit is zero.
    InvalidLimit,
    /// Immutable record already exists.
    AlreadyExists,
    /// Requested record does not exist.
    NotFound,
    /// Record path is not a regular file; symlinks are rejected.
    UnsupportedFileType,
    /// Serialized or loaded report exceeds configured bound.
    ReportTooLarge {
        /// Configured maximum bytes.
        limit: usize,
        /// Observed bytes.
        actual: usize,
    },
    /// Listing exceeds configured report count.
    TooManyReports {
        /// Configured maximum entries.
        limit: usize,
    },
    /// JSON does not encode a valid Harness Lens report.
    InvalidReport(serde_json::Error),
    /// Local filesystem operation failed.
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => formatter.write_str("invalid report key"),
            Self::InvalidLimit => formatter.write_str("store limits must be nonzero"),
            Self::AlreadyExists => formatter.write_str("report key already exists"),
            Self::NotFound => formatter.write_str("report key not found"),
            Self::UnsupportedFileType => formatter.write_str("report is not a regular file"),
            Self::ReportTooLarge { limit, actual } => {
                write!(formatter, "report has {actual} bytes; limit is {limit}")
            }
            Self::TooManyReports { limit } => {
                write!(formatter, "store contains more than {limit} reports")
            }
            Self::InvalidReport(error) => write!(formatter, "invalid report JSON: {error}"),
            Self::Io(error) => write!(formatter, "report store I/O failed: {error}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidReport(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use harness_lens::{HarnessLensConfig, Scanner};

    use super::{DirectoryReportStore, ReportKey, ReportStore, StoreError};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("harness-lens-store-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn report(root: &Path) -> harness_lens::AnalysisReport {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("AGENTS.md"), "Keep reports content-safe.\n").unwrap();
        Scanner::new()
            .scan(root, &HarnessLensConfig::default())
            .unwrap()
    }

    #[test]
    fn stores_loads_and_lists_immutable_reports() {
        let directory = TestDirectory::new();
        let report = report(&directory.0.join("workspace"));
        let store = DirectoryReportStore::new(directory.0.join("reports"));
        let key = ReportKey::parse("scan-2026-09-11").unwrap();

        store.put(&key, &report).unwrap();

        assert_eq!(store.get(&key).unwrap(), report);
        assert_eq!(store.list().unwrap(), vec![key.clone()]);
        assert!(matches!(
            store.put(&key, &report),
            Err(StoreError::AlreadyExists)
        ));
    }

    #[test]
    fn rejects_unsafe_keys_and_enforces_size_bound() {
        assert!(matches!(
            ReportKey::parse("../outside"),
            Err(StoreError::InvalidKey)
        ));

        let directory = TestDirectory::new();
        let report = report(&directory.0.join("workspace"));
        let store = DirectoryReportStore::with_limits(directory.0.join("reports"), 1, 1).unwrap();
        let key = ReportKey::parse("bounded").unwrap();

        assert!(matches!(
            store.put(&key, &report),
            Err(StoreError::ReportTooLarge { limit: 1, .. })
        ));
    }
}
