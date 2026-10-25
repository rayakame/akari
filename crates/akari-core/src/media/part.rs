use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A file being written; deleted when dropped unless `keep` took it.
pub(crate) struct PartFile(Option<PathBuf>);

impl PartFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    /// A name beside `destination` that no other save in this process uses.
    pub(crate) fn beside(destination: &Path) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        Self::new(
            destination.with_file_name(format!("{name}.{}-{number}.part", std::process::id())),
        )
    }

    pub(crate) fn path(&self) -> &Path {
        self.0.as_deref().unwrap_or(Path::new(""))
    }

    /// The file stays: the caller moves it into place.
    pub(crate) fn keep(mut self) -> PathBuf {
        self.0.take().unwrap_or_default()
    }
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::tests::TempDir;

    #[test]
    fn a_dropped_part_is_deleted_and_a_kept_one_stays() {
        let dir = TempDir::new();
        let dropped = PartFile::new(dir.path().join("dropped.part"));
        std::fs::write(dropped.path(), b"half").unwrap();
        let kept = PartFile::new(dir.path().join("kept.part"));
        std::fs::write(kept.path(), b"whole").unwrap();

        drop(dropped);
        let kept = kept.keep();

        assert!(!dir.path().join("dropped.part").exists());
        assert_eq!(std::fs::read(kept).unwrap(), b"whole");
    }

    #[test]
    fn parts_beside_a_destination_share_its_folder_and_never_its_name() {
        let destination = Path::new("/saves/photo.png");

        let first = PartFile::beside(destination).keep();
        let second = PartFile::beside(destination).keep();

        assert_eq!(first.parent(), Some(Path::new("/saves")));
        assert_ne!(first, second);
        assert_ne!(first, destination);
        assert!(first.to_string_lossy().contains("photo.png."));
    }
}
