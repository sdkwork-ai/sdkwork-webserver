use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
    future::{pending, Future},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use tokio::{
    sync::watch,
    task::{JoinError, JoinHandle},
    time::{sleep_until, Instant},
};

use super::{
    proxy::{ActiveHealthTransition, ProxyUpstream},
    runtime::RuntimeGeneration,
};

type ProbeFuture =
    Pin<Box<dyn Future<Output = (ScheduledProbe, ActiveHealthTransition)> + Send + 'static>>;

pub(super) struct ActiveHealthSupervisor {
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl ActiveHealthSupervisor {
    pub(super) fn start(generation: Arc<RuntimeGeneration>) -> Option<Self> {
        if generation
            .upstreams
            .values()
            .all(|upstream| upstream.active_health_interval().is_none())
        {
            return None;
        }
        let (stop, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run_scheduler(generation, stop_rx));
        Some(Self {
            stop,
            task: Some(task),
        })
    }

    pub(super) async fn stop(mut self) -> Result<(), JoinError> {
        let _ = self.stop.send(true);
        match self.task.take() {
            Some(task) => task.await,
            None => Ok(()),
        }
    }
}

impl Drop for ActiveHealthSupervisor {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScheduledProbe {
    due: Instant,
    upstream_index: usize,
    target_index: usize,
}

impl Ord for ScheduledProbe {
    fn cmp(&self, other: &Self) -> Ordering {
        self.due
            .cmp(&other.due)
            .then_with(|| self.upstream_index.cmp(&other.upstream_index))
            .then_with(|| self.target_index.cmp(&other.target_index))
    }
}

impl PartialOrd for ScheduledProbe {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

async fn run_scheduler(generation: Arc<RuntimeGeneration>, mut stop: watch::Receiver<bool>) {
    let upstreams = generation
        .upstreams
        .values()
        .filter(|upstream| upstream.active_health_interval().is_some())
        .cloned()
        .collect::<Vec<_>>();
    let mut schedule = initial_schedule(&upstreams);
    let maximum_concurrency = generation.app.config().limits.max_concurrent_health_checks;
    let mut probes = FuturesUnordered::<ProbeFuture>::new();

    loop {
        launch_due_probes(&upstreams, &mut schedule, &mut probes, maximum_concurrency);
        let next_due = (probes.len() < maximum_concurrency)
            .then(|| schedule.peek().map(|probe| probe.0.due))
            .flatten();
        let wait_for_deadline = async move {
            match next_due {
                Some(deadline) => sleep_until(deadline).await,
                None => pending().await,
            }
        };

        tokio::select! {
            biased;
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
            completed = probes.next(), if !probes.is_empty() => {
                if let Some((mut probe, transition)) = completed {
                    let upstream = &upstreams[probe.upstream_index];
                    if transition != ActiveHealthTransition::Unchanged {
                        tracing::info!(
                            config_generation = generation.id,
                            upstream_id = %upstream.id(),
                            target_index = probe.target_index,
                            active_health = transition.as_str(),
                            "upstream active health state changed"
                        );
                    }
                    probe.due = Instant::now()
                        + upstream
                            .active_health_interval()
                            .expect("scheduled upstream retains active health policy");
                    schedule.push(Reverse(probe));
                }
            }
            () = wait_for_deadline => {}
        }
    }
}

fn initial_schedule(upstreams: &[Arc<ProxyUpstream>]) -> BinaryHeap<Reverse<ScheduledProbe>> {
    let now = Instant::now();
    let mut schedule = BinaryHeap::new();
    for (upstream_index, upstream) in upstreams.iter().enumerate() {
        let interval = upstream
            .active_health_interval()
            .expect("filtered upstream has active health policy");
        let target_count = upstream.target_count();
        for target_index in 0..target_count {
            let numerator = interval
                .as_millis()
                .saturating_mul((target_index + 1) as u128);
            let delay_ms = (numerator / target_count as u128)
                .max(1)
                .min(u64::MAX as u128) as u64;
            schedule.push(Reverse(ScheduledProbe {
                due: now + Duration::from_millis(delay_ms),
                upstream_index,
                target_index,
            }));
        }
    }
    schedule
}

fn launch_due_probes(
    upstreams: &[Arc<ProxyUpstream>],
    schedule: &mut BinaryHeap<Reverse<ScheduledProbe>>,
    probes: &mut FuturesUnordered<ProbeFuture>,
    maximum_concurrency: usize,
) {
    let now = Instant::now();
    while probes.len() < maximum_concurrency {
        let Some(Reverse(next)) = schedule.peek().copied() else {
            return;
        };
        if next.due > now {
            return;
        }
        let Reverse(probe) = schedule.pop().expect("peeked scheduled probe exists");
        let upstream = upstreams[probe.upstream_index].clone();
        probes.push(Box::pin(async move {
            let transition = upstream.run_active_health_check(probe.target_index).await;
            (probe, transition)
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::{Reverse, ScheduledProbe};

    #[test]
    fn schedule_orders_earliest_deadline_then_stable_target_identity() {
        let now = Instant::now();
        let mut schedule = std::collections::BinaryHeap::new();
        schedule.push(Reverse(ScheduledProbe {
            due: now + Duration::from_millis(20),
            upstream_index: 0,
            target_index: 0,
        }));
        schedule.push(Reverse(ScheduledProbe {
            due: now + Duration::from_millis(10),
            upstream_index: 1,
            target_index: 2,
        }));
        assert_eq!(schedule.pop().expect("earliest probe").0.target_index, 2);
    }
}
