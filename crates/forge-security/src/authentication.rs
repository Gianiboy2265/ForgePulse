use std::{
    collections::{HashSet, VecDeque},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use forge_core::{
    ForgeError, IPC_PROTOCOL_VERSION, Result,
    ipc::{AuthenticatedRequest, RequestCommand},
};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 18;
const ALLOWED_CLOCK_SKEW: Duration = Duration::from_secs(30);
const REPLAY_TTL: Duration = Duration::from_secs(60);
const RATE_WINDOW: Duration = Duration::from_secs(10);
const RATE_LIMIT: usize = 120;
const MAX_REPLAY_ENTRIES: usize = 8_192;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_BYTES]);

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("SecretKey")
            .field(&"<redacted>")
            .finish()
    }
}

impl SecretKey {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0_u8; KEY_BYTES];
        getrandom::fill(&mut bytes).map_err(|error| {
            ForgeError::Authentication(format!("secure random generation failed: {error}"))
        })?;
        Ok(Self(bytes))
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ForgeError::io(parent, source))?;
        }
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => {
                let key = Self::generate()?;
                file.write_all(&key.0)
                    .and_then(|()| file.sync_all())
                    .map_err(|source| ForgeError::io(path, source))?;
                Ok(key)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Self::load(path),
            Err(source) => Err(ForgeError::io(path, source)),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|source| ForgeError::io(path, source))?;
        let metadata = file
            .metadata()
            .map_err(|source| ForgeError::io(path, source))?;
        if metadata.len() != KEY_BYTES as u64 {
            return Err(ForgeError::Authentication(
                "IPC key has an invalid length".to_owned(),
            ));
        }
        let mut bytes = [0_u8; KEY_BYTES];
        file.read_exact(&mut bytes)
            .map_err(|source| ForgeError::io(path, source))?;
        Ok(Self(bytes))
    }
}

#[derive(Debug, Clone)]
pub struct RequestSigner {
    key: SecretKey,
}

impl RequestSigner {
    #[must_use]
    pub const fn new(key: SecretKey) -> Self {
        Self { key }
    }

    pub fn sign(&self, command: RequestCommand) -> Result<AuthenticatedRequest> {
        let mut nonce = [0_u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|error| {
            ForgeError::Authentication(format!("secure nonce generation failed: {error}"))
        })?;
        let mut request = AuthenticatedRequest {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: Uuid::new_v4(),
            timestamp_ms: Utc::now().timestamp_millis(),
            nonce: URL_SAFE_NO_PAD.encode(nonce),
            command,
            signature: String::new(),
        };
        request.signature = self.signature_for(&request)?;
        Ok(request)
    }

    pub fn verify_signature(&self, request: &AuthenticatedRequest) -> Result<()> {
        let signature = URL_SAFE_NO_PAD.decode(&request.signature).map_err(|_| {
            ForgeError::Authentication("request signature is not valid base64url".to_owned())
        })?;
        let payload = serde_json::to_vec(&request.unsigned())?;
        let mut mac = HmacSha256::new_from_slice(&self.key.0).map_err(|_| {
            ForgeError::Invariant("HMAC rejected a fixed-size SHA-256 key".to_owned())
        })?;
        mac.update(&payload);
        mac.verify_slice(&signature)
            .map_err(|_| ForgeError::Authentication("request signature does not match".to_owned()))
    }

    fn signature_for(&self, request: &AuthenticatedRequest) -> Result<String> {
        let payload = serde_json::to_vec(&request.unsigned())?;
        let mut mac = HmacSha256::new_from_slice(&self.key.0).map_err(|_| {
            ForgeError::Invariant("HMAC rejected a fixed-size SHA-256 key".to_owned())
        })?;
        mac.update(&payload);
        Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
    }
}

#[derive(Debug)]
pub struct AuthenticationGuard {
    signer: RequestSigner,
    state: Mutex<GuardState>,
}

#[derive(Debug, Default)]
struct GuardState {
    nonces: HashSet<String>,
    nonce_order: VecDeque<(Instant, String)>,
    request_times: VecDeque<Instant>,
}

impl AuthenticationGuard {
    #[must_use]
    pub fn new(key: SecretKey) -> Self {
        Self {
            signer: RequestSigner::new(key),
            state: Mutex::new(GuardState::default()),
        }
    }

    pub fn verify(&self, request: &AuthenticatedRequest) -> Result<()> {
        if request.protocol_version != IPC_PROTOCOL_VERSION {
            return Err(ForgeError::Protocol(format!(
                "protocol {} is unsupported",
                request.protocol_version
            )));
        }
        let now_ms = Utc::now().timestamp_millis();
        let skew_ms = now_ms.abs_diff(request.timestamp_ms);
        if skew_ms > ALLOWED_CLOCK_SKEW.as_millis() as u64 {
            return Err(ForgeError::Authentication(
                "request timestamp is outside the allowed clock window".to_owned(),
            ));
        }
        let nonce = URL_SAFE_NO_PAD.decode(&request.nonce).map_err(|_| {
            ForgeError::Authentication("request nonce is not valid base64url".to_owned())
        })?;
        if nonce.len() != NONCE_BYTES {
            return Err(ForgeError::Authentication(
                "request nonce has an invalid length".to_owned(),
            ));
        }
        self.signer.verify_signature(request)?;

        let now = Instant::now();
        let mut state = self.state.lock();
        while state
            .nonce_order
            .front()
            .is_some_and(|(seen_at, _)| now.duration_since(*seen_at) > REPLAY_TTL)
        {
            if let Some((_, expired)) = state.nonce_order.pop_front() {
                state.nonces.remove(&expired);
            }
        }
        while state
            .request_times
            .front()
            .is_some_and(|seen_at| now.duration_since(*seen_at) > RATE_WINDOW)
        {
            state.request_times.pop_front();
        }
        if state.request_times.len() >= RATE_LIMIT {
            return Err(ForgeError::Authentication(
                "IPC request rate limit exceeded".to_owned(),
            ));
        }
        if state.nonces.contains(&request.nonce) {
            return Err(ForgeError::Authentication(
                "request nonce was already used".to_owned(),
            ));
        }
        if state.nonce_order.len() >= MAX_REPLAY_ENTRIES
            && let Some((_, oldest)) = state.nonce_order.pop_front()
        {
            state.nonces.remove(&oldest);
        }
        state.nonces.insert(request.nonce.clone());
        state.nonce_order.push_back((now, request.nonce.clone()));
        state.request_times.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_replay() -> Result<()> {
        let key = SecretKey::generate()?;
        let request = RequestSigner::new(key.clone()).sign(RequestCommand::Ping)?;
        let guard = AuthenticationGuard::new(key);
        guard.verify(&request)?;
        assert!(guard.verify(&request).is_err());
        Ok(())
    }

    #[test]
    fn key_round_trip() -> Result<()> {
        let directory = tempfile::tempdir().map_err(|source| ForgeError::io("temp", source))?;
        let path = directory.path().join("ipc.key");
        let key = SecretKey::load_or_create(&path)?;
        let signer = RequestSigner::new(key);
        let request = signer.sign(RequestCommand::Status)?;
        RequestSigner::new(SecretKey::load(&path)?).verify_signature(&request)
    }
}
