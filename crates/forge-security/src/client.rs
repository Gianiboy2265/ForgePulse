use std::{path::Path, time::Duration};

use forge_core::{
    DEFAULT_PIPE_NAME, ForgeError, Result,
    ipc::{RequestCommand, ResponsePayload},
};
use tokio::{
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
    time::sleep,
};

use crate::{RequestSigner, SecretKey, read_frame, write_frame};

// Windows returns ERROR_PIPE_BUSY when every existing pipe instance is connected. The service
// creates the next listening instance immediately after accepting a client, so callers should
// briefly retry this transient race instead of reporting the service as offline.
const ERROR_PIPE_BUSY: i32 = 231;
const PIPE_BUSY_RETRIES: u8 = 40;
const PIPE_BUSY_RETRY_DELAY: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct AuthenticatedClient {
    pipe_name: String,
    signer: RequestSigner,
}

impl AuthenticatedClient {
    pub fn from_key_file(path: &Path) -> Result<Self> {
        Ok(Self {
            pipe_name: DEFAULT_PIPE_NAME.to_owned(),
            signer: RequestSigner::new(SecretKey::load(path)?),
        })
    }

    #[must_use]
    pub fn with_pipe_name(mut self, pipe_name: impl Into<String>) -> Self {
        self.pipe_name = pipe_name.into();
        self
    }

    pub async fn request(&self, command: RequestCommand) -> Result<ResponsePayload> {
        let request = self.signer.sign(command)?;
        let mut pipe = self.connect().await?;
        write_frame(&mut pipe, &request).await?;
        read_frame(&mut pipe).await
    }

    async fn connect(&self) -> Result<NamedPipeClient> {
        let mut retries_remaining = PIPE_BUSY_RETRIES;
        loop {
            match ClientOptions::new().open(&self.pipe_name) {
                Ok(pipe) => return Ok(pipe),
                Err(error)
                    if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && retries_remaining > 0 =>
                {
                    retries_remaining -= 1;
                    sleep(PIPE_BUSY_RETRY_DELAY).await;
                }
                Err(error) => {
                    return Err(ForgeError::ServiceUnavailable(format!(
                        "could not connect to {}: {error}",
                        self.pipe_name
                    )));
                }
            }
        }
    }
}
