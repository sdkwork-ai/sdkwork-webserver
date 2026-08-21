use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};

use sdkwork_webserver_core::{ResourcePressureConfig, ResourceSampleFailurePolicy};
use tokio::{
    sync::watch,
    task::{JoinError, JoinHandle},
    time::{sleep_until, Instant},
};

use super::DataPlaneError;

const REASON_PROCESS_MEMORY: u8 = 1 << 0;
const REASON_CGROUP_MEMORY: u8 = 1 << 1;
const REASON_OPEN_HANDLES: u8 = 1 << 2;
const REASON_EVENT_LOOP_LAG: u8 = 1 << 3;
const REASON_SAMPLE_FAILURE: u8 = 1 << 4;

pub(super) struct ResourcePressureController {
    enabled: bool,
    pressured: AtomicBool,
    reasons: AtomicU8,
}

pub(super) struct ResourcePressureSnapshot {
    pub enabled: bool,
    pub pressured: bool,
    pub reasons: [bool; 5],
}

impl ResourcePressureController {
    pub(super) fn new(enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled,
            pressured: AtomicBool::new(false),
            reasons: AtomicU8::new(0),
        })
    }

    pub(super) fn is_pressured(&self) -> bool {
        self.enabled && self.pressured.load(Ordering::Acquire)
    }

    pub(super) fn snapshot(&self) -> ResourcePressureSnapshot {
        let reasons = self.reasons.load(Ordering::Acquire);
        ResourcePressureSnapshot {
            enabled: self.enabled,
            pressured: self.is_pressured(),
            reasons: [
                reasons & REASON_PROCESS_MEMORY != 0,
                reasons & REASON_CGROUP_MEMORY != 0,
                reasons & REASON_OPEN_HANDLES != 0,
                reasons & REASON_EVENT_LOOP_LAG != 0,
                reasons & REASON_SAMPLE_FAILURE != 0,
            ],
        }
    }

    fn publish(&self, pressured: bool, reasons: u8) {
        self.reasons.store(reasons, Ordering::Release);
        self.pressured.store(pressured, Ordering::Release);
    }
}

pub(super) struct ResourcePressureSupervisor {
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl ResourcePressureSupervisor {
    pub(super) async fn start(
        controller: Arc<ResourcePressureController>,
        policy: ResourcePressureConfig,
    ) -> Result<Self, DataPlaneError> {
        let initial = sample_once(policy.maximum_open_handles).await;
        let mut evaluator = PressureEvaluator::new(policy.clone());
        match initial {
            Ok(sample) => {
                validate_effective_reserves(&policy, &sample)?;
                publish_transition(
                    &controller,
                    evaluator.record_sample(&sample, Duration::ZERO),
                );
            }
            Err(error)
                if policy.sample_failure_policy == ResourceSampleFailurePolicy::FailClosed =>
            {
                return Err(DataPlaneError::ResourcePressureInitialSample {
                    class: error.class(),
                });
            }
            Err(error) => {
                tracing::warn!(
                    error_class = error.class(),
                    "resource pressure initial sample failed open"
                );
            }
        }

        let (stop, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run_sampler(controller, policy, evaluator, stop_rx));
        Ok(Self {
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

impl Drop for ResourcePressureSupervisor {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

async fn run_sampler(
    controller: Arc<ResourcePressureController>,
    policy: ResourcePressureConfig,
    mut evaluator: PressureEvaluator,
    mut stop: watch::Receiver<bool>,
) {
    let interval = Duration::from_millis(policy.sample_interval_ms);
    let mut deadline = Instant::now() + interval;
    loop {
        tokio::select! {
            biased;
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
            () = sleep_until(deadline) => {}
        }

        let woke_at = Instant::now();
        let event_loop_lag = woke_at.saturating_duration_since(deadline);
        let sample = sample_once(policy.maximum_open_handles).await;
        let transition = match sample {
            Ok(sample) => match validate_effective_reserves(&policy, &sample) {
                Ok(()) => evaluator.record_sample(&sample, event_loop_lag),
                Err(error) => {
                    tracing::warn!(%error, "effective resource pressure capacity became unsafe");
                    evaluator.record_failure(ResourceSampleFailurePolicy::FailClosed)
                }
            },
            Err(error) => {
                tracing::warn!(
                    error_class = error.class(),
                    fail_closed =
                        policy.sample_failure_policy == ResourceSampleFailurePolicy::FailClosed,
                    "resource pressure sample failed"
                );
                evaluator.record_failure(policy.sample_failure_policy)
            }
        };
        publish_transition(&controller, transition);

        let scheduled = deadline + interval;
        deadline = if scheduled <= Instant::now() {
            Instant::now() + interval
        } else {
            scheduled
        };
    }
}

async fn sample_once(maximum_open_handles: u64) -> Result<ResourceSample, SampleError> {
    tokio::task::spawn_blocking(move || os::sample(maximum_open_handles))
        .await
        .map_err(|_| SampleError::Worker)?
}

fn validate_effective_reserves(
    policy: &ResourcePressureConfig,
    sample: &ResourceSample,
) -> Result<(), DataPlaneError> {
    if sample
        .cgroup_memory
        .is_some_and(|(_, limit)| limit <= policy.memory_reserve_bytes)
    {
        return Err(DataPlaneError::ResourcePressureCapacity {
            resource: "cgroup-v2-memory",
        });
    }
    if sample.cgroup_memory.is_some_and(|(_, limit)| {
        recovery_threshold(
            limit,
            policy.memory_reserve_bytes,
            policy.memory_recovery_percent,
        ) >= admission_threshold(
            limit,
            policy.memory_reserve_bytes,
            policy.memory_admission_percent,
        )
    }) {
        return Err(DataPlaneError::ResourcePressureCapacity {
            resource: "cgroup-v2-memory-hysteresis",
        });
    }
    let handle_limit = sample
        .host_open_handle_limit
        .map_or(policy.maximum_open_handles, |host| {
            host.min(policy.maximum_open_handles)
        });
    if handle_limit <= policy.open_handle_reserve {
        return Err(DataPlaneError::ResourcePressureCapacity {
            resource: "open-handles",
        });
    }
    if recovery_threshold(
        handle_limit,
        policy.open_handle_reserve,
        policy.open_handle_recovery_percent,
    ) >= admission_threshold(
        handle_limit,
        policy.open_handle_reserve,
        policy.open_handle_admission_percent,
    ) {
        return Err(DataPlaneError::ResourcePressureCapacity {
            resource: "open-handle-hysteresis",
        });
    }
    Ok(())
}

fn publish_transition(controller: &ResourcePressureController, transition: PressureTransition) {
    match transition {
        PressureTransition::Unchanged => {}
        PressureTransition::Pressured(reasons) => {
            controller.publish(true, reasons);
            tracing::warn!(
                process_memory = reasons & REASON_PROCESS_MEMORY != 0,
                cgroup_memory = reasons & REASON_CGROUP_MEMORY != 0,
                open_handles = reasons & REASON_OPEN_HANDLES != 0,
                event_loop_lag = reasons & REASON_EVENT_LOOP_LAG != 0,
                sample_failure = reasons & REASON_SAMPLE_FAILURE != 0,
                "resource pressure admission activated"
            );
        }
        PressureTransition::Recovered => {
            controller.publish(false, 0);
            tracing::info!("resource pressure admission recovered");
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ResourceSample {
    process_memory_bytes: u64,
    cgroup_memory: Option<(u64, u64)>,
    open_handles: u64,
    host_open_handle_limit: Option<u64>,
}

struct PressureEvaluator {
    policy: ResourcePressureConfig,
    pressured: bool,
    pressure_samples: u32,
    recovery_samples: u32,
}

impl PressureEvaluator {
    fn new(policy: ResourcePressureConfig) -> Self {
        Self {
            policy,
            pressured: false,
            pressure_samples: 0,
            recovery_samples: 0,
        }
    }

    fn record_sample(
        &mut self,
        sample: &ResourceSample,
        event_loop_lag: Duration,
    ) -> PressureTransition {
        let pressure_reasons = self.pressure_reasons(sample, event_loop_lag);
        if !self.pressured {
            self.recovery_samples = 0;
            if pressure_reasons == 0 {
                self.pressure_samples = 0;
                return PressureTransition::Unchanged;
            }
            self.pressure_samples = self.pressure_samples.saturating_add(1);
            if self.pressure_samples >= self.policy.consecutive_pressure_samples {
                self.pressured = true;
                self.pressure_samples = 0;
                return PressureTransition::Pressured(pressure_reasons);
            }
            return PressureTransition::Unchanged;
        }

        self.pressure_samples = 0;
        if !self.within_recovery(sample, event_loop_lag) {
            self.recovery_samples = 0;
            return PressureTransition::Unchanged;
        }
        self.recovery_samples = self.recovery_samples.saturating_add(1);
        if self.recovery_samples >= self.policy.consecutive_recovery_samples {
            self.pressured = false;
            self.recovery_samples = 0;
            return PressureTransition::Recovered;
        }
        PressureTransition::Unchanged
    }

    fn record_failure(&mut self, policy: ResourceSampleFailurePolicy) -> PressureTransition {
        if policy == ResourceSampleFailurePolicy::FailOpen {
            if !self.pressured {
                self.pressure_samples = 0;
            }
            self.recovery_samples = 0;
            return PressureTransition::Unchanged;
        }
        if self.pressured {
            self.recovery_samples = 0;
            return PressureTransition::Unchanged;
        }
        self.pressure_samples = self.pressure_samples.saturating_add(1);
        if self.pressure_samples >= self.policy.consecutive_pressure_samples {
            self.pressured = true;
            self.pressure_samples = 0;
            return PressureTransition::Pressured(REASON_SAMPLE_FAILURE);
        }
        PressureTransition::Unchanged
    }

    fn pressure_reasons(&self, sample: &ResourceSample, event_loop_lag: Duration) -> u8 {
        let mut reasons = 0;
        if at_or_above(
            sample.process_memory_bytes,
            admission_threshold(
                self.policy.maximum_process_memory_bytes,
                self.policy.memory_reserve_bytes,
                self.policy.memory_admission_percent,
            ),
        ) {
            reasons |= REASON_PROCESS_MEMORY;
        }
        if sample.cgroup_memory.is_some_and(|(current, limit)| {
            at_or_above(
                current,
                admission_threshold(
                    limit,
                    self.policy.memory_reserve_bytes,
                    self.policy.memory_admission_percent,
                ),
            )
        }) {
            reasons |= REASON_CGROUP_MEMORY;
        }
        let handle_limit = effective_handle_limit(&self.policy, sample);
        if at_or_above(
            sample.open_handles,
            admission_threshold(
                handle_limit,
                self.policy.open_handle_reserve,
                self.policy.open_handle_admission_percent,
            ),
        ) {
            reasons |= REASON_OPEN_HANDLES;
        }
        if event_loop_lag >= Duration::from_millis(self.policy.event_loop_lag_admission_ms) {
            reasons |= REASON_EVENT_LOOP_LAG;
        }
        reasons
    }

    fn within_recovery(&self, sample: &ResourceSample, event_loop_lag: Duration) -> bool {
        let process_memory_ok = below(
            sample.process_memory_bytes,
            recovery_threshold(
                self.policy.maximum_process_memory_bytes,
                self.policy.memory_reserve_bytes,
                self.policy.memory_recovery_percent,
            ),
        );
        let cgroup_memory_ok = sample.cgroup_memory.is_none_or(|(current, limit)| {
            below(
                current,
                recovery_threshold(
                    limit,
                    self.policy.memory_reserve_bytes,
                    self.policy.memory_recovery_percent,
                ),
            )
        });
        let handle_limit = effective_handle_limit(&self.policy, sample);
        let handles_ok = below(
            sample.open_handles,
            recovery_threshold(
                handle_limit,
                self.policy.open_handle_reserve,
                self.policy.open_handle_recovery_percent,
            ),
        );
        let event_loop_ok =
            event_loop_lag < Duration::from_millis(self.policy.event_loop_lag_recovery_ms);
        process_memory_ok && cgroup_memory_ok && handles_ok && event_loop_ok
    }
}

fn effective_handle_limit(policy: &ResourcePressureConfig, sample: &ResourceSample) -> u64 {
    sample
        .host_open_handle_limit
        .map_or(policy.maximum_open_handles, |host| {
            host.min(policy.maximum_open_handles)
        })
}

fn admission_threshold(limit: u64, reserve: u64, percent: u8) -> u64 {
    percentage(limit, percent).min(limit.saturating_sub(reserve))
}

fn recovery_threshold(limit: u64, reserve: u64, percent: u8) -> u64 {
    percentage(limit, percent).min(limit.saturating_sub(reserve))
}

fn percentage(value: u64, percent: u8) -> u64 {
    ((value as u128 * percent as u128) / 100).min(u64::MAX as u128) as u64
}

fn at_or_above(value: u64, threshold: u64) -> bool {
    value >= threshold
}

fn below(value: u64, threshold: u64) -> bool {
    value < threshold
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PressureTransition {
    Unchanged,
    Pressured(u8),
    Recovered,
}

#[derive(Debug)]
enum SampleError {
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    Unsupported,
    #[cfg(target_os = "linux")]
    Read,
    #[cfg(target_os = "linux")]
    Parse,
    #[cfg(target_os = "windows")]
    System,
    Worker,
}

impl SampleError {
    fn class(&self) -> &'static str {
        match self {
            #[cfg(not(any(target_os = "windows", target_os = "linux")))]
            Self::Unsupported => "unsupported-platform",
            #[cfg(target_os = "linux")]
            Self::Read => "resource-read-failed",
            #[cfg(target_os = "linux")]
            Self::Parse => "resource-parse-failed",
            #[cfg(target_os = "windows")]
            Self::System => "resource-system-call-failed",
            Self::Worker => "resource-sampler-task-failed",
        }
    }
}

#[cfg(target_os = "windows")]
mod os {
    use std::mem::size_of;

    use windows_sys::Win32::System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{GetCurrentProcess, GetProcessHandleCount},
    };

    use super::{ResourceSample, SampleError};

    pub(super) fn sample(_maximum_open_handles: u64) -> Result<ResourceSample, SampleError> {
        // SAFETY: GetCurrentProcess returns a process-local pseudo handle with no ownership transfer.
        let process = unsafe { GetCurrentProcess() };
        let mut memory = PROCESS_MEMORY_COUNTERS {
            cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..PROCESS_MEMORY_COUNTERS::default()
        };
        // SAFETY: `memory` is initialized, writable, and its declared size matches the allocation.
        if unsafe { K32GetProcessMemoryInfo(process, &mut memory, memory.cb) } == 0 {
            return Err(SampleError::System);
        }
        let mut handles = 0_u32;
        // SAFETY: `process` is the valid current-process pseudo handle and `handles` is writable.
        if unsafe { GetProcessHandleCount(process, &mut handles) } == 0 {
            return Err(SampleError::System);
        }
        Ok(ResourceSample {
            process_memory_bytes: memory.WorkingSetSize as u64,
            cgroup_memory: None,
            open_handles: handles as u64,
            host_open_handle_limit: None,
        })
    }
}

#[cfg(target_os = "linux")]
mod os {
    use std::{
        fs::{self, File},
        io::{Read, Result as IoResult},
        path::{Component, Path, PathBuf},
    };

    use super::{ResourceSample, SampleError};

    const MAX_PROC_TEXT_BYTES: u64 = 256 * 1024;
    const MAX_CGROUP_VALUE_BYTES: u64 = 128;

    pub(super) fn sample(maximum_open_handles: u64) -> Result<ResourceSample, SampleError> {
        Ok(ResourceSample {
            process_memory_bytes: process_rss_bytes()?,
            cgroup_memory: cgroup_v2_memory()?,
            open_handles: open_file_descriptors(maximum_open_handles)?,
            host_open_handle_limit: host_open_file_limit()?,
        })
    }

    fn process_rss_bytes() -> Result<u64, SampleError> {
        let status = read_bounded_text(Path::new("/proc/self/status"), MAX_PROC_TEXT_BYTES)
            .map_err(|_| SampleError::Read)?;
        let value = status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .and_then(|value| value.split_whitespace().next())
            .ok_or(SampleError::Parse)?
            .parse::<u64>()
            .map_err(|_| SampleError::Parse)?;
        value.checked_mul(1024).ok_or(SampleError::Parse)
    }

    fn open_file_descriptors(maximum: u64) -> Result<u64, SampleError> {
        let mut count = 0_u64;
        for entry in fs::read_dir("/proc/self/fd").map_err(|_| SampleError::Read)? {
            entry.map_err(|_| SampleError::Read)?;
            count = count.saturating_add(1);
            if count > maximum {
                break;
            }
        }
        Ok(count)
    }

    fn host_open_file_limit() -> Result<Option<u64>, SampleError> {
        let limits = read_bounded_text(Path::new("/proc/self/limits"), MAX_PROC_TEXT_BYTES)
            .map_err(|_| SampleError::Read)?;
        let Some(value) = limits
            .lines()
            .find_map(|line| line.strip_prefix("Max open files"))
            .and_then(|value| value.split_whitespace().next())
        else {
            return Err(SampleError::Parse);
        };
        if value == "unlimited" {
            return Ok(None);
        }
        value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| SampleError::Parse)
    }

    fn cgroup_v2_memory() -> Result<Option<(u64, u64)>, SampleError> {
        let cgroup = read_bounded_text(Path::new("/proc/self/cgroup"), MAX_PROC_TEXT_BYTES)
            .map_err(|_| SampleError::Read)?;
        let Some(configured) = cgroup.lines().find_map(|line| {
            let mut parts = line.splitn(3, ':');
            (parts.next() == Some("0") && parts.next() == Some(""))
                .then(|| parts.next())
                .flatten()
        }) else {
            return Ok(None);
        };
        let relative = safe_cgroup_relative_path(configured)?;
        let root = Path::new("/sys/fs/cgroup").join(relative);
        let current_path = root.join("memory.current");
        let maximum_path = root.join("memory.max");
        if !current_path.is_file() || !maximum_path.is_file() {
            return Ok(None);
        }
        let current = parse_cgroup_value(&current_path)?.ok_or(SampleError::Parse)?;
        let maximum = parse_cgroup_value(&maximum_path)?;
        Ok(maximum.map(|limit| (current, limit)))
    }

    fn safe_cgroup_relative_path(configured: &str) -> Result<PathBuf, SampleError> {
        let path = Path::new(configured.trim_start_matches('/'));
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
            && !path.as_os_str().is_empty()
        {
            return Err(SampleError::Parse);
        }
        Ok(path.to_path_buf())
    }

    fn parse_cgroup_value(path: &Path) -> Result<Option<u64>, SampleError> {
        let value =
            read_bounded_text(path, MAX_CGROUP_VALUE_BYTES).map_err(|_| SampleError::Read)?;
        let value = value.trim();
        if value == "max" {
            return Ok(None);
        }
        value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| SampleError::Parse)
    }

    fn read_bounded_text(path: &Path, maximum: u64) -> IoResult<String> {
        let mut bytes = Vec::with_capacity(maximum.min(16 * 1024) as usize);
        File::open(path)?
            .take(maximum + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > maximum {
            return Err(std::io::Error::other(
                "bounded resource file exceeded limit",
            ));
        }
        String::from_utf8(bytes).map_err(std::io::Error::other)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod os {
    use super::{ResourceSample, SampleError};

    pub(super) fn sample(_maximum_open_handles: u64) -> Result<ResourceSample, SampleError> {
        Err(SampleError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sdkwork_webserver_core::{ResourcePressureConfig, ResourceSampleFailurePolicy};

    use super::{
        os, validate_effective_reserves, PressureEvaluator, PressureTransition, ResourceSample,
        REASON_CGROUP_MEMORY, REASON_EVENT_LOOP_LAG, REASON_OPEN_HANDLES, REASON_PROCESS_MEMORY,
        REASON_SAMPLE_FAILURE,
    };

    fn policy() -> ResourcePressureConfig {
        ResourcePressureConfig {
            sample_interval_ms: 100,
            maximum_process_memory_bytes: 1_000,
            memory_reserve_bytes: 100,
            memory_admission_percent: 90,
            memory_recovery_percent: 70,
            maximum_open_handles: 100,
            open_handle_reserve: 10,
            open_handle_admission_percent: 90,
            open_handle_recovery_percent: 70,
            event_loop_lag_admission_ms: 100,
            event_loop_lag_recovery_ms: 20,
            consecutive_pressure_samples: 2,
            consecutive_recovery_samples: 2,
            operations_reserve_requests: 2,
            sample_failure_policy: ResourceSampleFailurePolicy::FailClosed,
        }
    }

    fn sample(memory: u64, cgroup: Option<(u64, u64)>, handles: u64) -> ResourceSample {
        ResourceSample {
            process_memory_bytes: memory,
            cgroup_memory: cgroup,
            open_handles: handles,
            host_open_handle_limit: None,
        }
    }

    #[test]
    fn pressure_requires_consecutive_samples_and_recovery_uses_hysteresis() {
        let mut evaluator = PressureEvaluator::new(policy());
        let high = sample(900, None, 20);
        assert_eq!(
            evaluator.record_sample(&high, Duration::ZERO),
            PressureTransition::Unchanged
        );
        assert_eq!(
            evaluator.record_sample(&high, Duration::ZERO),
            PressureTransition::Pressured(REASON_PROCESS_MEMORY)
        );

        let middle = sample(750, None, 20);
        assert_eq!(
            evaluator.record_sample(&middle, Duration::ZERO),
            PressureTransition::Unchanged
        );
        let low = sample(600, None, 20);
        assert_eq!(
            evaluator.record_sample(&low, Duration::ZERO),
            PressureTransition::Unchanged
        );
        assert_eq!(
            evaluator.record_sample(&low, Duration::ZERO),
            PressureTransition::Recovered
        );
    }

    #[test]
    fn all_resource_classes_contribute_bounded_reason_bits() {
        let evaluator = PressureEvaluator::new(policy());
        let reasons = evaluator.pressure_reasons(
            &sample(900, Some((900, 1_000)), 90),
            Duration::from_millis(100),
        );
        assert_eq!(
            reasons,
            REASON_PROCESS_MEMORY
                | REASON_CGROUP_MEMORY
                | REASON_OPEN_HANDLES
                | REASON_EVENT_LOOP_LAG
        );
    }

    #[test]
    fn fail_closed_sampling_errors_transition_but_fail_open_does_not() {
        let mut closed = PressureEvaluator::new(policy());
        assert_eq!(
            closed.record_failure(ResourceSampleFailurePolicy::FailClosed),
            PressureTransition::Unchanged
        );
        assert_eq!(
            closed.record_failure(ResourceSampleFailurePolicy::FailClosed),
            PressureTransition::Pressured(REASON_SAMPLE_FAILURE)
        );

        let mut open = PressureEvaluator::new(policy());
        assert_eq!(
            open.record_failure(ResourceSampleFailurePolicy::FailOpen),
            PressureTransition::Unchanged
        );
    }

    #[test]
    fn rejects_dynamic_capacity_that_collapses_effective_hysteresis() {
        let cgroup_error = validate_effective_reserves(
            &policy(),
            &ResourceSample {
                process_memory_bytes: 100,
                cgroup_memory: Some((100, 150)),
                open_handles: 10,
                host_open_handle_limit: None,
            },
        )
        .expect_err("cgroup reserve collapses effective hysteresis");
        assert!(cgroup_error
            .to_string()
            .contains("cgroup-v2-memory-hysteresis"));

        let handle_error = validate_effective_reserves(
            &policy(),
            &ResourceSample {
                process_memory_bytes: 100,
                cgroup_memory: None,
                open_handles: 10,
                host_open_handle_limit: Some(20),
            },
        )
        .expect_err("host handle limit collapses effective hysteresis");
        assert!(handle_error.to_string().contains("open-handle-hysteresis"));
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn current_platform_sampler_reports_nonzero_process_resources() {
        let sampled = os::sample(1_048_576).expect("sample current process resources");
        assert!(sampled.process_memory_bytes > 0);
        assert!(sampled.open_handles > 0);
    }
}
