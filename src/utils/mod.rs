use std::path::{Path, PathBuf};

pub fn resolve_case_path(path: &Path) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }
    if path.extension().is_none() {
        let md = path.with_extension("md");
        if md.exists() {
            return md;
        }
    }
    path.to_path_buf()
}

pub fn is_csv_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("csv"))
}
