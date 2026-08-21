use std::{future::Future, io, net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::Semaphore,
    task::JoinSet,
    time::timeout,
};

const RELAY_BIND_ENV: &str = "SDKWORK_WEBSERVER_WEBSITE_PROVIDER_EVENT_RELAY_BIND";
const RELAY_TARGET_ENV: &str = "SDKWORK_WEBSERVER_WEBSITE_PROVIDER_EVENT_RELAY_TARGET";
const MAXIMUM_CONNECTIONS: usize = 64;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const CONNECTION_LIFETIME: Duration = Duration::from_secs(30);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
struct ProviderEventRelayConfig {
    bind: SocketAddr,
    target: SocketAddr,
}

impl ProviderEventRelayConfig {
    fn from_env() -> Result<Self, String> {
        let bind = socket_address_from_env(RELAY_BIND_ENV)?;
        let target = socket_address_from_env(RELAY_TARGET_ENV)?;
        validate_addresses(bind, target)?;
        Ok(Self { bind, target })
    }
}

fn validate_addresses(bind: SocketAddr, target: SocketAddr) -> Result<(), String> {
    if !target.ip().is_loopback() {
        return Err(format!("{RELAY_TARGET_ENV} must use a loopback address"));
    }
    if bind == target {
        return Err(format!(
            "{RELAY_BIND_ENV} and {RELAY_TARGET_ENV} must differ"
        ));
    }
    Ok(())
}

pub async fn run_from_env_until<F>(shutdown: F) -> Result<(), io::Error>
where
    F: Future<Output = ()>,
{
    let config = ProviderEventRelayConfig::from_env()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(
        address = %listener.local_addr()?,
        target = %config.target,
        "provider-event relay listener started"
    );
    run_listener_until(listener, config.target, shutdown).await
}

async fn run_listener_until<F>(
    listener: TcpListener,
    target: SocketAddr,
    shutdown: F,
) -> Result<(), io::Error>
where
    F: Future<Output = ()>,
{
    let permits = Arc::new(Semaphore::new(MAXIMUM_CONNECTIONS));
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            biased;
            () = &mut shutdown => break,
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = result {
                    tracing::warn!(%error, "provider-event relay task failed");
                }
            }
            accepted = listener.accept() => {
                let (client, _) = accepted?;
                let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                    drop(client);
                    continue;
                };
                connections.spawn(async move {
                    let _permit = permit;
                    if let Err(error) = relay_connection(client, target).await {
                        tracing::debug!(%error, "provider-event relay connection closed");
                    }
                });
            }
        }
    }

    if timeout(DRAIN_TIMEOUT, async {
        while let Some(result) = connections.join_next().await {
            if let Err(error) = result {
                tracing::warn!(%error, "provider-event relay task failed during drain");
            }
        }
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    Ok(())
}

async fn relay_connection(mut client: TcpStream, target: SocketAddr) -> Result<(), io::Error> {
    client.set_nodelay(true)?;
    let mut upstream = timeout(CONNECT_TIMEOUT, TcpStream::connect(target))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "relay target connect timed out"))??;
    upstream.set_nodelay(true)?;
    timeout(
        CONNECTION_LIFETIME,
        copy_bidirectional(&mut client, &mut upstream),
    )
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "relay connection lifetime exceeded",
        )
    })??;
    Ok(())
}

fn socket_address_from_env(key: &str) -> Result<SocketAddr, String> {
    let value = std::env::var(key).map_err(|_| format!("{key} is required"))?;
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    let address = value
        .parse::<SocketAddr>()
        .map_err(|error| format!("{key} is not a socket address: {error}"))?;
    if address.port() == 0 || address.ip().is_multicast() {
        return Err(format!("{key} must use a non-zero, non-multicast socket"));
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::oneshot,
    };

    use super::*;

    #[test]
    fn relay_target_is_loopback_only_and_cannot_equal_the_bind() {
        assert!(validate_addresses(
            "127.0.0.1:3811".parse().unwrap(),
            "192.0.2.1:3810".parse().unwrap(),
        )
        .is_err());
        assert!(validate_addresses(
            "127.0.0.1:3810".parse().unwrap(),
            "127.0.0.1:3810".parse().unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn relay_preserves_exact_bidirectional_bytes() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream.local_addr().unwrap();
        let relay = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_address = relay.local_addr().unwrap();
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let relay_task = tokio::spawn(async move {
            run_listener_until(relay, upstream_address, async {
                let _ = stop_rx.await;
            })
            .await
        });
        let upstream_task = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            let mut request = [0_u8; 15];
            stream.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"exact-request!!");
            stream.write_all(b"exact-response!").await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let mut client = TcpStream::connect(relay_address).await.unwrap();
        client.write_all(b"exact-request!!").await.unwrap();
        client.shutdown().await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        assert_eq!(response, b"exact-response!");

        upstream_task.await.unwrap();
        let _ = stop_tx.send(());
        relay_task.await.unwrap().unwrap();
    }
}
