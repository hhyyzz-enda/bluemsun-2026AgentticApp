//! Remember only a verified built-in Palpo package's reviewed grant, per account.
//! The current package digest binds every requested operation and its UI bytes.
use std::path::PathBuf;

fn path(account: &str) -> PathBuf {
    let key = blake3::hash(account.as_bytes()).to_hex().to_string();
    crate::app_data_dir()
        .join("miniapps/grants")
        .join(key)
        .join("palpo.json")
}
pub fn remembered(account: &str, digest: &str) -> bool {
    std::fs::read_to_string(path(account))
        .ok()
        .is_some_and(|s| s == digest)
}
pub fn remember(account: &str, digest: &str) -> Result<(), String> {
    use std::io::Write;
    let path = path(account);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|_| "Could not save app consent")?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(digest.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|_| "Could not save app consent".into())
}
