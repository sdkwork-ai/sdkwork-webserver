use super::*;
use sdkwork_webserver_resolver_cache::ResolutionChain;

impl ProxyUpstream<UpstreamClient> {
    pub(crate) fn build(
        app: &CompiledWebServerApp,
        config: &UpstreamConfig,
        resolver: Arc<BoundedSystemResolver>,
        metrics: Arc<DataPlaneMetrics>,
        resolution_chain: Option<Arc<ResolutionChain>>,
    ) -> Result<Self, DataPlaneError> {
        let resolver = Arc::new(match resolution_chain {
            Some(chain) => GuardedDnsResolver::with_chain(
                resolver,
                config.address_policy.clone(),
                metrics,
                chain,
            ),
            None => GuardedDnsResolver::new_observed(
                resolver,
                config.address_policy.clone(),
                metrics,
            ),
        });
        let client = UpstreamClient::build(app, config, resolver)?;
        let targets = config
            .targets
            .iter()
            .map(|target| {
                Url::parse(&target.url)
                    .map(|url| ProxyTarget {
                        active_health_url: config
                            .active_health
                            .as_ref()
                            .map(|policy| active_health_url(&url, &policy.uri)),
                        url,
                        weight: usize::from(target.weight),
                        backup: target.backup,
                        slow_start_duration_ms: target.slow_start_ms.unwrap_or(0),
                        slow_start_started_ms: AtomicU64::new(0),
                        active_requests: Arc::new(AtomicUsize::new(0)),
                        consecutive_failures: AtomicU32::new(0),
                        ejected_until_ms: AtomicU64::new(0),
                        probe_in_flight: AtomicBool::new(false),
                        active_available: AtomicBool::new(true),
                        active_failures: AtomicU32::new(0),
                        active_successes: AtomicU32::new(0),
                    })
                    .map_err(|_| DataPlaneError::InvalidUpstreamTarget {
                        upstream_id: config.id.clone(),
                        target: target.url.clone(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let hash = HashPolicy::from_config(config, &targets);
        Ok(Self {
            id: config.id.clone(),
            client,
            smooth_weighted: SmoothWeightedState::new(targets.len()),
            targets,
            load_balancing: config.load_balancing,
            hash,
            cursor: AtomicUsize::new(0),
            random_state: AtomicU64::new(scheduling_seed()),
            permits: Arc::new(Semaphore::new(config.max_in_flight_requests)),
            max_in_flight_requests: config.max_in_flight_requests,
            retry: RetryPolicy::from_config(config),
            health: PassiveHealthPolicy {
                failure_threshold: config.passive_health.failure_threshold,
                ejection_time_ms: config.passive_health.ejection_time_ms,
                failure_statuses: config.passive_health.failure_statuses.clone(),
            },
            active_health: config.active_health.as_ref().map(ActiveHealthPolicy::from),
            epoch: Instant::now(),
        })
    }

    pub(in crate::data_plane) fn connection_capacity(&self) -> [u64; 3] {
        self.client.connection_capacity()
    }

    pub(in crate::data_plane) fn target_connection_capacity(&self) -> [u64; 3] {
        self.client.target_connection_capacity()
    }

    pub(in crate::data_plane) async fn run_active_health_check(
        &self,
        target_index: usize,
    ) -> ActiveHealthTransition {
        let Some(policy) = &self.active_health else {
            return ActiveHealthTransition::Unchanged;
        };
        let Some(target) = self.targets.get(target_index) else {
            return ActiveHealthTransition::Unchanged;
        };
        let Some(url) = target.active_health_url.clone() else {
            return ActiveHealthTransition::Unchanged;
        };
        let method = match policy.method {
            UpstreamActiveHealthMethod::Get => axum::http::Method::GET,
            UpstreamActiveHealthMethod::Head => axum::http::Method::HEAD,
        };
        let request = match Request::builder()
            .method(method)
            .uri(url.as_str())
            .body(Body::empty())
        {
            Ok(request) => request,
            Err(_) => return self.record_active_health(target_index, false),
        };
        let result = self
            .client
            .execute_with_timeout(request, policy.timeout)
            .await;
        let success = match result {
            Err(error) if error.is_connection_saturated() => {
                return ActiveHealthTransition::Unchanged
            }
            Ok(response)
                if response.status().as_u16() >= policy.success_status_min
                    && response.status().as_u16() <= policy.success_status_max =>
            {
                response_body_within_limit(response, policy.max_response_body_bytes).await
            }
            _ => false,
        };
        self.record_active_health(target_index, success)
    }
}

impl<T> ProxyUpstream<T> {
    pub(in crate::data_plane) fn id(&self) -> &str {
        &self.id
    }

    pub(in crate::data_plane) fn target_count(&self) -> usize {
        self.targets.len()
    }

    pub(in crate::data_plane) fn active_health_interval(&self) -> Option<Duration> {
        self.active_health.as_ref().map(|policy| policy.interval)
    }

    pub(in crate::data_plane) fn aggregate_target_health(&self) -> [u64; 4] {
        self.targets.iter().fold([0_u64; 4], |mut counts, target| {
            let state = if !target.active_available.load(Ordering::Acquire) {
                3
            } else if target.probe_in_flight.load(Ordering::Acquire) {
                2
            } else if target.ejected_until_ms.load(Ordering::Acquire) != 0 {
                1
            } else {
                0
            };
            counts[state] = counts[state].saturating_add(1);
            counts
        })
    }

    pub(in crate::data_plane) fn request_capacity(&self) -> [u64; 3] {
        capacity_snapshot(
            self.max_in_flight_requests,
            self.permits.available_permits(),
        )
    }

    pub(super) fn try_admit(&self) -> Result<OwnedSemaphorePermit, ()> {
        self.permits.clone().try_acquire_owned().map_err(|_| ())
    }

    #[cfg(test)]
    pub(super) fn select_target(&self) -> Option<SelectedTarget<'_>> {
        self.select_target_observed(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), "", None)
    }

    pub(super) fn select_target_observed(
        &self,
        client_ip: IpAddr,
        hash_key: &str,
        metrics: Option<&DataPlaneMetrics>,
    ) -> Option<SelectedTarget<'_>> {
        self.select_target_excluding_observed(
            &AttemptedTargets::default(),
            client_ip,
            hash_key,
            metrics,
        )
    }

    #[cfg(test)]
    pub(super) fn select_target_excluding(
        &self,
        attempted: &AttemptedTargets,
    ) -> Option<SelectedTarget<'_>> {
        self.select_target_excluding_observed(
            attempted,
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            "",
            None,
        )
    }

    pub(super) fn select_target_excluding_observed(
        &self,
        attempted: &AttemptedTargets,
        client_ip: IpAddr,
        hash_key: &str,
        metrics: Option<&DataPlaneMetrics>,
    ) -> Option<SelectedTarget<'_>> {
        let now_ms = self.now_ms();
        let use_backups = !self.targets.iter().enumerate().any(|(index, target)| {
            !attempted.contains(index) && !target.backup && target.is_eligible(now_ms)
        });
        match self.load_balancing {
            UpstreamLoadBalancingStrategy::RoundRobin => {
                self.select_smooth_weighted(attempted, now_ms, use_backups, metrics)
            }
            UpstreamLoadBalancingStrategy::LeastConnections => {
                self.select_weighted_least_connections(attempted, now_ms, use_backups)
            }
            UpstreamLoadBalancingStrategy::RandomTwoLeastConnections => {
                self.select_random_two_least_connections(attempted, now_ms, use_backups)
            }
            UpstreamLoadBalancingStrategy::IpHash => {
                self.select_ip_hash(attempted, now_ms, use_backups, client_ip, metrics)
            }
            UpstreamLoadBalancingStrategy::Hash => {
                self.select_hash(attempted, now_ms, use_backups, hash_key, metrics)
            }
        }
    }

    fn select_smooth_weighted(
        &self,
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
        metrics: Option<&DataPlaneMetrics>,
    ) -> Option<SelectedTarget<'_>> {
        let selection = self
            .smooth_weighted
            .select(&self.targets, attempted, now_ms, use_backups);
        if selection.contended {
            if let Some(metrics) = metrics {
                metrics.record_upstream_selection_contention();
            }
        }
        selection.target
    }

    fn select_ip_hash(
        &self,
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
        client_ip: IpAddr,
        metrics: Option<&DataPlaneMetrics>,
    ) -> Option<SelectedTarget<'_>> {
        let total_weight = self
            .targets
            .iter()
            .filter(|target| target.backup == use_backups)
            .fold(0_usize, |total, target| total.saturating_add(target.weight));
        if total_weight == 0 {
            return None;
        }

        let mut hash = 89_usize;
        for _ in 0..=20 {
            hash = advance_ip_hash(hash, client_ip);
            let mut ticket = hash % total_weight;
            let mut selected_index = None;
            for (index, target) in self.targets.iter().enumerate() {
                if target.backup != use_backups {
                    continue;
                }
                if ticket < target.weight {
                    selected_index = Some(index);
                    break;
                }
                ticket -= target.weight;
            }
            let Some(index) = selected_index else {
                break;
            };
            if attempted.contains(index) || !self.targets[index].is_eligible(now_ms) {
                continue;
            }
            if let Some(selected) = self.targets[index].try_select(index, now_ms) {
                return Some(selected);
            }
        }

        self.select_smooth_weighted(attempted, now_ms, use_backups, metrics)
    }

    fn select_hash(
        &self,
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
        hash_key: &str,
        metrics: Option<&DataPlaneMetrics>,
    ) -> Option<SelectedTarget<'_>> {
        if hash_key.is_empty() {
            return self.select_smooth_weighted(attempted, now_ms, use_backups, metrics);
        }
        let Some(policy) = self.hash.as_ref() else {
            return self.select_smooth_weighted(attempted, now_ms, use_backups, metrics);
        };
        if policy.consistent {
            return self.select_consistent_hash(attempted, now_ms, use_backups, hash_key, metrics);
        }

        let total_weight = self
            .targets
            .iter()
            .filter(|target| target.backup == use_backups)
            .fold(0_usize, |total, target| total.saturating_add(target.weight));
        if total_weight == 0 {
            return None;
        }

        let key_bytes = hash_key.as_bytes();
        let mut accumulated = 0_u32;
        let mut rehash = 0_usize;
        let mut tries = 0_usize;
        loop {
            accumulated = accumulated.wrapping_add(hash_lb::nginx_hash_step(key_bytes, rehash));
            rehash = rehash.saturating_add(1);
            let mut ticket = (accumulated as usize) % total_weight;
            let mut selected_index = None;
            for (index, target) in self.targets.iter().enumerate() {
                if target.backup != use_backups {
                    continue;
                }
                if ticket < target.weight {
                    selected_index = Some(index);
                    break;
                }
                ticket -= target.weight;
            }
            let Some(index) = selected_index else {
                break;
            };
            if !attempted.contains(index) && self.targets[index].is_eligible(now_ms) {
                if let Some(selected) = self.targets[index].try_select(index, now_ms) {
                    return Some(selected);
                }
            }
            tries = tries.saturating_add(1);
            if tries > 20 {
                break;
            }
        }
        self.select_smooth_weighted(attempted, now_ms, use_backups, metrics)
    }

    fn select_consistent_hash(
        &self,
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
        hash_key: &str,
        metrics: Option<&DataPlaneMetrics>,
    ) -> Option<SelectedTarget<'_>> {
        let Some(policy) = self.hash.as_ref() else {
            return self.select_smooth_weighted(attempted, now_ms, use_backups, metrics);
        };
        if policy.points.is_empty() {
            return self.select_smooth_weighted(attempted, now_ms, use_backups, metrics);
        }
        let key_hash = hash_lb::NGX_CRC32.checksum(hash_key.as_bytes());
        let mut point_index = hash_lb::find_consistent_hash_point(&policy.points, key_hash);
        let mut tries = 0_usize;
        while tries <= 20 {
            let server_index = policy.points[point_index % policy.points.len()].target_index;
            if self.targets.get(server_index).is_some_and(|target| {
                target.backup == use_backups
                    && !attempted.contains(server_index)
                    && target.is_eligible(now_ms)
            }) {
                if let Some(selected) = self.targets[server_index].try_select(server_index, now_ms)
                {
                    return Some(selected);
                }
            }
            point_index = point_index.saturating_add(1);
            tries = tries.saturating_add(1);
        }
        self.select_smooth_weighted(attempted, now_ms, use_backups, metrics)
    }

    pub(super) fn select_random_two_least_connections(
        &self,
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
    ) -> Option<SelectedTarget<'_>> {
        let (eligible_count, total_weight) = self.targets.iter().enumerate().fold(
            (0_usize, 0_usize),
            |(count, total), (index, target)| {
                if attempted.contains(index)
                    || target.backup != use_backups
                    || !target.is_eligible(now_ms)
                {
                    (count, total)
                } else {
                    (
                        count.saturating_add(1),
                        total.saturating_add(target.effective_weight(now_ms)),
                    )
                }
            },
        );
        if eligible_count == 0 || total_weight == 0 {
            return None;
        }

        let first =
            self.weighted_random_candidate(attempted, now_ms, use_backups, None, total_weight);
        let Some((first_index, first_weight)) = first else {
            return self.select_weighted_least_connections(attempted, now_ms, use_backups);
        };
        if eligible_count == 1 {
            return self.targets[first_index]
                .try_select(first_index, now_ms)
                .or_else(|| {
                    self.select_weighted_least_connections(attempted, now_ms, use_backups)
                });
        }

        let remaining_weight = total_weight.saturating_sub(first_weight);
        if remaining_weight == 0 {
            return self.select_weighted_least_connections(attempted, now_ms, use_backups);
        }
        let Some((second_index, second_weight)) = self.weighted_random_candidate(
            attempted,
            now_ms,
            use_backups,
            Some(first_index),
            remaining_weight,
        ) else {
            return self.select_weighted_least_connections(attempted, now_ms, use_backups);
        };

        let first_active = self.targets[first_index]
            .active_requests
            .load(Ordering::Acquire);
        let second_active = self.targets[second_index]
            .active_requests
            .load(Ordering::Acquire);
        let (winner, alternative) =
            if weighted_load_cmp(first_active, first_weight, second_active, second_weight)
                == std::cmp::Ordering::Greater
            {
                (second_index, first_index)
            } else {
                (first_index, second_index)
            };
        self.targets[winner]
            .try_select(winner, now_ms)
            .or_else(|| self.targets[alternative].try_select(alternative, now_ms))
            .or_else(|| self.select_weighted_least_connections(attempted, now_ms, use_backups))
    }

    fn weighted_random_candidate(
        &self,
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
        excluded_index: Option<usize>,
        total_weight: usize,
    ) -> Option<(usize, usize)> {
        let mut ticket = self.random_below(total_weight);
        for (index, target) in self.targets.iter().enumerate() {
            if excluded_index == Some(index)
                || attempted.contains(index)
                || target.backup != use_backups
                || !target.is_eligible(now_ms)
            {
                continue;
            }
            let weight = target.effective_weight(now_ms);
            if ticket < weight {
                return Some((index, weight));
            }
            ticket -= weight;
        }
        None
    }

    pub(super) fn random_below(&self, upper_bound: usize) -> usize {
        debug_assert!(upper_bound != 0);
        let value = splitmix64(
            self.random_state
                .fetch_add(SPLITMIX64_GAMMA, Ordering::Relaxed),
        );
        ((u128::from(value) * upper_bound as u128) >> 64) as usize
    }

    fn select_weighted_least_connections(
        &self,
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
    ) -> Option<SelectedTarget<'_>> {
        let mut minimum: Option<(usize, usize)> = None;
        let mut tied_weight = 0usize;
        for (index, target) in self.targets.iter().enumerate() {
            if attempted.contains(index)
                || target.backup != use_backups
                || !target.is_eligible(now_ms)
            {
                continue;
            }
            let active = target.active_requests.load(Ordering::Acquire);
            let effective_weight = target.effective_weight(now_ms);
            match minimum {
                None => {
                    minimum = Some((active, effective_weight));
                    tied_weight = effective_weight;
                }
                Some((minimum_active, minimum_weight)) => {
                    match weighted_load_cmp(
                        active,
                        effective_weight,
                        minimum_active,
                        minimum_weight,
                    ) {
                        std::cmp::Ordering::Less => {
                            minimum = Some((active, effective_weight));
                            tied_weight = effective_weight;
                        }
                        std::cmp::Ordering::Equal => {
                            tied_weight = tied_weight.saturating_add(effective_weight);
                        }
                        std::cmp::Ordering::Greater => {}
                    }
                }
            }
        }
        let (minimum_active, minimum_weight) = minimum?;
        let ticket = self.cursor.fetch_add(1, Ordering::Relaxed) % tied_weight;
        let mut remaining = ticket;
        for (index, target) in self.targets.iter().enumerate() {
            if attempted.contains(index)
                || target.backup != use_backups
                || !target.is_eligible(now_ms)
            {
                continue;
            }
            let active = target.active_requests.load(Ordering::Acquire);
            let effective_weight = target.effective_weight(now_ms);
            if weighted_load_cmp(active, effective_weight, minimum_active, minimum_weight)
                != std::cmp::Ordering::Equal
            {
                continue;
            }
            if remaining < effective_weight {
                if let Some(selected) = target.try_select(index, now_ms) {
                    return Some(selected);
                }
                break;
            }
            remaining -= effective_weight;
        }

        self.targets
            .iter()
            .enumerate()
            .filter(|(index, target)| {
                !attempted.contains(*index)
                    && target.backup == use_backups
                    && target.is_eligible(now_ms)
            })
            .min_by(|(_, left), (_, right)| {
                weighted_load_cmp(
                    left.active_requests.load(Ordering::Acquire),
                    left.effective_weight(now_ms),
                    right.active_requests.load(Ordering::Acquire),
                    right.effective_weight(now_ms),
                )
            })
            .and_then(|(index, target)| target.try_select(index, now_ms))
    }

    pub(super) fn claim_target_activity(&self, target_index: usize) -> TargetActivityLease {
        TargetActivityLease::claim(&self.targets[target_index].active_requests)
    }

    pub(super) fn next_retry_target(
        &self,
        attempted: &mut AttemptedTargets,
        context: RetryTargetContext<'_>,
    ) -> Option<SelectedTarget<'_>> {
        if context.attempts_started >= context.maximum_attempts
            || Instant::now() >= context.deadline
        {
            return None;
        }
        let selected = self.select_target_excluding_observed(
            attempted,
            context.client_ip,
            context.hash_key,
            Some(context.metrics),
        )?;
        attempted.insert(selected.index);
        context.metrics.record_upstream_retry(context.reason);
        Some(selected)
    }

    pub(super) fn status_is_failure(&self, status: StatusCode) -> bool {
        self.health.failure_statuses.contains(&status.as_u16())
    }

    pub(super) fn record_success(&self, selected: SelectedTarget<'_>) {
        let target = &self.targets[selected.index];
        let recovered = if selected.ejection_deadline_ms == 0 {
            false
        } else {
            let mut smooth = self.smooth_weighted.lock_target(selected.index);
            let recovered = target
                .ejected_until_ms
                .compare_exchange(
                    selected.ejection_deadline_ms,
                    0,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok();
            if recovered {
                target.consecutive_failures.store(0, Ordering::Release);
                if target.active_available.load(Ordering::Acquire) {
                    target.start_slow_start(self.now_ms());
                }
                smooth.reset(target.slow_start_marker());
            }
            recovered
        };
        if recovered
            || (selected.ejection_deadline_ms == 0
                && target.ejected_until_ms.load(Ordering::Acquire) == 0)
        {
            target.consecutive_failures.store(0, Ordering::Release);
        }
        if selected.probe {
            target.probe_in_flight.store(false, Ordering::Release);
        }
    }

    pub(super) fn record_failure(&self, selected: SelectedTarget<'_>) {
        let target = &self.targets[selected.index];
        if target.ejected_until_ms.load(Ordering::Acquire) != selected.ejection_deadline_ms {
            if selected.probe {
                target.probe_in_flight.store(false, Ordering::Release);
            }
            return;
        }
        let failures = if selected.probe {
            self.health.failure_threshold
        } else {
            target
                .consecutive_failures
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    Some(current.saturating_add(1))
                })
                .unwrap_or(u32::MAX)
                .saturating_add(1)
        };
        if failures >= self.health.failure_threshold {
            let next_deadline = self.now_ms().saturating_add(self.health.ejection_time_ms);
            let mut smooth = self.smooth_weighted.lock_target(selected.index);
            if target
                .ejected_until_ms
                .compare_exchange(
                    selected.ejection_deadline_ms,
                    next_deadline,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                target.consecutive_failures.store(0, Ordering::Release);
                target.slow_start_started_ms.store(0, Ordering::Release);
                smooth.reset(0);
            }
        }
        if selected.probe {
            target.probe_in_flight.store(false, Ordering::Release);
        }
    }

    pub(super) fn abandon_probe(&self, selected: SelectedTarget<'_>) {
        if selected.probe {
            self.targets[selected.index]
                .probe_in_flight
                .store(false, Ordering::Release);
        }
    }

    /// Health-aware stream endpoint pick (shared with HTTP reverse proxy).
    pub(crate) fn select_stream_endpoint(&self, client_ip: IpAddr) -> Option<StreamEndpoint> {
        let selected = self.select_target_observed(client_ip, "", None)?;
        let port = selected.url.port_or_known_default()?;
        let host = selected.url.host_str()?.to_owned();
        Some(StreamEndpoint {
            host,
            port,
            index: selected.index,
            probe: selected.probe,
            ejection_deadline_ms: selected.ejection_deadline_ms,
        })
    }

    pub(crate) fn record_stream_success(&self, endpoint: &StreamEndpoint) {
        self.record_success(SelectedTarget {
            index: endpoint.index,
            url: &self.targets[endpoint.index].url,
            probe: endpoint.probe,
            ejection_deadline_ms: endpoint.ejection_deadline_ms,
        });
    }

    pub(crate) fn record_stream_failure(&self, endpoint: &StreamEndpoint) {
        self.record_failure(SelectedTarget {
            index: endpoint.index,
            url: &self.targets[endpoint.index].url,
            probe: endpoint.probe,
            ejection_deadline_ms: endpoint.ejection_deadline_ms,
        });
    }

    pub(super) fn probe_claim_lease(
        &self,
        target_index: usize,
        claimed: bool,
    ) -> ProbeClaimLease<'_> {
        ProbeClaimLease {
            flag: claimed.then(|| &self.targets[target_index].probe_in_flight),
        }
    }

    pub(super) fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    pub(super) fn record_active_health(
        &self,
        target_index: usize,
        success: bool,
    ) -> ActiveHealthTransition {
        let Some(policy) = &self.active_health else {
            return ActiveHealthTransition::Unchanged;
        };
        let target = &self.targets[target_index];
        if success {
            target.active_failures.store(0, Ordering::Release);
            if target.active_available.load(Ordering::Acquire) {
                target.active_successes.store(0, Ordering::Release);
                return ActiveHealthTransition::Unchanged;
            }
            let successes = saturating_increment(&target.active_successes);
            if successes >= policy.healthy_threshold {
                let mut smooth = self.smooth_weighted.lock_target(target_index);
                if target
                    .active_available
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    target.active_successes.store(0, Ordering::Release);
                    if target.ejected_until_ms.load(Ordering::Acquire) == 0 {
                        target.start_slow_start(self.now_ms());
                        smooth.reset(target.slow_start_marker());
                    } else {
                        smooth.reset(0);
                    }
                    return ActiveHealthTransition::BecameHealthy;
                }
            }
            return ActiveHealthTransition::Unchanged;
        }

        target.active_successes.store(0, Ordering::Release);
        if !target.active_available.load(Ordering::Acquire) {
            target.active_failures.store(0, Ordering::Release);
            return ActiveHealthTransition::Unchanged;
        }
        let failures = saturating_increment(&target.active_failures);
        if failures >= policy.unhealthy_threshold {
            let mut smooth = self.smooth_weighted.lock_target(target_index);
            if target
                .active_available
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                target.active_failures.store(0, Ordering::Release);
                target.slow_start_started_ms.store(0, Ordering::Release);
                smooth.reset(0);
                return ActiveHealthTransition::BecameUnhealthy;
            }
        }
        ActiveHealthTransition::Unchanged
    }
}

pub(super) fn weighted_load_cmp(
    left_active: usize,
    left_weight: usize,
    right_active: usize,
    right_weight: usize,
) -> std::cmp::Ordering {
    (left_active as u128 * right_weight as u128).cmp(&(right_active as u128 * left_weight as u128))
}

pub(super) fn advance_ip_hash(hash: usize, client_ip: IpAddr) -> usize {
    match client_ip {
        IpAddr::V4(address) => address.octets()[..3]
            .iter()
            .fold(hash, |hash, byte| (hash * 113 + usize::from(*byte)) % 6271),
        IpAddr::V6(address) => address
            .octets()
            .iter()
            .fold(hash, |hash, byte| (hash * 113 + usize::from(*byte)) % 6271),
    }
}

fn scheduling_seed() -> u64 {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    time ^ u64::from(std::process::id()).rotate_left(32)
        ^ RANDOM_SEED_SEQUENCE.fetch_add(SPLITMIX64_GAMMA, Ordering::Relaxed)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(SPLITMIX64_GAMMA);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

impl ProxyTarget {
    pub(super) fn start_slow_start(&self, now_ms: u64) {
        if self.slow_start_duration_ms != 0 {
            self.slow_start_started_ms
                .store(now_ms.max(1), Ordering::Release);
        }
    }

    pub(in crate::data_plane) fn effective_weight(&self, now_ms: u64) -> usize {
        if self.slow_start_duration_ms == 0 {
            return self.weight;
        }
        let started_ms = self.slow_start_started_ms.load(Ordering::Acquire);
        if started_ms == 0 {
            return self.weight;
        }
        let elapsed_ms = now_ms.saturating_sub(started_ms);
        if elapsed_ms >= self.slow_start_duration_ms {
            return match self.slow_start_started_ms.compare_exchange(
                started_ms,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => self.weight,
                Err(current_started_ms) => self.ramped_weight(now_ms, current_started_ms),
            };
        }
        self.ramped_weight(now_ms, started_ms)
    }

    pub(super) fn ramped_weight(&self, now_ms: u64, started_ms: u64) -> usize {
        if started_ms == 0 {
            return self.weight;
        }
        let elapsed_ms = now_ms.saturating_sub(started_ms);
        if elapsed_ms >= self.slow_start_duration_ms {
            return self.weight;
        }
        let scaled = (elapsed_ms as u128 * self.weight as u128
            / self.slow_start_duration_ms as u128) as usize;
        scaled.clamp(1, self.weight)
    }

    pub(in crate::data_plane) fn slow_start_marker(&self) -> u64 {
        self.slow_start_started_ms.load(Ordering::Acquire)
    }

    pub(in crate::data_plane) fn is_eligible(&self, now_ms: u64) -> bool {
        if !self.active_available.load(Ordering::Acquire) {
            return false;
        }
        let ejected_until = self.ejected_until_ms.load(Ordering::Acquire);
        ejected_until == 0
            || (ejected_until <= now_ms && !self.probe_in_flight.load(Ordering::Acquire))
    }

    pub(in crate::data_plane) fn try_select(
        &self,
        index: usize,
        now_ms: u64,
    ) -> Option<SelectedTarget<'_>> {
        if !self.active_available.load(Ordering::Acquire) {
            return None;
        }
        let ejected_until = self.ejected_until_ms.load(Ordering::Acquire);
        if ejected_until == 0 {
            return Some(SelectedTarget {
                index,
                url: &self.url,
                probe: false,
                ejection_deadline_ms: 0,
            });
        }
        if ejected_until <= now_ms
            && self
                .probe_in_flight
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            return Some(SelectedTarget {
                index,
                url: &self.url,
                probe: true,
                ejection_deadline_ms: ejected_until,
            });
        }
        None
    }
}

impl From<&UpstreamActiveHealthConfig> for ActiveHealthPolicy {
    fn from(config: &UpstreamActiveHealthConfig) -> Self {
        Self {
            method: config.method,
            interval: Duration::from_millis(config.interval_ms),
            timeout: Duration::from_millis(config.timeout_ms),
            unhealthy_threshold: config.unhealthy_threshold,
            healthy_threshold: config.healthy_threshold,
            success_status_min: config.success_status_min,
            success_status_max: config.success_status_max,
            max_response_body_bytes: config.max_response_body_bytes,
        }
    }
}

fn active_health_url(target: &Url, uri: &str) -> Url {
    target
        .join(uri)
        .expect("semantic validation guarantees an origin-form active health URI")
}

async fn response_body_within_limit(
    response: Response<UpstreamResponseBody>,
    maximum_bytes: u64,
) -> bool {
    if response.headers().get(CONTENT_LENGTH).is_some_and(|value| {
        value
            .to_str()
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|length| length > maximum_bytes)
    }) {
        return false;
    }
    let mut body = response.into_body();
    let mut observed = 0_u64;
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                observed =
                    observed.saturating_add(frame.data_ref().map_or(0, |data| data.len() as u64));
                if observed > maximum_bytes {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    true
}

fn saturating_increment(counter: &AtomicU32) -> u32 {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            Some(current.saturating_add(1))
        })
        .unwrap_or(u32::MAX)
        .saturating_add(1)
}

fn capacity_snapshot(configured: usize, available: usize) -> [u64; 3] {
    let configured = configured as u64;
    let available = available.min(configured as usize) as u64;
    [configured, configured.saturating_sub(available), available]
}
