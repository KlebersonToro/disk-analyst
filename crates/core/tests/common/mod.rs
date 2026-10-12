//! Utilidades compartilhadas pelos testes de integração do núcleo. Todo diretório temporário é
//! removido pelo `Drop`, inclusive quando o teste falha.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Diretório temporário com nome único, removido no `Drop`.
pub struct TempBase(PathBuf);

impl TempBase {
    pub fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("da-scan-{tag}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("criar diretório temporário");
        TempBase(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempBase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Link para uma pasta (symlink no Linux, junction no Windows), removido no `Drop` sem tocar no
/// alvo.
pub struct LinkGuard(PathBuf);

impl Drop for LinkGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = fs::remove_file(&self.0);
        #[cfg(windows)]
        let _ = fs::remove_dir(&self.0);
    }
}

#[cfg(unix)]
pub fn make_dir_link(link: &Path, target: &Path) -> LinkGuard {
    std::os::unix::fs::symlink(target, link).expect("criar symlink");
    LinkGuard(link.to_path_buf())
}

#[cfg(windows)]
pub fn make_dir_link(link: &Path, target: &Path) -> LinkGuard {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .expect("executar cmd")
        .status;
    assert!(status.success(), "mklink /J falhou");
    LinkGuard(link.to_path_buf())
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}
