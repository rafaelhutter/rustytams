pub mod error;
pub mod flow;

/// Check that an ID is a safe single filename component. IDs are used as
/// filenames (e.g. segment persistence uses `{flow_id}.json`), so they must
/// not contain path separators, `..`, or other traversal characters.
/// Uses std::path::Path::file_name() for platform-native checks, plus an
/// explicit backslash rejection for cross-platform safety (Unix allows `\`
/// in filenames but we reject it for portability).
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('\\')
        && std::path::Path::new(id).file_name() == Some(std::ffi::OsStr::new(id))
}
pub mod object;
pub mod pagination;
pub mod rational;
pub mod segment;
pub mod service;
pub mod source;
pub mod tags;
pub mod timerange;
pub mod timestamp;
pub mod webhook;
