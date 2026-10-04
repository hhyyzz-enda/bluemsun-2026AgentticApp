//! Rinx's mini-app library: App Hub's signed catalog read by Rinx's own
//! client ([`hub`]), under the app contract (`octosense-app-contract` 1.x,
//! OctoSense ADR 0005). No new publication format or keys: the catalog,
//! packs and anchor are App Hub's; the code that reads them is Rinx's, so
//! an App Hub change does not force a Rinx release.
pub mod hub;
use hub::{Entry, Store};
use octosense_app_contract::{AppManifest, HostLimits};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

pub const HUB_URL: &str = "https://raw.githubusercontent.com/OctoSense-org/OctoSense-App-Hub/main/";
pub const HUB_ANCHOR: &str = "6000284a069ba7cada2925094074e8e0baae07e25d1b7fc31f396c993f363e11";
pub const ARTICLE_ID: &str = "rinx.article-editor";
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub enum Source {
    Remote(String),
    Directory(PathBuf),
}
impl Source {
    fn catalog(&self) -> Result<String, String> {
        match self {
            Self::Remote(base) => hub::Remote::new(base).catalog(),
            Self::Directory(root) => fs::read_to_string(root.join("catalog.json")).map_err(err),
        }
    }
    fn pack(&self, artifact: &str) -> Result<hub::Pack, String> {
        relative(artifact)?;
        match self {
            Self::Remote(base) => hub::Remote::new(base).pack(artifact),
            Self::Directory(root) => {
                let path = root.join(format!("{artifact}.pack.json"));
                if fs::metadata(&path).map_err(err)?.len() > (MAX_BYTES * 2) as u64 {
                    return Err("Bundle download is too large".into());
                }
                serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)
            }
        }
    }
    fn asset(&self, artifact: &str, asset: &str) -> Option<String> {
        relative(artifact).ok()?;
        relative(asset).ok()?;
        Some(match self {
            Self::Remote(base) => format!("{}/{artifact}/{asset}", base.trim_end_matches('/')),
            Self::Directory(root) => root
                .join(artifact)
                .join(asset)
                .to_string_lossy()
                .into_owned(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Available,
    Installed,
    Update,
    Unavailable(String),
}
#[derive(Clone, Debug)]
pub struct App {
    pub id: String,
    pub name: String,
    pub version: String,
    pub subtitle: String,
    pub description: String,
    pub publisher: String,
    pub repository: String,
    pub permissions: Vec<String>,
    pub icon: Option<String>,
    pub status: Status,
    pub installed: bool,
    pub consent: String,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub apps: Vec<App>,
    pub recent: Vec<String>,
    pub warning: Option<String>,
    pub verified: bool,
}
/// An install/open approval binds all reviewed metadata, including permissions,
/// publisher key and artifact, not only an app id or version number.
#[derive(Clone, Debug)]
pub struct Consent {
    pub id: String,
    pub entry: String,
}
#[derive(Clone, Debug)]
pub struct VerifiedBundle {
    pub root: PathBuf,
    pub manifest: AppManifest,
    pub publisher: String,
    pub publisher_key: String,
}
impl VerifiedBundle {
    /// Also call this on the frozen runtime snapshot: never rely on a path
    /// having been verified before another process could have changed it.
    pub fn verify(&self, root: &Path) -> Result<(), String> {
        let local =
            AppManifest::parse(&fs::read_to_string(root.join("manifest.json")).map_err(err)?)?;
        if serde_json::to_value(&local).map_err(err)?
            != serde_json::to_value(&self.manifest).map_err(err)?
        {
            return Err("Installed manifest differs from the verified catalog".into());
        }
        let keys = hub::PublisherKeys::new().with(&self.publisher, &self.publisher_key);
        octosense_app_contract::admit_digest(&local, &octosense_app_contract::digest_dir(root)?, &keys)
    }
}

/// Only capability families the Rinx runtime actually implements are offered.
/// Unknown/new Hub capabilities leave a visible unavailable listing.
pub fn compatible(
    manifest: &AppManifest,
    platform: &str,
    entry: Option<&Entry>,
) -> Result<(), String> {
    AppManifest::parse(&serde_json::to_string(manifest).map_err(err)?)?;
    if manifest.id.starts_with("os.") || manifest.id == ARTICLE_ID {
        return Err("This app ships with its host and cannot be installed from the Hub".into());
    }
    if manifest.agent.is_some() {
        return Err(
            "This app requires a Hub agent profile; Rinx provides explicit Octos services".into(),
        );
    }
    if let Some(entry) = entry
        && let Some(about) = entry.about()?
        && !about.platforms.iter().any(|p| p == platform)
    {
        return Err(format!("The publisher has not listed {platform} support"));
    }
    octosense_app_contract::policy::resolve(manifest, &HostLimits::default())?;
    for cap in &manifest.capabilities {
        if !matches!(cap.as_str(), "storage" | "net" | "images" | "clipboard")
            && !cap.starts_with("matrix.")
            && !cap.starts_with("octos.")
            && !octosense_app_contract::palpo::SERVICES.contains(&cap.as_str())
        {
            return Err(format!("Rinx does not yet provide the {cap} service"));
        }
    }
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
struct History {
    recent: Vec<String>,
}
pub struct Client {
    root: PathBuf,
    source: Source,
    anchor: String,
    platform: String,
    store: Store,
    history: History,
    warning: Option<String>,
    durable: bool,
    _lock: fs::File,
}
impl Client {
    pub fn production(root: PathBuf, platform: &str) -> Result<Self, String> {
        Self::new(
            root,
            Source::Remote(HUB_URL.into()),
            HUB_ANCHOR.into(),
            platform,
        )
    }
    /// One serial worker owns a library; a file lock excludes other processes.
    pub fn new(
        root: PathBuf,
        source: Source,
        anchor: String,
        platform: &str,
    ) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(err)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".lock"))
            .map_err(err)?;
        lock.try_lock()
            .map_err(|_| "Another Rinx window is using this app library".to_string())?;
        recover(&root)?;
        let history = fs::read(root.join("recent.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let mut client = Self {
            store: Store::new(&anchor, &root, HostLimits::default()),
            root,
            source,
            anchor,
            platform: platform.into(),
            history,
            warning: None,
            durable: true,
            _lock: lock,
        };
        match fs::read_to_string(client.root.join("catalog.json")) {
            Ok(json) => {
                if let Err(error) = client.accept(&json) {
                    client.warning = Some(error);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => client.warning = Some(e.to_string()),
        }
        Ok(client)
    }
    fn accept(&mut self, json: &str) -> Result<(), String> {
        let mut next = Store::new(&self.anchor, &self.root, HostLimits::default());
        if let Some(old) = self.store.catalog_json() {
            next.accept_catalog(old)?;
        }
        next.accept_catalog(json)?;
        let catalog = next.catalog().unwrap();
        for entry in &catalog.entries {
            valid_id(entry.app_id())?;
            relative(&entry.artifact)?;
        }
        if let Some(old) = self.store.catalog()
            && catalog.sequence == old.sequence
            && next.catalog_signing_bytes() != self.store.catalog_signing_bytes()
        {
            return Err("The Hub changed an existing catalog sequence".into());
        }
        // Keep a verified withdrawal in memory even if disk persistence fails.
        self.store = next;
        self.durable = false;
        atomic_write(&self.root.join("catalog.json"), json.as_bytes())?;
        self.durable = true;
        Ok(())
    }
    pub fn refresh(&mut self) -> Snapshot {
        self.warning = self.refresh_required().err().map(|e| {
            format!("Could not refresh App Hub. Using verified local information if available. {e}")
        });
        self.snapshot()
    }
    fn refresh_required(&mut self) -> Result<(), String> {
        let json = self.source.catalog()?;
        self.accept(&json)
    }
    pub fn snapshot(&self) -> Snapshot {
        let freshness = self.store.installs_allowed(&hub::today());
        let mut apps: Vec<App> = self
            .store
            .listings()
            .into_iter()
            .map(|listing| {
                let entry = self.store.entry(&listing.app_id).unwrap();
                let installed_version = self.store.installed_version(&listing.app_id);
                let installed = installed_version.is_some();
                let status = if let hub::Status::Withdrawn(reason) = &entry.status {
                    Status::Unavailable(format!("Withdrawn: {reason}"))
                } else if let Err(e) = compatible(&entry.manifest, &self.platform, Some(entry)) {
                    Status::Unavailable(e)
                } else if installed_version.as_deref() == Some(entry.version()) {
                    Status::Installed
                } else if !self.durable {
                    Status::Unavailable("The verified catalog could not be saved".into())
                } else if let Err(e) = &freshness {
                    Status::Unavailable(e.clone())
                } else if installed {
                    Status::Update
                } else {
                    Status::Available
                };
                let about = listing.about.as_ref();
                let icon = about.and_then(|a| a.icon.as_deref()).and_then(|asset| {
                    relative(asset).ok()?;
                    let local = self.store.install_dir(&listing.app_id).join(asset);
                    if matches!(status, Status::Installed) && local.is_file() {
                        Some(local.to_string_lossy().into_owned())
                    } else {
                        self.source.asset(&listing.artifact, asset)
                    }
                });
                App {
                    id: listing.app_id,
                    name: listing.name,
                    version: listing.version,
                    subtitle: about.map(|a| a.subtitle.clone()).unwrap_or_default(),
                    description: about.map(|a| a.description.clone()).unwrap_or_default(),
                    publisher: about
                        .map(|a| a.publisher.name.clone())
                        .unwrap_or(listing.publisher),
                    repository: entry.source.repository.clone(),
                    permissions: listing.permissions,
                    icon,
                    status,
                    installed,
                    consent: serde_json::to_string(entry).unwrap(),
                }
            })
            .collect();
        // A withdrawn/removed catalog entry must not strand an installation:
        // retain a disabled, removable library row without granting execution.
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                if valid_id(&id).is_err() || apps.iter().any(|a| a.id == id) {
                    continue;
                }
                let Some(manifest) = fs::read_to_string(entry.path().join("bundle/manifest.json"))
                    .ok()
                    .and_then(|s| AppManifest::parse(&s).ok())
                else {
                    continue;
                };
                apps.push(App {
                    id,
                    name: manifest.name,
                    version: manifest.version,
                    subtitle: "Installed on this device".into(),
                    description: String::new(),
                    publisher: String::new(),
                    repository: String::new(),
                    permissions: vec![],
                    icon: None,
                    status: Status::Unavailable(
                        "This app is not offered by the verified catalog".into(),
                    ),
                    installed: true,
                    consent: String::new(),
                });
            }
        }
        Snapshot {
            apps,
            recent: self.history.recent.clone(),
            warning: self.warning.clone(),
            verified: self.store.catalog().is_some(),
        }
    }
    fn consent_entry(&self, consent: &Consent) -> Result<&Entry, String> {
        valid_id(&consent.id)?;
        let entry = self
            .store
            .entry(&consent.id)
            .ok_or("This app is no longer in the catalog")?;
        if !entry.status.is_offered() {
            return Err("This app was withdrawn".into());
        }
        if serde_json::to_string(entry).map_err(err)? != consent.entry {
            return Err("App details changed. Review its permissions again".into());
        }
        compatible(&entry.manifest, &self.platform, Some(entry))?;
        Ok(entry)
    }
    pub fn install(&mut self, consent: &Consent) -> Result<Snapshot, String> {
        self.refresh_required()?;
        self.store.installs_allowed(&hub::today())?;
        let artifact = self.consent_entry(consent)?.artifact.clone();
        let pack = self.source.pack(&artifact)?;
        validate_pack(&pack)?;
        let work = Scratch::new(&self.root)?;
        let staged = work.0.join("download");
        hub::unpack(&pack, &staged)?;
        self.refresh_required()?;
        let entry = self.consent_entry(consent)?;
        let verified = VerifiedBundle {
            root: staged.clone(),
            manifest: entry.manifest.clone(),
            publisher: entry.publisher.clone(),
            publisher_key: entry.publisher_key.clone(),
        };
        verified.verify(&staged)?;
        // The shared installer writes into an isolated staging root. A failed
        // validation/copy must never delete the last working installed version.
        let mut prepared = Store::new(
            &self.anchor,
            &work.0.join("verified"),
            HostLimits::default(),
        );
        prepared.accept_catalog(self.store.catalog_json().unwrap())?;
        prepared.install_staged(
            &consent.id,
            &staged,
            &self.store.publisher_keys(),
            &hub::today(),
        )?;
        publish(
            &self.root.join(&consent.id),
            &prepared.install_dir(&consent.id),
        )?;
        Ok(self.snapshot())
    }
    pub fn open(&self, consent: &Consent) -> Result<VerifiedBundle, String> {
        let entry = self.consent_entry(consent)?;
        self.store.may_run(&consent.id)?;
        let bundle = VerifiedBundle {
            root: self.store.install_dir(&consent.id),
            manifest: entry.manifest.clone(),
            publisher: entry.publisher.clone(),
            publisher_key: entry.publisher_key.clone(),
        };
        bundle.verify(&bundle.root)?;
        Ok(bundle)
    }
    pub fn record_open(&mut self, id: &str) -> Result<Snapshot, String> {
        valid_id(id)?;
        self.history.recent.retain(|known| known != id);
        self.history.recent.insert(0, id.into());
        self.history.recent.truncate(24);
        atomic_write(
            &self.root.join("recent.json"),
            &serde_json::to_vec(&self.history).map_err(err)?,
        )?;
        Ok(self.snapshot())
    }
    /// Remove executable bytes and history. Account-scoped app documents are
    /// retained; the UI says so, and a reinstall can recover the user's work.
    pub fn remove(&mut self, id: &str) -> Result<Snapshot, String> {
        valid_id(id)?;
        if id == ARTICLE_ID || id.starts_with("os.") {
            return Err("Built-in apps cannot be removed here".into());
        }
        self.store.remove(id)?;
        self.history.recent.retain(|known| known != id);
        atomic_write(
            &self.root.join("recent.json"),
            &serde_json::to_vec(&self.history).map_err(err)?,
        )?;
        Ok(self.snapshot())
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || id.starts_with('.')
        || id.contains("..")
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
    {
        return Err("Invalid app identity".into());
    }
    Ok(())
}
fn relative(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains(['\\', ':', '?', '#', '%'])
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
        || !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("Invalid bundle-relative path".into());
    }
    Ok(())
}
fn validate_pack(pack: &hub::Pack) -> Result<(), String> {
    if pack.files.len() > 4096 {
        return Err("Too many bundle files".into());
    }
    let mut bytes = 0usize;
    for (path, data) in &pack.files {
        relative(path)?;
        if path.split('/').count() > 32 {
            return Err("Bundle nesting is too deep".into());
        }
        let padding = data.bytes().rev().take_while(|b| *b == b'=').count().min(2);
        bytes = bytes.saturating_add((data.len() / 4 * 3).saturating_sub(padding));
        if bytes > MAX_BYTES {
            return Err("Bundle exceeds the Hub's 8 MB limit".into());
        }
    }
    Ok(())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(err)?;
        file.write_all(bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
        fs::rename(&tmp, path).map_err(err)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
struct Scratch(PathBuf);
impl Scratch {
    fn new(root: &Path) -> Result<Self, String> {
        let path = root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).map_err(err)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn publish(app: &Path, prepared: &Path) -> Result<(), String> {
    fs::create_dir_all(app).map_err(err)?;
    let target = app.join("bundle");
    let old = app.join("bundle.previous");
    recover_app(app)?;
    if target.exists() {
        fs::rename(&target, &old).map_err(err)?;
    }
    if let Err(e) = fs::rename(prepared, &target) {
        let _ = recover_app(app);
        return Err(e.to_string());
    }
    if old.exists() {
        fs::remove_dir_all(old).map_err(err)?;
    }
    Ok(())
}
fn recover_app(app: &Path) -> Result<(), String> {
    let target = app.join("bundle");
    let old = app.join("bundle.previous");
    if old.exists() {
        if target.exists() {
            fs::remove_dir_all(old).map_err(err)?;
        } else {
            fs::rename(old, target).map_err(err)?;
        }
    }
    Ok(())
}
fn recover(root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(err)? {
        let entry = entry.map_err(err)?;
        if entry.file_type().map_err(err)?.is_dir()
            && valid_id(&entry.file_name().to_string_lossy()).is_ok()
        {
            recover_app(&entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
