use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

pub(super) struct FixedHistogram<const SERIES: usize, const BUCKETS: usize> {
    exclusive_buckets: [[AtomicU64; BUCKETS]; SERIES],
    sum_microseconds: [AtomicU64; SERIES],
    count: [AtomicU64; SERIES],
}

impl<const SERIES: usize, const BUCKETS: usize> FixedHistogram<SERIES, BUCKETS> {
    pub(super) fn new() -> Self {
        Self {
            exclusive_buckets: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU64::new(0))),
            sum_microseconds: std::array::from_fn(|_| AtomicU64::new(0)),
            count: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    pub(super) fn observe(
        &self,
        series: usize,
        duration: Duration,
        finite_upper_bounds_microseconds: &[u64],
    ) {
        debug_assert_eq!(finite_upper_bounds_microseconds.len() + 1, BUCKETS);
        let microseconds = duration.as_micros().min(u64::MAX as u128) as u64;
        let bucket = finite_upper_bounds_microseconds
            .iter()
            .position(|upper| duration <= Duration::from_micros(*upper))
            .unwrap_or(BUCKETS - 1);
        saturating_increment(&self.exclusive_buckets[series][bucket]);
        saturating_add(&self.sum_microseconds[series], microseconds);
        saturating_increment(&self.count[series]);
    }

    pub(super) fn cumulative_bucket(&self, series: usize, bucket: usize) -> u64 {
        self.exclusive_buckets[series][..=bucket]
            .iter()
            .fold(0_u64, |total, value| {
                total.saturating_add(value.load(Ordering::Relaxed))
            })
    }

    pub(super) fn sum_microseconds(&self, series: usize) -> u64 {
        self.sum_microseconds[series].load(Ordering::Relaxed)
    }

    pub(super) fn count(&self, series: usize) -> u64 {
        self.count[series].load(Ordering::Relaxed)
    }
}

fn saturating_increment(counter: &AtomicU64) {
    saturating_add(counter, 1);
}

fn saturating_add(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(value))
    });
}

#[cfg(test)]
mod tests {
    use std::{sync::atomic::Ordering, time::Duration};

    use super::FixedHistogram;

    #[test]
    fn stores_exclusive_fixed_buckets_and_renders_cumulative_values_without_wrap() {
        let histogram = FixedHistogram::<1, 3>::new();
        histogram.observe(0, Duration::from_micros(5), &[5, 10]);
        histogram.observe(0, Duration::from_micros(7), &[5, 10]);
        histogram.observe(0, Duration::from_micros(20), &[5, 10]);

        assert_eq!(histogram.cumulative_bucket(0, 0), 1);
        assert_eq!(histogram.cumulative_bucket(0, 1), 2);
        assert_eq!(histogram.cumulative_bucket(0, 2), 3);
        assert_eq!(histogram.sum_microseconds(0), 32);
        assert_eq!(histogram.count(0), 3);
    }

    #[test]
    fn saturates_bucket_sum_count_and_cumulative_reads() {
        let histogram = FixedHistogram::<1, 2>::new();
        histogram.exclusive_buckets[0][0].store(u64::MAX, Ordering::Relaxed);
        histogram.sum_microseconds[0].store(u64::MAX, Ordering::Relaxed);
        histogram.count[0].store(u64::MAX, Ordering::Relaxed);

        histogram.observe(0, Duration::from_micros(1), &[5]);

        assert_eq!(histogram.cumulative_bucket(0, 0), u64::MAX);
        assert_eq!(histogram.cumulative_bucket(0, 1), u64::MAX);
        assert_eq!(histogram.sum_microseconds(0), u64::MAX);
        assert_eq!(histogram.count(0), u64::MAX);
    }

    #[test]
    fn sub_microsecond_excess_does_not_fall_below_a_bucket_boundary() {
        let histogram = FixedHistogram::<1, 2>::new();
        histogram.observe(0, Duration::from_micros(5) + Duration::from_nanos(1), &[5]);

        assert_eq!(histogram.cumulative_bucket(0, 0), 0);
        assert_eq!(histogram.cumulative_bucket(0, 1), 1);
    }
}
