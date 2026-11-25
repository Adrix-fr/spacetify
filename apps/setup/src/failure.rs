//! Why an install or an uninstall stopped, in a form the window can explain.

use std::io;
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Failure {
    /// No space left on the disk.
    DiskFull,
    /// Windows refused access to a file: most often an antivirus.
    Blocked { file: String },
    /// Another program has the file open.
    InUse { file: String },
    /// Anything else, with the message for the details box.
    Other { message: String },
}

impl Failure {
    /// Sorts a file error, naming the file by its name only.
    pub fn from_io(err: &io::Error, path: &Path) -> Self {
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        // ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL, ERROR_ACCESS_DENIED,
        // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
        match err.raw_os_error() {
            Some(112 | 39) => Self::DiskFull,
            Some(5) => Self::Blocked { file },
            Some(32 | 33) => Self::InUse { file },
            _ => match err.kind() {
                io::ErrorKind::StorageFull => Self::DiskFull,
                io::ErrorKind::PermissionDenied => Self::Blocked { file },
                _ => Self::Other {
                    message: format!("{}: {err}", path.display()),
                },
            },
        }
    }

    pub fn other(message: impl std::fmt::Display) -> Self {
        Self::Other {
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DiskFull => f.write_str("disk full"),
            Self::Blocked { file } => write!(f, "access denied: {file}"),
            Self::InUse { file } => write!(f, "file in use: {file}"),
            Self::Other { message } => f.write_str(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_the_common_windows_errors() {
        let path = Path::new(r"C:\x\spacetify-service.exe");
        let of = |code| Failure::from_io(&io::Error::from_raw_os_error(code), path);
        assert_eq!(of(112), Failure::DiskFull);
        assert_eq!(
            of(5),
            Failure::Blocked {
                file: "spacetify-service.exe".into()
            }
        );
        assert_eq!(
            of(32),
            Failure::InUse {
                file: "spacetify-service.exe".into()
            }
        );
        assert!(matches!(of(2), Failure::Other { .. }));
    }
}
