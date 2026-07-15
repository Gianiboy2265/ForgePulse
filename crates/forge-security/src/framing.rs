use forge_core::{ForgeError, MAX_IPC_FRAME_BYTES, Result};
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = serde_json::to_vec(value)?;
    if payload.len() > MAX_IPC_FRAME_BYTES {
        return Err(ForgeError::Protocol("IPC frame exceeds 1 MiB".to_owned()));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| ForgeError::Protocol("IPC frame length overflowed u32".to_owned()))?;
    writer
        .write_all(&length.to_le_bytes())
        .await
        .map_err(|error| ForgeError::Protocol(format!("writing frame length failed: {error}")))?;
    writer
        .write_all(&payload)
        .await
        .map_err(|error| ForgeError::Protocol(format!("writing frame payload failed: {error}")))?;
    writer
        .flush()
        .await
        .map_err(|error| ForgeError::Protocol(format!("flushing frame failed: {error}")))?;
    Ok(())
}

pub async fn read_frame<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut length_bytes = [0_u8; 4];
    reader
        .read_exact(&mut length_bytes)
        .await
        .map_err(|error| ForgeError::Protocol(format!("reading frame length failed: {error}")))?;
    let length = usize::try_from(u32::from_le_bytes(length_bytes))
        .map_err(|_| ForgeError::Protocol("IPC frame length overflowed usize".to_owned()))?;
    if length == 0 || length > MAX_IPC_FRAME_BYTES {
        return Err(ForgeError::Protocol(format!(
            "IPC frame length {length} is outside the allowed range"
        )));
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|error| ForgeError::Protocol(format!("reading frame payload failed: {error}")))?;
    serde_json::from_slice(&payload).map_err(ForgeError::from)
}

#[cfg(test)]
mod tests {
    use forge_core::ipc::RequestCommand;

    use super::*;

    #[tokio::test]
    async fn frame_round_trip() -> Result<()> {
        let (mut client, mut server) = tokio::io::duplex(512);
        let send =
            tokio::spawn(async move { write_frame(&mut client, &RequestCommand::Ping).await });
        let received: RequestCommand = read_frame(&mut server).await?;
        let send_result = send
            .await
            .map_err(|error| ForgeError::Invariant(format!("writer task failed: {error}")))?;
        send_result?;
        assert_eq!(received, RequestCommand::Ping);
        Ok(())
    }
}
