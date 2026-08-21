use std::{
    panic::AssertUnwindSafe,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use futures_util::FutureExt;
use hyper::upgrade::OnUpgrade;
use hyper_util::rt::TokioIo;
use tokio::{
    io::{copy_bidirectional_with_sizes, AsyncRead, AsyncWrite},
    sync::{watch, Notify, OwnedSemaphorePermit},
    time::{sleep, timeout},
};

use super::{metrics::DataPlaneMetrics, proxy::TargetActivityLease, runtime::RuntimeGeneration};

const TUNNEL_COPY_BUFFER_BYTES: usize = 16 * 1024;

pub(crate) struct TunnelSupervisor {
    accepting: AtomicBool,
    shutdown: watch::Sender<bool>,
    active: AtomicUsize,
    drained: Notify,
    maximum_lifetime: Duration,
    metrics: Arc<DataPlaneMetrics>,
}

impl TunnelSupervisor {
    pub(crate) fn new(maximum_lifetime: Duration, metrics: Arc<DataPlaneMetrics>) -> Arc<Self> {
        let (shutdown, _) = watch::channel(false);
        Arc::new(Self {
            accepting: AtomicBool::new(true),
            shutdown,
            active: AtomicUsize::new(0),
            drained: Notify::new(),
            maximum_lifetime,
            metrics,
        })
    }

    pub(crate) fn try_spawn(
        self: &Arc<Self>,
        downstream: OnUpgrade,
        upstream: OnUpgrade,
        upstream_permit: OwnedSemaphorePermit,
        generation: Arc<RuntimeGeneration>,
        target_activity: TargetActivityLease,
    ) -> Result<(), ()> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(());
        }
        self.active.fetch_add(1, Ordering::AcqRel);
        if !self.accepting.load(Ordering::Acquire) {
            if self.active.fetch_sub(1, Ordering::AcqRel) == 1 {
                self.drained.notify_waiters();
            }
            return Err(());
        }
        self.metrics.tunnel_opened();

        let supervisor = self.clone();
        let shutdown = self.shutdown.subscribe();
        let maximum_lifetime = self.maximum_lifetime;
        let metrics = self.metrics.clone();
        tokio::spawn(async move {
            let _active = ActiveTunnel::new(supervisor);
            let _upstream_permit = upstream_permit;
            let _generation = generation;
            let _target_activity = target_activity;
            if AssertUnwindSafe(run_tunnel(
                downstream,
                upstream,
                shutdown,
                maximum_lifetime,
                metrics,
            ))
            .catch_unwind()
            .await
            .is_err()
            {
                tracing::error!("WebSocket tunnel task panicked");
            }
        });
        Ok(())
    }

    pub(crate) async fn stop_and_drain(&self, drain_timeout: Duration) -> bool {
        self.accepting.store(false, Ordering::Release);
        self.metrics.tunnel_shutdown_started();
        let _ = self.shutdown.send(true);
        let drained = timeout(drain_timeout, async {
            loop {
                let notified = self.drained.notified();
                if self.active.load(Ordering::Acquire) == 0 {
                    return;
                }
                notified.await;
            }
        })
        .await
        .is_ok();
        if !drained {
            self.metrics.tunnel_drain_timed_out();
        }
        drained
    }

    pub(crate) fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }

    fn finish_one(&self) {
        self.metrics.tunnel_closed();
        if self.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.drained.notify_waiters();
        }
    }
}

struct ActiveTunnel {
    supervisor: Arc<TunnelSupervisor>,
}

impl ActiveTunnel {
    fn new(supervisor: Arc<TunnelSupervisor>) -> Self {
        Self { supervisor }
    }
}

impl Drop for ActiveTunnel {
    fn drop(&mut self) {
        self.supervisor.finish_one();
    }
}

async fn run_tunnel(
    downstream: OnUpgrade,
    upstream: OnUpgrade,
    mut shutdown: watch::Receiver<bool>,
    maximum_lifetime: Duration,
    metrics: Arc<DataPlaneMetrics>,
) {
    let lifetime = sleep(maximum_lifetime);
    tokio::pin!(lifetime);
    let upgraded = tokio::select! {
        biased;
        () = wait_for_shutdown(&mut shutdown) => return,
        () = &mut lifetime => return,
        result = async { tokio::try_join!(downstream, upstream) } => result,
    };
    let (downstream, upstream) = match upgraded {
        Ok(upgraded) => upgraded,
        Err(error) => {
            tracing::debug!(%error, "WebSocket upgrade failed before tunnel establishment");
            return;
        }
    };

    let mut downstream = TokioIo::new(downstream);
    let mut upstream = TokioIo::new(upstream);
    tokio::select! {
        biased;
        () = wait_for_shutdown(&mut shutdown) => {}
        () = &mut lifetime => {}
        result = copy_tunnel(&mut downstream, &mut upstream) => {
            match result {
                Ok((downstream_to_upstream, upstream_to_downstream)) => {
                    metrics.record_tunnel_bytes(downstream_to_upstream, upstream_to_downstream);
                }
                Err(error) => {
                    tracing::debug!(%error, "WebSocket tunnel closed with an I/O error");
                }
            }
        }
    }
}

async fn copy_tunnel<D, U>(downstream: &mut D, upstream: &mut U) -> std::io::Result<(u64, u64)>
where
    D: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    copy_bidirectional_with_sizes(
        downstream,
        upstream,
        TUNNEL_COPY_BUFFER_BYTES,
        TUNNEL_COPY_BUFFER_BYTES,
    )
    .await
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::copy_tunnel;
    use crate::{
        data_plane::metrics::DataPlaneMetrics, metric_dimensions::CanonicalMetricDimensions,
    };

    #[tokio::test]
    async fn bidirectional_copy_reports_authoritative_directional_bytes() {
        let (mut downstream, mut client) = tokio::io::duplex(64);
        let (mut upstream, mut origin) = tokio::io::duplex(64);
        let copy = tokio::spawn(async move { copy_tunnel(&mut downstream, &mut upstream).await });

        client
            .write_all(b"down")
            .await
            .expect("write first downstream segment");
        client
            .write_all(b"stream")
            .await
            .expect("write second downstream segment");
        client.shutdown().await.expect("finish downstream writes");
        origin
            .write_all(b"up")
            .await
            .expect("write first upstream segment");
        origin
            .write_all(b"stream")
            .await
            .expect("write second upstream segment");
        origin.shutdown().await.expect("finish upstream writes");

        let mut client_received = Vec::new();
        let mut origin_received = Vec::new();
        let (client_result, origin_result) = tokio::join!(
            client.read_to_end(&mut client_received),
            origin.read_to_end(&mut origin_received),
        );
        client_result.expect("read upstream tunnel bytes");
        origin_result.expect("read downstream tunnel bytes");
        assert_eq!(client_received, b"upstream");
        assert_eq!(origin_received, b"downstream");

        let copied = copy
            .await
            .expect("copy task joins")
            .expect("copy completes");
        assert_eq!(copied, (10, 8));
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        metrics.record_tunnel_bytes(copied.0, copied.1);
        assert_eq!(metrics.tunnel_bytes(0), 10);
        assert_eq!(metrics.tunnel_bytes(1), 8);
    }
}
