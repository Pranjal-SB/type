//! Paths to `file://` URIs and back.

use std::path::{Path, PathBuf};

/// The URI naming `path`, or `None` if it cannot have one.
///
/// `None` means the path is relative. LSP has no way to express that — a
/// document URI is absolute or it is meaningless — and `url` reports it as
/// `Err(())`, an error carrying no information. Translating that to `None` at
/// the boundary is the honest shape.
pub fn path_to_uri(path: &Path) -> Option<lsp_types::Uri> {
    let url = url::Url::from_file_path(path).ok()?;
    url.as_str().parse().ok()
}

/// The path a `file://` URI names, or `None` if it does not name one.
pub fn uri_to_path(uri: &lsp_types::Uri) -> Option<PathBuf> {
    let url: url::Url = uri.as_str().parse().ok()?;
    url.to_file_path().ok()
}
