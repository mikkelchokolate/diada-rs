use crate::Journal;
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TRANSACTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct SaveError {
    pub path: PathBuf,
    pub cause: io::Error,
}
impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.cause)
    }
}
impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveOutcome {
    pub path: PathBuf,
    pub backup: Option<PathBuf>,
    pub bytes_written: usize,
}

/// Original MAIN copies backups with ReadLn/WriteLn, canonicalizing line endings.
pub fn legacy_backup_copy(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for line in Journal::from_bytes(bytes).lines() {
        output.extend_from_slice(&line.bytes[..line.bytes.len().min(255)]);
        output.extend_from_slice(b"\r\n");
    }
    output
}

/// A save transaction over the user's selected `.DI`, without a new project format.
///
/// The original bytes are retained for rollback. A complete edited journal is
/// staged in STR/ST1; the historical fixed backup name is used when replacing
/// an existing dialogue. A failed replacement restores the original target.
pub struct SaveTransaction {
    path: PathBuf,
    before: Option<Vec<u8>>,
}
impl SaveTransaction {
    pub fn begin(path: impl AsRef<Path>) -> Result<Self, SaveError> {
        let path = path.as_ref().to_owned();
        if !path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("di"))
        {
            return Err(SaveError {
                path,
                cause: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "dialogue target must have a .DI extension",
                ),
            });
        }
        let before = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(cause) if cause.kind() == io::ErrorKind::NotFound => None,
            Err(cause) => return Err(SaveError { path, cause }),
        };
        Ok(Self { path, before })
    }
    pub fn original_bytes(&self) -> Option<&[u8]> {
        self.before.as_deref()
    }

    pub fn commit_current(&self, journal: &Journal) -> Result<SaveOutcome, SaveError> {
        let bytes = journal.to_bytes();
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let working = self.path.with_extension("STR");
        let staged = self.path.with_extension("ST1");
        let mut dimension_suffix = Vec::new();
        let mut in_dimensions = false;
        for line in journal.lines() {
            if line.text() == "_Начало_задания_размеров_механизма" {
                in_dimensions = true;
            }
            if in_dimensions {
                dimension_suffix.extend_from_slice(&line.bytes);
                dimension_suffix.extend_from_slice(b"\r\n");
            }
        }
        write(&working, &dimension_suffix)?;
        write(&staged, &bytes)?;
        let backup = if let Some(previous) = &self.before {
            let path = parent.join("Diada.bak");
            write(&path, &legacy_backup_copy(previous))?;
            Some(path)
        } else {
            None
        };

        let rollback_path = parent.join(format!(
            ".diada-save-{}-{}.tmp",
            std::process::id(),
            TRANSACTION_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let target_exists = self.path.exists();
        if target_exists {
            if rollback_path.exists() {
                return Err(SaveError {
                    path: rollback_path,
                    cause: io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "previous recovery file must be inspected",
                    ),
                });
            }
            fs::rename(&self.path, &rollback_path).map_err(|cause| SaveError {
                path: self.path.clone(),
                cause,
            })?;
        }
        if let Err(cause) = fs::rename(&staged, &self.path) {
            if target_exists {
                let _ = fs::rename(&rollback_path, &self.path);
            }
            return Err(SaveError {
                path: self.path.clone(),
                cause,
            });
        }
        if target_exists {
            fs::remove_file(&rollback_path).map_err(|cause| SaveError {
                path: rollback_path,
                cause,
            })?;
        }
        fs::remove_file(&working).map_err(|cause| SaveError {
            path: working,
            cause,
        })?;
        Ok(SaveOutcome {
            path: self.path.clone(),
            backup,
            bytes_written: bytes.len(),
        })
    }

    /// Restore the exact prior bytes after an explicitly requested rollback.
    pub fn rollback(&self) -> Result<(), SaveError> {
        match &self.before {
            Some(bytes) => write(&self.path, bytes),
            None if self.path.exists() => fs::remove_file(&self.path).map_err(|cause| SaveError {
                path: self.path.clone(),
                cause,
            }),
            None => Ok(()),
        }
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    fs::write(path, bytes).map_err(|cause| SaveError {
        path: path.to_owned(),
        cause,
    })
}
