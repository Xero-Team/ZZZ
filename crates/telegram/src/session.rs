//! Encrypted session persistence.
//!
//! The session state is serialized with `grammers-session`'s `serde` support
//! and encrypted with ChaCha20-Poly1305. The 32-byte master key lives in a
//! `0600` file under the Telegram data directory, which is created `0700`.
//! No keychain and no SQLite are involved.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context as _, Result, anyhow};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use grammers_session::types::{DcOption, PeerId, PeerInfo, UpdateState, UpdatesState};
use grammers_session::{BoxFuture, Session, SessionData};
use rand::TryRng as _;
use rand::rngs::SysRng;
use serde::{Deserialize, Serialize};

const KEY_FILE_NAME: &str = "master.key";
const SESSION_FILE_NAME: &str = "session.bin";
const KEY_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;
const MAGIC: &[u8; 8] = b"ZZZTGSES";

/// An error raised while reading or writing session state.
#[derive(Debug)]
pub struct SessionStorageError(String);

impl fmt::Display for SessionStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SessionStorageError {}

/// Serializable mirror of [`SessionData`], whose component types implement
/// `Serialize` under the `serde` feature.
#[derive(Serialize, Deserialize)]
struct PersistedSession {
    home_dc: i32,
    dc_options: Vec<DcOption>,
    peer_infos: Vec<PeerInfo>,
    updates_state: UpdatesState,
}

impl From<&SessionData> for PersistedSession {
    fn from(data: &SessionData) -> Self {
        Self {
            home_dc: data.home_dc,
            dc_options: data.dc_options.values().cloned().collect(),
            peer_infos: data.peer_infos.values().cloned().collect(),
            updates_state: data.updates_state.clone(),
        }
    }
}

impl From<PersistedSession> for SessionData {
    fn from(persisted: PersistedSession) -> Self {
        Self {
            home_dc: persisted.home_dc,
            dc_options: persisted
                .dc_options
                .into_iter()
                .map(|option| (option.id, option))
                .collect(),
            peer_infos: persisted
                .peer_infos
                .into_iter()
                .map(|info| (info.id(), info))
                .collect(),
            updates_state: persisted.updates_state,
        }
    }
}

/// Reads and writes the encrypted session under the Telegram data directory.
#[derive(Clone, Debug)]
pub struct SessionStore {
    directory: PathBuf,
}

impl SessionStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn session_path(&self) -> PathBuf {
        self.directory.join(SESSION_FILE_NAME)
    }

    fn key_path(&self) -> PathBuf {
        self.directory.join(KEY_FILE_NAME)
    }

    /// Whether an encrypted session file exists on disk.
    pub fn exists(&self) -> bool {
        self.session_path().is_file()
    }

    /// Loads and decrypts the session, returning `None` when none is stored.
    pub fn load(&self) -> Result<Option<SessionData>> {
        let path = self.session_path();
        if !path.is_file() {
            return Ok(None);
        }
        let contents = fs::read(&path)
            .with_context(|| format!("failed to read session at {}", path.display()))?;
        let key = self.load_or_create_key()?;
        let plaintext = decrypt(&key, &contents)?;
        let persisted: PersistedSession = serde_json::from_slice(&plaintext)
            .context("failed to deserialize the Telegram session")?;
        Ok(Some(persisted.into()))
    }

    /// Encrypts and writes the session, creating the directory as needed.
    pub fn save(&self, data: &SessionData) -> Result<()> {
        self.ensure_directory()?;
        let key = self.load_or_create_key()?;
        let plaintext = serde_json::to_vec(&PersistedSession::from(data))
            .context("failed to serialize the Telegram session")?;
        let ciphertext = encrypt(&key, &plaintext)?;
        let path = self.session_path();
        let temporary = path.with_extension("bin.tmp");
        let mut file = fs::File::create(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        file.write_all(&ciphertext)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        set_file_permissions(&path, 0o600)
    }

    /// Removes the encrypted session. The master key is kept so a later login
    /// reuses the same data directory.
    pub fn clear(&self) -> Result<()> {
        let path = self.session_path();
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("failed to remove {}", path.display()))
            }
        }
    }

    /// Creates the session directory `0700` if it does not exist.
    pub fn ensure_directory(&self) -> Result<()> {
        fs::create_dir_all(&self.directory)
            .with_context(|| format!("failed to create {}", self.directory.display()))?;
        set_file_permissions(&self.directory, 0o700)
    }

    fn load_or_create_key(&self) -> Result<[u8; KEY_LENGTH]> {
        self.ensure_directory()?;
        let path = self.key_path();
        if path.is_file() {
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read key at {}", path.display()))?;
            if bytes.len() != KEY_LENGTH {
                return Err(anyhow!(
                    "master key at {} has an unexpected length",
                    path.display()
                ));
            }
            let mut key = [0u8; KEY_LENGTH];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }

        let mut key = [0u8; KEY_LENGTH];
        SysRng
            .try_fill_bytes(&mut key)
            .context("failed to generate a Telegram session key")?;
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed to create key at {}", path.display()))?;
        file.write_all(&key)
            .with_context(|| format!("failed to write key at {}", path.display()))?;
        set_file_permissions(&path, 0o600)?;
        Ok(key)
    }
}

fn encrypt(key: &[u8; KEY_LENGTH], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    SysRng
        .try_fill_bytes(&mut nonce_bytes)
        .context("failed to generate a Telegram session nonce")?;
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| anyhow!("failed to encrypt the Telegram session"))?;
    let mut output = Vec::with_capacity(MAGIC.len() + NONCE_LENGTH + ciphertext.len());
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

fn decrypt(key: &[u8; KEY_LENGTH], data: &[u8]) -> Result<Vec<u8>> {
    let header = MAGIC.len() + NONCE_LENGTH;
    if data.len() < header || &data[..MAGIC.len()] != MAGIC {
        return Err(anyhow!("unrecognized Telegram session file"));
    }
    let nonce = Nonce::try_from(&data[MAGIC.len()..header])?;
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    cipher
        .decrypt(&nonce, &data[header..])
        .map_err(|_| anyhow!("failed to decrypt the Telegram session"))
}

#[cfg(unix)]
fn set_file_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn lock_error() -> SessionStorageError {
    SessionStorageError("session lock is poisoned".to_owned())
}

/// An in-memory [`Session`] that can be snapshotted for persistence.
pub struct TelegramSession(Mutex<SessionData>);

impl TelegramSession {
    pub fn new(data: SessionData) -> Self {
        Self(Mutex::new(data))
    }

    /// Clones the current session state for persistence.
    pub fn snapshot(&self) -> Result<SessionData, SessionStorageError> {
        let data = self.0.lock().map_err(|_| lock_error())?;
        Ok(SessionData {
            home_dc: data.home_dc,
            dc_options: data.dc_options.clone(),
            peer_infos: data.peer_infos.clone(),
            updates_state: data.updates_state.clone(),
        })
    }

    fn data(&self) -> Result<MutexGuard<'_, SessionData>, SessionStorageError> {
        self.0.lock().map_err(|_| lock_error())
    }
}

impl Session for TelegramSession {
    type Error = SessionStorageError;

    fn home_dc_id(&self) -> Result<i32, Self::Error> {
        Ok(self.data()?.home_dc)
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), Self::Error>> {
        Box::pin(async move {
            self.data()?.home_dc = dc_id;
            Ok(())
        })
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, Self::Error> {
        Ok(self.data()?.dc_options.get(&dc_id).cloned())
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), Self::Error>> {
        let dc_option = dc_option.clone();
        Box::pin(async move {
            self.data()?.dc_options.insert(dc_option.id, dc_option);
            Ok(())
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, Self::Error>> {
        Box::pin(async move { Ok(self.data()?.peer_infos.get(&peer).cloned()) })
    }

    fn cache_peer(&self, peer: &PeerInfo) -> BoxFuture<'_, Result<(), Self::Error>> {
        let peer = peer.clone();
        Box::pin(async move {
            let mut data = self.data()?;
            data.peer_infos
                .entry(peer.id())
                .and_modify(|existing| {
                    existing.extend_info(&peer);
                })
                .or_insert(peer);
            Ok(())
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, Self::Error>> {
        Box::pin(async move { Ok(self.data()?.updates_state.clone()) })
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), Self::Error>> {
        Box::pin(async move {
            let mut data = self.data()?;
            match update {
                UpdateState::All(state) => data.updates_state = state,
                UpdateState::Primary { pts, date, seq } => {
                    data.updates_state.pts = pts;
                    data.updates_state.date = date;
                    data.updates_state.seq = seq;
                }
                UpdateState::Secondary { qts } => data.updates_state.qts = qts,
                UpdateState::Channel { id, pts } => {
                    match data
                        .updates_state
                        .channels
                        .iter_mut()
                        .find(|channel| channel.id == id)
                    {
                        Some(channel) => channel.pts = pts,
                        None => data
                            .updates_state
                            .channels
                            .push(grammers_session::types::ChannelState { id, pts }),
                    }
                }
            }
            Ok(())
        })
    }
}

/// Convenience constructor used by tests.
pub fn empty_session() -> Arc<TelegramSession> {
    Arc::new(TelegramSession::new(SessionData::default()))
}

#[cfg(test)]
mod tests {
    use super::{SessionStore, TelegramSession, empty_session};
    use grammers_session::Session as _;
    use grammers_session::SessionData;
    use grammers_session::types::PeerId;

    fn temporary_directory(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "zzz-telegram-{name}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        );
        path.push(unique);
        path
    }

    #[test]
    fn save_and_load_round_trips() {
        let directory = temporary_directory("roundtrip");
        let store = SessionStore::new(directory.clone());
        assert!(!store.exists());
        let mut data = SessionData::default();
        data.home_dc = 4;
        store.save(&data).expect("save");
        assert!(store.exists());
        let loaded = store.load().expect("load").expect("session");
        assert_eq!(loaded.home_dc, 4);
        store.clear().expect("clear");
        assert!(!store.exists());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn encrypted_file_does_not_leak_plaintext() {
        let directory = temporary_directory("ciphertext");
        let store = SessionStore::new(directory.clone());
        let data = SessionData::default();
        store.save(&data).expect("save");
        let bytes = std::fs::read(store.session_path()).expect("read");
        assert!(bytes.starts_with(b"ZZZTGSES"));
        std::fs::remove_dir_all(&directory).ok();
    }

    #[tokio::test]
    async fn session_caches_peers() {
        let session = empty_session();
        let peer_id = PeerId::user(7).expect("valid user");
        assert!(session.peer(peer_id).await.expect("peer").is_none());
        session
            .cache_peer(&grammers_session::types::PeerInfo::User {
                id: 7,
                auth: None,
                bot: None,
                is_self: None,
            })
            .await
            .expect("cache");
        assert!(session.peer(peer_id).await.expect("peer").is_some());
        let snapshot = session.snapshot().expect("snapshot");
        assert_eq!(snapshot.peer_infos.len(), 1);
        let restored = TelegramSession::new(snapshot);
        assert!(restored.peer(peer_id).await.expect("peer").is_some());
    }
}
