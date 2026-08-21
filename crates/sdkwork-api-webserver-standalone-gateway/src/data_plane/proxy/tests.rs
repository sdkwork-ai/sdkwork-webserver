use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Method, Request, Version},
};
use tokio::sync::Semaphore;

use super::{
    advance_ip_hash, apply_proxy_set_headers, build_target_url, classify_upgrade_request,
    expand_proxy_header_value, is_bodyless_idempotent_request, ActiveHealthPolicy,
    ActiveHealthTransition, AtomicBool, AtomicU32, AtomicU64, AttemptedTargets, IpAddr,
    PassiveHealthPolicy, ProxyTarget, ProxyUpstream, RetryPolicy, SmoothWeightedState,
    TargetActivityLease, UpgradeDisposition, UpstreamActiveHealthMethod,
    UpstreamLoadBalancingStrategy, Url,
};

#[test]
fn classic_websocket_upgrade_requires_http11_get_and_exact_tokens() {
    let valid = Request::builder()
        .method(Method::GET)
        .version(Version::HTTP_11)
        .header("connection", "keep-alive, Upgrade")
        .header("upgrade", "WebSocket")
        .body(Body::empty())
        .expect("valid request");
    assert_eq!(
        classify_upgrade_request(&valid),
        Ok(UpgradeDisposition::WebSocket)
    );

    for request in [
        Request::builder()
            .method(Method::GET)
            .version(Version::HTTP_2)
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .body(Body::empty())
            .expect("HTTP/2 request"),
        Request::builder()
            .method(Method::POST)
            .version(Version::HTTP_11)
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .body(Body::empty())
            .expect("POST request"),
        Request::builder()
            .method(Method::GET)
            .version(Version::HTTP_11)
            .header("connection", "x-upgrade")
            .header("upgrade", "websocket")
            .body(Body::empty())
            .expect("substring token request"),
        Request::builder()
            .method(Method::GET)
            .version(Version::HTTP_11)
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("content-length", "0")
            .body(Body::empty())
            .expect("Body-framed request"),
    ] {
        assert_eq!(classify_upgrade_request(&request), Err(()));
    }
}

#[test]
fn other_well_formed_upgrade_protocols_remain_explicitly_unsupported() {
    let request = Request::builder()
        .method(Method::GET)
        .version(Version::HTTP_11)
        .header("connection", "upgrade")
        .header("upgrade", "h2c")
        .body(Body::empty())
        .expect("unsupported upgrade request");
    assert_eq!(
        classify_upgrade_request(&request),
        Ok(UpgradeDisposition::Unsupported)
    );
}

fn test_upstream(target_count: usize, threshold: u32, ejection_time_ms: u64) -> ProxyUpstream<()> {
    test_weighted_upstream(&vec![1; target_count], threshold, ejection_time_ms)
}

fn test_weighted_upstream(
    weights: &[u16],
    threshold: u32,
    ejection_time_ms: u64,
) -> ProxyUpstream<()> {
    ProxyUpstream {
        id: "test-upstream".to_owned(),
        client: (),
        targets: weights
            .iter()
            .enumerate()
            .map(|(index, weight)| ProxyTarget {
                url: Url::parse(&format!("http://127.0.0.1:{}/", 10_000 + index))
                    .expect("valid target URL"),
                weight: usize::from(*weight),
                backup: false,
                slow_start_duration_ms: 0,
                slow_start_started_ms: AtomicU64::new(0),
                active_requests: Arc::new(AtomicUsize::new(0)),
                consecutive_failures: AtomicU32::new(0),
                ejected_until_ms: AtomicU64::new(0),
                probe_in_flight: AtomicBool::new(false),
                active_available: AtomicBool::new(true),
                active_failures: AtomicU32::new(0),
                active_successes: AtomicU32::new(0),
                active_health_url: None,
            })
            .collect(),
        load_balancing: UpstreamLoadBalancingStrategy::RoundRobin,
        hash: None,
        smooth_weighted: SmoothWeightedState::new(weights.len()),
        cursor: AtomicUsize::new(0),
        random_state: AtomicU64::new(0),
        permits: Arc::new(Semaphore::new(1)),
        max_in_flight_requests: 1,
        retry: RetryPolicy {
            enabled: false,
            maximum_attempts: 1,
            total_timeout: Duration::from_secs(1),
            attempt_timeout: Duration::from_secs(1),
            transport_failure: false,
            timeout: false,
            statuses: [false; 3],
        },
        health: PassiveHealthPolicy {
            failure_threshold: threshold,
            ejection_time_ms,
            failure_statuses: vec![503],
        },
        active_health: None,
        epoch: Instant::now(),
    }
}

#[test]
fn rewritten_target_encodes_canonical_reserved_and_unicode_path_bytes() {
    let target = Url::parse("https://origin.example/base").expect("valid target");
    let rewritten = build_target_url(
        &target,
        true,
        "/rewrite",
        "/rewrite/a%3Fb%23c%25d/%E4%B8%AD",
        "/rewrite/a?b#c%d/中",
        Some("x=%2F&y=1"),
    )
    .expect("rewritten URL");

    assert_eq!(
        rewritten.as_str(),
        "https://origin.example/base/a%3Fb%23c%25d/%E4%B8%AD?x=%2F&y=1"
    );
    assert_eq!(rewritten.path(), "/base/a%3Fb%23c%25d/%E4%B8%AD");
    assert_eq!(rewritten.query(), Some("x=%2F&y=1"));
    assert_eq!(rewritten.fragment(), None);
}

#[test]
fn no_rewrite_target_preserves_raw_path_and_query_encoding() {
    let target = Url::parse("https://origin.example/base").expect("valid target");
    let rewritten = build_target_url(
        &target,
        false,
        "/rewrite",
        "/rewrite/a%2Fb/%E4%B8%AD",
        "/rewrite/a/b/中",
        Some("x=%2F&y=1"),
    )
    .expect("raw URL");

    assert_eq!(
        rewritten.as_str(),
        "https://origin.example/base/rewrite/a%2Fb/%E4%B8%AD?x=%2F&y=1"
    );
}

#[tokio::test]
async fn admission_never_queues_and_releases_on_drop() {
    let upstream = test_upstream(1, 1, 10);
    assert_eq!(upstream.request_capacity(), [1, 0, 1]);
    let permit = upstream.try_admit().expect("first request is admitted");
    assert_eq!(upstream.request_capacity(), [1, 1, 0]);
    assert!(upstream.try_admit().is_err());
    drop(permit);
    assert_eq!(upstream.request_capacity(), [1, 0, 1]);
    assert!(upstream.try_admit().is_ok());
}

#[tokio::test]
async fn cancelled_attempt_releases_half_open_probe_claim() {
    let upstream = test_upstream(1, 1, 10);
    upstream.targets[0]
        .probe_in_flight
        .store(true, Ordering::Release);

    let pending_attempt = async {
        let _lease = upstream.probe_claim_lease(0, true);
        std::future::pending::<()>().await;
    };
    let mut pending_attempt = Box::pin(pending_attempt);
    assert!(
        tokio::time::timeout(Duration::from_millis(1), pending_attempt.as_mut())
            .await
            .is_err()
    );
    assert!(upstream.targets[0].probe_in_flight.load(Ordering::Acquire));

    drop(pending_attempt);
    assert!(!upstream.targets[0].probe_in_flight.load(Ordering::Acquire));
}

#[test]
fn retry_selection_never_reuses_a_target_and_request_replay_is_fail_closed() {
    let upstream = test_upstream(3, 1, 10);
    let mut attempted = AttemptedTargets::default();
    let first = upstream.select_target().expect("first target");
    attempted.insert(first.index);
    let second = upstream
        .select_target_excluding(&attempted)
        .expect("different second target");
    assert_ne!(first.index, second.index);
    attempted.insert(second.index);
    let third = upstream
        .select_target_excluding(&attempted)
        .expect("different third target");
    assert_ne!(first.index, third.index);
    assert_ne!(second.index, third.index);

    let empty_get = Request::builder()
        .method(Method::GET)
        .body(Body::empty())
        .expect("empty GET");
    assert!(is_bodyless_idempotent_request(&empty_get));
    let post = Request::builder()
        .method(Method::POST)
        .body(Body::empty())
        .expect("POST");
    assert!(!is_bodyless_idempotent_request(&post));
    let body_get = Request::builder()
        .method(Method::GET)
        .body(Body::from("payload"))
        .expect("body GET");
    assert!(!is_bodyless_idempotent_request(&body_get));
}

#[tokio::test]
async fn backup_tier_waits_for_primary_exhaustion_and_yields_to_half_open_probe() {
    let mut upstream = test_upstream(3, 1, 10);
    upstream.targets[2].backup = true;
    let mut attempted = AttemptedTargets::default();

    let first = upstream.select_target().expect("first primary target");
    assert_eq!(first.index, 0);
    attempted.insert(first.index);
    let second = upstream
        .select_target_excluding(&attempted)
        .expect("second primary precedes backup");
    assert_eq!(second.index, 1);
    attempted.insert(second.index);
    let backup = upstream
        .select_target_excluding(&attempted)
        .expect("backup follows exhausted primaries");
    assert_eq!(backup.index, 2);

    let mut health_upstream = test_upstream(2, 1, 10);
    health_upstream.targets[1].backup = true;
    let primary = health_upstream.select_target().expect("healthy primary");
    assert_eq!(primary.index, 0);
    health_upstream.record_failure(primary);
    let backup = health_upstream
        .select_target()
        .expect("backup serves during primary ejection");
    assert_eq!(backup.index, 1);
    health_upstream.record_success(backup);

    tokio::time::sleep(Duration::from_millis(15)).await;
    let probe = health_upstream
        .select_target()
        .expect("expired primary probe precedes backup");
    assert_eq!(probe.index, 0);
    assert!(probe.probe);
    health_upstream.record_success(probe);
    assert_eq!(
        health_upstream
            .select_target()
            .expect("recovered primary resumes traffic")
            .index,
        0
    );
}

#[test]
fn weighted_selection_honors_relative_slots_and_equal_weight_round_robin() {
    let weighted = test_weighted_upstream(&[3, 1], 3, 1_000);
    let selected = (0..8)
        .map(|_| weighted.select_target().expect("weighted target").index)
        .collect::<Vec<_>>();
    assert_eq!(selected, [0, 0, 1, 0, 0, 0, 1, 0]);

    let equal = test_weighted_upstream(&[1, 1, 1], 3, 1_000);
    let selected = (0..6)
        .map(|_| equal.select_target().expect("equal target").index)
        .collect::<Vec<_>>();
    assert_eq!(selected, [0, 1, 2, 0, 1, 2]);

    let three_way = test_weighted_upstream(&[5, 1, 1], 3, 1_000);
    let selected = (0..7)
        .map(|_| three_way.select_target().expect("smooth target").index)
        .collect::<Vec<_>>();
    assert_eq!(selected, [0, 0, 1, 0, 2, 0, 0]);
}

#[test]
fn attempted_exclusion_retains_the_global_smooth_phase() {
    let upstream = test_weighted_upstream(&[3, 1], 3, 1_000);
    assert_eq!(upstream.select_target().expect("first target").index, 0);
    assert_eq!(upstream.select_target().expect("second target").index, 0);

    let mut attempted = AttemptedTargets::default();
    attempted.insert(0);
    assert_eq!(
        upstream
            .select_target_excluding(&attempted)
            .expect("retry target")
            .index,
        1
    );
    assert_eq!(
        upstream
            .select_target()
            .expect("retained phase target")
            .index,
        1,
        "request-local exclusion must not reset the skipped target's current weight"
    );
    assert_eq!(
        upstream
            .select_target()
            .expect("completed smooth cycle target")
            .index,
        0
    );
}

#[test]
fn primary_and_backup_tiers_retain_independent_smooth_phases() {
    let mut upstream = test_weighted_upstream(&[3, 1, 2, 1], 3, 1_000);
    upstream.targets[2].backup = true;
    upstream.targets[3].backup = true;

    assert_eq!(
        upstream
            .select_target()
            .expect("initial primary target")
            .index,
        0
    );
    let mut primaries_attempted = AttemptedTargets::default();
    primaries_attempted.insert(0);
    primaries_attempted.insert(1);
    assert_eq!(
        upstream
            .select_target_excluding(&primaries_attempted)
            .expect("initial backup target")
            .index,
        2
    );

    assert_eq!(
        upstream
            .select_target()
            .expect("retained primary phase")
            .index,
        0
    );
    assert_eq!(
        upstream
            .select_target()
            .expect("next retained primary phase")
            .index,
        1
    );
    assert_eq!(
        upstream
            .select_target_excluding(&primaries_attempted)
            .expect("retained backup phase")
            .index,
        3,
        "primary selections must not advance or reset the backup tier"
    );
}

#[tokio::test]
async fn passive_recovery_resets_the_target_smooth_phase() {
    let upstream = test_weighted_upstream(&[3, 1], 1, 20);
    let failed = upstream.select_target().expect("initial weighted target");
    assert_eq!(failed.index, 0);
    upstream.record_failure(failed);

    let survivor = upstream
        .select_target()
        .expect("healthy target during ejection");
    assert_eq!(survivor.index, 1);
    upstream.record_success(survivor);

    tokio::time::sleep(Duration::from_millis(25)).await;
    let probe = upstream.select_target().expect("half-open recovery probe");
    assert_eq!(probe.index, 0);
    assert!(probe.probe);
    upstream.record_success(probe);

    assert_eq!(
        upstream
            .select_target()
            .expect("reset recovery phase target")
            .index,
        0
    );
    assert_eq!(
        upstream
            .select_target()
            .expect("smooth post-recovery target")
            .index,
        1
    );
}

#[test]
fn active_recovery_resets_phase_and_uses_slow_start_weight() {
    let mut upstream = test_weighted_upstream(&[4, 1], 1, 1_000);
    upstream.targets[0].slow_start_duration_ms = 1_000;
    upstream.active_health = Some(ActiveHealthPolicy {
        method: UpstreamActiveHealthMethod::Get,
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        unhealthy_threshold: 1,
        healthy_threshold: 1,
        success_status_min: 200,
        success_status_max: 399,
        max_response_body_bytes: 64,
    });

    assert_eq!(upstream.select_target().expect("nominal target").index, 0);
    assert_eq!(
        upstream.record_active_health(0, false),
        ActiveHealthTransition::BecameUnhealthy
    );
    assert_eq!(
        upstream
            .select_target()
            .expect("survivor while target is unavailable")
            .index,
        1
    );
    assert_eq!(
        upstream.record_active_health(0, true),
        ActiveHealthTransition::BecameHealthy
    );
    assert_eq!(upstream.targets[0].effective_weight(upstream.now_ms()), 1);
    assert_eq!(
        upstream
            .select_target()
            .expect("first slow-start phase target")
            .index,
        1,
        "the recovered target must re-enter with effective weight one"
    );
    assert_eq!(
        upstream
            .select_target()
            .expect("next slow-start phase target")
            .index,
        0
    );
}

#[test]
fn concurrent_smooth_selection_preserves_exact_weighted_totals_without_deadlock() {
    let upstream = Arc::new(test_weighted_upstream(&[3, 1], 3, 1_000));
    let counts = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
    let start = Arc::new(std::sync::Barrier::new(8));
    let threads = (0..8)
        .map(|_| {
            let upstream = upstream.clone();
            let counts = counts.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                for _ in 0..10_000 {
                    let selected = upstream.select_target().expect("concurrent target");
                    counts[selected.index].fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().expect("smooth selector thread joins");
    }
    assert_eq!(counts[0].load(Ordering::Acquire), 60_000);
    assert_eq!(counts[1].load(Ordering::Acquire), 20_000);
}

#[test]
fn weighted_least_connections_uses_active_request_ratios_and_tier_health() {
    let mut upstream = test_weighted_upstream(&[2, 1, 100], 1, 1_000);
    upstream.load_balancing = UpstreamLoadBalancingStrategy::LeastConnections;
    upstream.targets[2].backup = true;

    upstream.targets[0]
        .active_requests
        .store(1, Ordering::Release);
    upstream.targets[1]
        .active_requests
        .store(1, Ordering::Release);
    let selected = upstream
        .select_target()
        .expect("weighted lower ratio target is selected");
    assert_eq!(selected.index, 0, "1/2 is less loaded than 1/1");

    upstream.targets[0]
        .active_requests
        .store(2, Ordering::Release);
    upstream.targets[1]
        .active_requests
        .store(1, Ordering::Release);
    upstream.cursor.store(0, Ordering::Release);
    let tied = (0..6)
        .map(|_| {
            upstream
                .select_target()
                .expect("equal weighted load remains selectable")
                .index
        })
        .collect::<Vec<_>>();
    assert_eq!(tied, [0, 0, 1, 0, 0, 1]);
    assert!(tied.iter().all(|index| *index != 2));

    upstream.targets[0]
        .active_available
        .store(false, Ordering::Release);
    upstream.targets[1]
        .active_available
        .store(false, Ordering::Release);
    assert_eq!(
        upstream
            .select_target()
            .expect("backup becomes eligible after primary health exclusion")
            .index,
        2
    );
}

#[test]
fn random_two_selects_the_lower_weighted_load_from_distinct_candidates() {
    let mut upstream = test_weighted_upstream(&[10, 1], 1, 1_000);
    upstream.load_balancing = UpstreamLoadBalancingStrategy::RandomTwoLeastConnections;
    upstream.targets[0]
        .active_requests
        .store(1, Ordering::Release);
    upstream.targets[1]
        .active_requests
        .store(1, Ordering::Release);
    for _ in 0..32 {
        assert_eq!(
            upstream
                .select_target()
                .expect("two weighted candidates")
                .index,
            0,
            "1/10 is less loaded than 1/1 regardless of weighted sample order"
        );
    }

    upstream.targets[0]
        .active_requests
        .store(20, Ordering::Release);
    upstream.targets[1]
        .active_requests
        .store(0, Ordering::Release);
    for _ in 0..32 {
        assert_eq!(
            upstream
                .select_target()
                .expect("lower-load candidate")
                .index,
            1
        );
    }
}

#[test]
fn random_two_composes_attempted_primary_backup_and_single_target_rules() {
    let mut upstream = test_weighted_upstream(&[1, 1, 100], 1, 1_000);
    upstream.load_balancing = UpstreamLoadBalancingStrategy::RandomTwoLeastConnections;
    upstream.targets[2].backup = true;

    let mut attempted = AttemptedTargets::default();
    attempted.insert(0);
    assert_eq!(
        upstream
            .select_target_excluding(&attempted)
            .expect("single unattempted primary")
            .index,
        1
    );
    attempted.insert(1);
    assert_eq!(
        upstream
            .select_target_excluding(&attempted)
            .expect("backup after primary exhaustion")
            .index,
        2
    );
    attempted.insert(2);
    assert!(upstream.select_target_excluding(&attempted).is_none());
}

#[test]
fn random_two_uses_slow_start_effective_weight_and_bounded_mapping() {
    let mut upstream = test_weighted_upstream(&[10, 1], 1, 1_000);
    upstream.load_balancing = UpstreamLoadBalancingStrategy::RandomTwoLeastConnections;
    upstream.targets[0].slow_start_duration_ms = 1_000;
    upstream.targets[0].start_slow_start(100);
    upstream.targets[0]
        .active_requests
        .store(2, Ordering::Release);
    upstream.targets[1]
        .active_requests
        .store(1, Ordering::Release);
    assert_eq!(
        upstream
            .select_random_two_least_connections(&AttemptedTargets::default(), 100, false,)
            .expect("slow-start candidates")
            .index,
        1,
        "during recovery 2/1 is more loaded than 1/1"
    );
    upstream.targets[0]
        .slow_start_started_ms
        .store(0, Ordering::Release);
    assert_eq!(
        upstream
            .select_target()
            .expect("nominal-weight candidates")
            .index,
        0,
        "at nominal weight 2/10 is less loaded than 1/1"
    );

    for bound in [1, 2, 3, 1_000, 1_000_000] {
        for _ in 0..1_000 {
            assert!(upstream.random_below(bound) < bound);
        }
    }
}

#[test]
fn ip_hash_matches_nginx_ipv4_and_ipv6_vectors() {
    assert_eq!(
        advance_ip_hash(89, "192.0.2.1".parse::<IpAddr>().expect("IPv4")),
        6255
    );
    assert_eq!(
        advance_ip_hash(89, "192.0.2.254".parse::<IpAddr>().expect("IPv4")),
        6255,
        "the fourth IPv4 octet is intentionally excluded by Nginx ip_hash"
    );
    assert_eq!(
        advance_ip_hash(89, "2001:db8::1".parse::<IpAddr>().expect("IPv6")),
        2600
    );
    assert_eq!(
        advance_ip_hash(89, "2001:db8::2".parse::<IpAddr>().expect("IPv6")),
        2601,
        "all sixteen IPv6 bytes participate in the key"
    );
}

#[test]
fn ip_hash_preserves_weighted_affinity_and_rehashes_unavailable_or_attempted_targets() {
    let mut upstream = test_weighted_upstream(&[3, 1], 1, 1_000);
    upstream.load_balancing = UpstreamLoadBalancingStrategy::IpHash;
    let affinity_ip = "10.20.2.1".parse::<IpAddr>().expect("affinity IPv4");
    assert_eq!(
        upstream
            .select_target_excluding_observed(&AttemptedTargets::default(), affinity_ip, "", None,)
            .expect("weighted affinity target")
            .index,
        1,
        "the first hash is 4827 and weighted ticket 3 maps to target one"
    );
    assert_eq!(
        upstream
            .select_target_excluding_observed(
                &AttemptedTargets::default(),
                "10.20.2.254".parse::<IpAddr>().expect("same /24 IPv4"),
                "",
                None,
            )
            .expect("same IPv4 prefix target")
            .index,
        1
    );

    upstream.targets[1]
        .active_available
        .store(false, Ordering::Release);
    assert_eq!(
        upstream
            .select_target_excluding_observed(&AttemptedTargets::default(), affinity_ip, "", None,)
            .expect("deterministic health rehash target")
            .index,
        0,
        "the second Nginx hash maps the unavailable client to target zero"
    );

    upstream.targets[1]
        .active_available
        .store(true, Ordering::Release);
    let retry_ip = "10.20.3.9".parse::<IpAddr>().expect("retry IPv4");
    let first = upstream
        .select_target_excluding_observed(&AttemptedTargets::default(), retry_ip, "", None)
        .expect("initial affinity target");
    assert_eq!(first.index, 0);
    let mut attempted = AttemptedTargets::default();
    attempted.insert(first.index);
    assert_eq!(
        upstream
            .select_target_excluding_observed(&attempted, retry_ip, "", None)
            .expect("rehash excludes attempted target")
            .index,
        1
    );
}

#[test]
fn ip_hash_keeps_primary_authoritative_and_uses_backup_when_unavailable() {
    let mut upstream = test_weighted_upstream(&[1, 1_000], 1, 1_000);
    upstream.load_balancing = UpstreamLoadBalancingStrategy::IpHash;
    upstream.targets[1].backup = true;
    let client_ip = "2001:db8::42".parse::<IpAddr>().expect("client IPv6");
    assert_eq!(
        upstream
            .select_target_excluding_observed(&AttemptedTargets::default(), client_ip, "", None,)
            .expect("primary affinity target")
            .index,
        0
    );
    upstream.targets[0]
        .active_available
        .store(false, Ordering::Release);
    assert_eq!(
        upstream
            .select_target_excluding_observed(&AttemptedTargets::default(), client_ip, "", None,)
            .expect("backup affinity target")
            .index,
        1
    );
}

#[test]
fn ip_hash_mapping_is_stable_across_fresh_generations() {
    let mut first_generation = test_weighted_upstream(&[2, 5, 1], 1, 1_000);
    first_generation.load_balancing = UpstreamLoadBalancingStrategy::IpHash;
    let mut second_generation = test_weighted_upstream(&[2, 5, 1], 1, 1_000);
    second_generation.load_balancing = UpstreamLoadBalancingStrategy::IpHash;
    let client_ip = "2001:db8:1234::9"
        .parse::<IpAddr>()
        .expect("stable client IPv6");

    let first = first_generation
        .select_target_excluding_observed(&AttemptedTargets::default(), client_ip, "", None)
        .expect("first generation target");
    let second = second_generation
        .select_target_excluding_observed(&AttemptedTargets::default(), client_ip, "", None)
        .expect("second generation target");
    assert_eq!(first.index, second.index);
}

#[test]
fn weighted_load_comparison_and_activity_lease_are_overflow_safe() {
    assert_eq!(
        super::weighted_load_cmp(usize::MAX, 1_000, usize::MAX, 999),
        std::cmp::Ordering::Less
    );

    let counter = Arc::new(AtomicUsize::new(0));
    let lease = TargetActivityLease::claim(&counter);
    assert_eq!(counter.load(Ordering::Acquire), 1);
    drop(lease);
    assert_eq!(counter.load(Ordering::Acquire), 0);

    counter.store(usize::MAX, Ordering::Release);
    let saturated = TargetActivityLease::claim(&counter);
    assert!(!saturated.acquired);
    drop(saturated);
    assert_eq!(counter.load(Ordering::Acquire), usize::MAX);

    counter.store(0, Ordering::Release);
    drop(TargetActivityLease {
        counter: counter.clone(),
        acquired: true,
    });
    assert_eq!(counter.load(Ordering::Acquire), 0);
}

#[test]
fn slow_start_effective_weight_is_monotonic_bounded_and_restartable() {
    let mut upstream = test_weighted_upstream(&[10], 1, 1_000);
    let target = &mut upstream.targets[0];
    target.slow_start_duration_ms = 1_000;
    target.start_slow_start(100);

    assert_eq!(target.effective_weight(100), 1);
    assert_eq!(target.effective_weight(599), 4);
    assert_eq!(target.effective_weight(600), 5);
    assert_eq!(target.effective_weight(1_099), 9);
    assert_eq!(target.effective_weight(1_100), 10);
    assert_eq!(target.slow_start_started_ms.load(Ordering::Acquire), 0);
    assert_eq!(
        target.ramped_weight(1_100, 1_000),
        1,
        "a concurrently restarted ramp is recomputed instead of using stale nominal weight"
    );

    target.start_slow_start(2_000);
    assert_eq!(target.effective_weight(2_000), 1);
    assert_eq!(target.effective_weight(2_500), 5);
}

#[test]
fn least_connections_uses_slow_start_effective_weight() {
    let mut upstream = test_weighted_upstream(&[10, 1], 1, 1_000);
    upstream.load_balancing = UpstreamLoadBalancingStrategy::LeastConnections;
    upstream.targets[0].slow_start_duration_ms = 1_000;
    upstream.targets[0].start_slow_start(100);
    upstream.targets[0]
        .active_requests
        .store(1, Ordering::Release);
    upstream.targets[1]
        .active_requests
        .store(1, Ordering::Release);
    upstream.cursor.store(1, Ordering::Release);

    assert_eq!(
        upstream
            .select_target()
            .expect("recovery-weight tie remains selectable")
            .index,
        1,
        "effective weights are 1:1 during the first slow-start slot"
    );
    upstream.targets[0]
        .slow_start_started_ms
        .store(0, Ordering::Release);
    assert_eq!(
        upstream
            .select_target()
            .expect("nominal least-load target remains selectable")
            .index,
        0,
        "after slow start, 1/10 is below 1/1"
    );
}

#[tokio::test]
async fn active_and_passive_recovery_start_slow_start_only_when_eligible() {
    let mut active = test_weighted_upstream(&[10], 1, 20);
    active.targets[0].slow_start_duration_ms = 1_000;
    active.active_health = Some(ActiveHealthPolicy {
        method: UpstreamActiveHealthMethod::Get,
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        unhealthy_threshold: 1,
        healthy_threshold: 1,
        success_status_min: 200,
        success_status_max: 399,
        max_response_body_bytes: 64,
    });
    assert_eq!(
        active.record_active_health(0, false),
        ActiveHealthTransition::BecameUnhealthy
    );
    assert_eq!(
        active.targets[0]
            .slow_start_started_ms
            .load(Ordering::Acquire),
        0
    );
    assert_eq!(
        active.record_active_health(0, true),
        ActiveHealthTransition::BecameHealthy
    );
    assert_ne!(
        active.targets[0]
            .slow_start_started_ms
            .load(Ordering::Acquire),
        0
    );
    assert_eq!(active.targets[0].effective_weight(active.now_ms()), 1);

    let mut passive = test_weighted_upstream(&[10], 1, 20);
    passive.targets[0].slow_start_duration_ms = 1_000;
    passive.active_health = Some(ActiveHealthPolicy {
        method: UpstreamActiveHealthMethod::Get,
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        unhealthy_threshold: 1,
        healthy_threshold: 1,
        success_status_min: 200,
        success_status_max: 399,
        max_response_body_bytes: 64,
    });
    let failed = passive.select_target().expect("initial target selection");
    passive.record_failure(failed);
    assert_eq!(
        passive.record_active_health(0, false),
        ActiveHealthTransition::BecameUnhealthy
    );
    assert_eq!(
        passive.record_active_health(0, true),
        ActiveHealthTransition::BecameHealthy
    );
    assert_eq!(
        passive.targets[0]
            .slow_start_started_ms
            .load(Ordering::Acquire),
        0,
        "active recovery must not consume slow start while passive ejection remains"
    );
    tokio::time::sleep(Duration::from_millis(25)).await;
    let probe = passive.select_target().expect("half-open probe selection");
    assert!(probe.probe);
    passive.record_success(probe);
    assert_ne!(
        passive.targets[0]
            .slow_start_started_ms
            .load(Ordering::Acquire),
        0
    );
    assert_eq!(passive.targets[0].effective_weight(passive.now_ms()), 1);

    let failed_again = passive.select_target().expect("recovered target selection");
    passive.record_failure(failed_again);
    assert_eq!(
        passive.targets[0]
            .slow_start_started_ms
            .load(Ordering::Acquire),
        0
    );
}

#[tokio::test]
async fn ejection_allows_one_half_open_probe_and_recovers_or_restarts_deadline() {
    let upstream = test_weighted_upstream(&[100], 1, 20);
    assert_eq!(upstream.aggregate_target_health(), [1, 0, 0, 0]);
    let first = upstream
        .select_target()
        .expect("healthy target is selected");
    assert!(!first.probe);
    upstream.record_failure(first);
    assert_eq!(upstream.aggregate_target_health(), [0, 1, 0, 0]);
    assert!(upstream.select_target().is_none());

    tokio::time::sleep(Duration::from_millis(25)).await;
    let failed_probe = upstream
        .select_target()
        .expect("one probe is selected after ejection");
    assert!(failed_probe.probe);
    assert_eq!(upstream.aggregate_target_health(), [0, 0, 1, 0]);
    assert!(upstream.select_target().is_none());
    upstream.record_failure(failed_probe);
    assert_eq!(upstream.aggregate_target_health(), [0, 1, 0, 0]);
    assert!(upstream.select_target().is_none());

    tokio::time::sleep(Duration::from_millis(25)).await;
    let successful_probe = upstream
        .select_target()
        .expect("new probe is selected after the restarted deadline");
    assert!(successful_probe.probe);
    upstream.record_success(successful_probe);
    assert_eq!(upstream.aggregate_target_health(), [1, 0, 0, 0]);
    let normal = upstream
        .select_target()
        .expect("successful probe restores normal selection");
    assert!(!normal.probe);
}

#[test]
fn weighted_selection_excludes_actively_unavailable_target() {
    let upstream = test_weighted_upstream(&[100, 1], 3, 1_000);
    upstream.targets[0]
        .active_available
        .store(false, std::sync::atomic::Ordering::Release);
    for _ in 0..4 {
        assert_eq!(
            upstream
                .select_target()
                .expect("eligible target remains")
                .index,
            1
        );
    }
}

#[test]
fn ejected_target_is_skipped_while_healthy_alternative_remains() {
    let upstream = test_upstream(2, 1, 1_000);
    let failed = upstream.select_target().expect("first target selected");
    assert_eq!(failed.index, 0);
    upstream.record_failure(failed);

    for _ in 0..4 {
        let selected = upstream
            .select_target()
            .expect("healthy alternative remains selectable");
        assert_eq!(selected.index, 1);
        upstream.record_success(selected);
    }
}

#[test]
fn stale_success_cannot_clear_a_newer_ejection() {
    let upstream = test_upstream(1, 1, 1_000);
    let failing = upstream.select_target().expect("first concurrent request");
    let stale_success = upstream.select_target().expect("second concurrent request");
    upstream.record_failure(failing);
    upstream.record_success(stale_success);
    assert!(
        upstream.select_target().is_none(),
        "success selected before ejection cannot clear the newer state"
    );
}

#[test]
fn active_thresholds_gate_selection_without_clearing_passive_ejection() {
    let mut upstream = test_upstream(1, 1, 1_000);
    upstream.active_health = Some(ActiveHealthPolicy {
        method: UpstreamActiveHealthMethod::Get,
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        unhealthy_threshold: 2,
        healthy_threshold: 2,
        success_status_min: 200,
        success_status_max: 399,
        max_response_body_bytes: 64,
    });

    assert_eq!(
        upstream.record_active_health(0, false),
        ActiveHealthTransition::Unchanged
    );
    assert!(upstream.select_target().is_some());
    assert_eq!(
        upstream.record_active_health(0, false),
        ActiveHealthTransition::BecameUnhealthy
    );
    assert_eq!(upstream.aggregate_target_health(), [0, 0, 0, 1]);
    assert!(upstream.select_target().is_none());
    assert_eq!(
        upstream.record_active_health(0, true),
        ActiveHealthTransition::Unchanged
    );
    assert!(upstream.select_target().is_none());
    assert_eq!(
        upstream.record_active_health(0, true),
        ActiveHealthTransition::BecameHealthy
    );
    assert_eq!(upstream.aggregate_target_health(), [1, 0, 0, 0]);

    let selected = upstream
        .select_target()
        .expect("active recovery restores selection");
    upstream.record_failure(selected);
    assert!(upstream.select_target().is_none());
    assert_eq!(
        upstream.record_active_health(0, true),
        ActiveHealthTransition::Unchanged
    );
    assert!(
        upstream.select_target().is_none(),
        "active success must not clear passive ejection"
    );
}

#[test]
fn business_success_cannot_restore_an_actively_unhealthy_target() {
    let mut upstream = test_upstream(1, 1, 1_000);
    upstream.active_health = Some(ActiveHealthPolicy {
        method: UpstreamActiveHealthMethod::Get,
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        unhealthy_threshold: 1,
        healthy_threshold: 1,
        success_status_min: 200,
        success_status_max: 399,
        max_response_body_bytes: 64,
    });
    let selected = upstream
        .select_target()
        .expect("target starts actively available");
    assert_eq!(
        upstream.record_active_health(0, false),
        ActiveHealthTransition::BecameUnhealthy
    );
    upstream.record_success(selected);
    assert!(
        upstream.select_target().is_none(),
        "business success must not clear active health state"
    );
}

#[test]
fn proxy_set_header_expands_supported_nginx_variables() {
    let mut source = HeaderMap::new();
    source.insert("upgrade", HeaderValue::from_static("websocket"));
    source.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
    let current = HeaderMap::new();
    let client = "127.0.0.1".parse::<IpAddr>().expect("ip");
    assert_eq!(
        expand_proxy_header_value(
            "$host",
            &source,
            client,
            "https",
            "server.sdkwork.com",
            &current
        )
        .expect("expand"),
        "server.sdkwork.com"
    );
    assert_eq!(
        expand_proxy_header_value(
            "$scheme",
            &source,
            client,
            "https",
            "server.sdkwork.com",
            &current
        )
        .expect("expand"),
        "https"
    );
    assert_eq!(
        expand_proxy_header_value(
            "$proxy_add_x_forwarded_for",
            &source,
            client,
            "https",
            "server.sdkwork.com",
            &current
        )
        .expect("expand"),
        "10.0.0.1, 127.0.0.1"
    );
    assert_eq!(
        expand_proxy_header_value(
            "$http_upgrade",
            &source,
            client,
            "https",
            "server.sdkwork.com",
            &current
        )
        .expect("expand"),
        "websocket"
    );
    assert!(expand_proxy_header_value(
        "$uri",
        &source,
        client,
        "https",
        "server.sdkwork.com",
        &current
    )
    .is_err());
}

#[test]
fn apply_proxy_set_headers_overrides_host_and_forwarded_chain() {
    let mut target = HeaderMap::new();
    target.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
    let source = HeaderMap::new();
    let client = "127.0.0.1".parse::<IpAddr>().expect("ip");
    apply_proxy_set_headers(
        &mut target,
        &[
            "Host $host".to_owned(),
            "X-Forwarded-For $proxy_add_x_forwarded_for".to_owned(),
            "X-Forwarded-Proto $scheme".to_owned(),
        ],
        &source,
        client,
        "https",
        "app.example.com:443",
    )
    .expect("apply");
    assert_eq!(
        target.get("host").and_then(|v| v.to_str().ok()),
        Some("app.example.com")
    );
    assert_eq!(
        target.get("x-forwarded-for").and_then(|v| v.to_str().ok()),
        Some("10.0.0.1, 127.0.0.1")
    );
    assert_eq!(
        target
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok()),
        Some("https")
    );
}
