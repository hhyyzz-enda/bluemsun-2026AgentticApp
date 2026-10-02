//! Build-owned catalogs. A downloaded manifest cannot register a native app.
use octosense_app_contract::{AppManifest, HostLimits};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path},
};

pub const ARTICLE_ID: &str = "org.octosense.article-editor";
pub const WRITING_STUDIO_ID: &str = "org.octosense.writing-studio";
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NativeApp {
    ArticleEditor,
    WritingStudio,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema: u32,
    source: String,
    repository: String,
    apps: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    directory: String,
    native: Option<NativeApp>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackedCatalog {
    pub repository: String,
    pub apps: Vec<PackedApp>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackedApp {
    pub directory: String,
    pub native: Option<NativeApp>,
    pub manifest: AppManifest,
    pub files: Vec<BundleFile>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BundleFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

fn relative(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || !Path::new(path)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
    {
        return Err(format!("Unsafe bundle path: {path}"));
    }
    Ok(())
}
fn directory(path: &Path) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err("Catalog directories must not be symlinks".into());
    }
    Ok(())
}
fn collect(
    root: &Path,
    dir: &Path,
    files: &mut Vec<BundleFile>,
    size: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 {
        return Err("Bundle is too deep".into());
    }
    directory(dir)?;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Bundle symlinks are not allowed".into());
        }
        if kind.is_dir() {
            collect(root, &entry.path(), files, size, depth + 1)?;
            continue;
        }
        if !kind.is_file() {
            return Err("Only regular bundle files are allowed".into());
        }
        if files.len() >= 512 {
            return Err("Too many bundle files".into());
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        fs::File::open(entry.path())
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        *size += bytes.len();
        if *size > 8 * 1024 * 1024 {
            return Err("Bundle exceeds 8 MiB".into());
        }
        let path = entry
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        relative(&path)?;
        files.push(BundleFile { path, bytes });
    }
    Ok(())
}
/// Same manifest, policy and digest implementation as OctoSense App Hub.
/// Empty source digests are sealed into the build output, never written back.
pub fn pack(root: &Path) -> Result<PackedCatalog, String> {
    let catalog: Catalog = serde_json::from_slice(
        &fs::read(root.join("system-apps.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if catalog.schema != 1 || catalog.source != "apps" {
        return Err("Unsupported system-app catalog".into());
    }
    if !catalog.repository.starts_with("https://") || catalog.repository.contains(['\n', '\r']) {
        return Err("Expected HTTPS repository URL".into());
    }
    directory(&root.join("apps"))?;
    let mut ids = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    let mut apps = Vec::new();
    for entry in catalog.apps {
        relative(&entry.directory)?;
        if entry.directory.contains('/') || !dirs.insert(entry.directory.clone()) {
            return Err("Duplicate or nested app directory".into());
        }
        let app_root = root.join("apps").join(&entry.directory);
        directory(&app_root)?;
        let bundle = app_root.join("bundle");
        let mut files = Vec::new();
        collect(&bundle, &bundle, &mut files, &mut 0, 0)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let file = files
            .iter_mut()
            .find(|f| f.path == "manifest.json")
            .ok_or("Missing manifest.json")?;
        let mut manifest =
            AppManifest::parse(std::str::from_utf8(&file.bytes).map_err(|e| e.to_string())?)?;
        if !ids.insert(manifest.id.clone()) {
            return Err("Duplicate system app id".into());
        }
        if manifest.agent.is_some() {
            return Err("Apps use host-scoped octos services, not agent profiles".into());
        }
        if manifest.integrity.signature.is_some() {
            return Err(
                "System bundles are authenticated by the Rinx build, not a publisher signature"
                    .into(),
            );
        }
        let digest = octosense_app_contract::digest_dir(&bundle)?;
        if !manifest.integrity.bundle_blake3.is_empty()
            && manifest.integrity.bundle_blake3 != digest
        {
            return Err("Stale bundle digest".into());
        }
        manifest.integrity.bundle_blake3 = digest;
        octosense_app_contract::policy::resolve(
            &manifest,
            &HostLimits::default().with_require_signature(false),
        )?;
        file.bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
        match entry.native {
            Some(NativeApp::ArticleEditor) if manifest.id != ARTICLE_ID => {
                return Err("Native entry and stable app id disagree".into());
            }
            Some(NativeApp::WritingStudio) if manifest.id != WRITING_STUDIO_ID => {
                return Err("Native entry and stable app id disagree".into());
            }
            Some(_)
                if files
                    .iter()
                    .any(|f| matches!(f.path.as_str(), "main.splash" | "page.card")) =>
            {
                return Err("Native bundle cannot supply executable script".into());
            }
            None if manifest.id == ARTICLE_ID || manifest.id == WRITING_STUDIO_ID => {
                return Err("Reserved native app id".into());
            }
            None if !files
                .iter()
                .any(|f| matches!(f.path.as_str(), "main.splash" | "page.card")) =>
            {
                return Err("Script bundle needs main.splash or page.card".into());
            }
            _ => {}
        }
        apps.push(PackedApp {
            directory: entry.directory,
            native: entry.native,
            manifest,
            files,
        });
    }
    Ok(PackedCatalog {
        repository: catalog.repository,
        apps,
    })
}
impl PackedApp {
    /// Caller chooses a fresh private directory; never overlays another app.
    pub fn materialize(&self, destination: &Path) -> Result<(), String> {
        fs::create_dir(destination).map_err(|e| e.to_string())?;
        let result = (|| {
            for file in &self.files {
                relative(&file.path)?;
                let path = destination.join(&file.path);
                fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::write(path, &file.bytes).map_err(|e| e.to_string())?;
            }
            self.verify(destination)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(destination);
        }
        result
    }
    pub fn verify(&self, root: &Path) -> Result<(), String> {
        let expected = self
            .files
            .iter()
            .find(|f| f.path == "manifest.json")
            .ok_or("Missing embedded manifest")?;
        if fs::read(root.join("manifest.json")).map_err(|e| e.to_string())? != expected.bytes {
            return Err("Built-in manifest changed".into());
        }
        octosense_app_contract::admit_digest(
            &self.manifest,
            &octosense_app_contract::digest_dir(root)?,
            &octosense_app_contract::RefuseAllSignatures,
        )
    }
}
