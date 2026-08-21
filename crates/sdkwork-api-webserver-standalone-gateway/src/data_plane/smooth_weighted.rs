use std::sync::{Mutex, MutexGuard, TryLockError};

use super::proxy::{AttemptedTargets, ProxyTarget, SelectedTarget};

pub(super) struct SmoothWeightedState {
    state: Mutex<SmoothState>,
}

struct SmoothState {
    current_weights: Box<[i64]>,
    recovery_markers: Box<[u64]>,
}

pub(super) struct SmoothSelection<'a> {
    pub target: Option<SelectedTarget<'a>>,
    pub contended: bool,
}

pub(super) struct SmoothTargetStateGuard<'a> {
    state: MutexGuard<'a, SmoothState>,
    index: usize,
}

impl SmoothWeightedState {
    pub(super) fn new(target_count: usize) -> Self {
        Self {
            state: Mutex::new(SmoothState {
                current_weights: vec![0; target_count].into_boxed_slice(),
                recovery_markers: vec![0; target_count].into_boxed_slice(),
            }),
        }
    }

    pub(super) fn select<'a>(
        &self,
        targets: &'a [ProxyTarget],
        attempted: &AttemptedTargets,
        now_ms: u64,
        use_backups: bool,
    ) -> SmoothSelection<'a> {
        let (mut state, contended) = self.lock();
        debug_assert_eq!(state.current_weights.len(), targets.len());
        debug_assert_eq!(state.recovery_markers.len(), targets.len());

        let mut total_weight = 0_i64;
        let mut selected_index = None;
        for (index, target) in targets.iter().enumerate() {
            synchronize_recovery(&mut state, index, target.slow_start_marker());
            if !target.is_eligible(now_ms) {
                state.current_weights[index] = 0;
                continue;
            }
            if attempted.contains(index) || target.backup != use_backups {
                continue;
            }
            let effective_weight = target.effective_weight(now_ms) as i64;
            state.current_weights[index] =
                state.current_weights[index].saturating_add(effective_weight);
            total_weight = total_weight.saturating_add(effective_weight);
            if selected_index.is_none_or(|selected| {
                state.current_weights[index] > state.current_weights[selected]
            }) {
                selected_index = Some(index);
            }
        }

        let target = selected_index.and_then(|index| {
            if let Some(selected) = targets[index].try_select(index, now_ms) {
                state.current_weights[index] =
                    state.current_weights[index].saturating_sub(total_weight);
                return Some(selected);
            }

            state.current_weights[index] = 0;
            select_race_fallback(&mut state, targets, attempted, now_ms, use_backups, index)
        });
        SmoothSelection { target, contended }
    }

    pub(super) fn lock_target(&self, index: usize) -> SmoothTargetStateGuard<'_> {
        let (state, _) = self.lock();
        debug_assert!(index < state.current_weights.len());
        SmoothTargetStateGuard { state, index }
    }

    fn lock(&self) -> (MutexGuard<'_, SmoothState>, bool) {
        match self.state.try_lock() {
            Ok(state) => (state, false),
            Err(TryLockError::WouldBlock) => (
                self.state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
                true,
            ),
            Err(TryLockError::Poisoned(poisoned)) => (poisoned.into_inner(), false),
        }
    }
}

impl SmoothTargetStateGuard<'_> {
    pub(super) fn reset(&mut self, recovery_marker: u64) {
        self.state.current_weights[self.index] = 0;
        self.state.recovery_markers[self.index] = recovery_marker;
    }
}

fn synchronize_recovery(state: &mut SmoothState, index: usize, marker: u64) {
    if marker != 0 && marker != state.recovery_markers[index] {
        state.current_weights[index] = 0;
    }
    state.recovery_markers[index] = marker;
}

fn select_race_fallback<'a>(
    state: &mut SmoothState,
    targets: &'a [ProxyTarget],
    attempted: &AttemptedTargets,
    now_ms: u64,
    use_backups: bool,
    failed_index: usize,
) -> Option<SelectedTarget<'a>> {
    let mut total_weight = 0_i64;
    let mut selected_index = None;
    for (index, target) in targets.iter().enumerate() {
        if !target.is_eligible(now_ms) {
            state.current_weights[index] = 0;
            continue;
        }
        if index == failed_index || attempted.contains(index) || target.backup != use_backups {
            continue;
        }
        total_weight = total_weight.saturating_add(target.effective_weight(now_ms) as i64);
        if selected_index
            .is_none_or(|selected| state.current_weights[index] > state.current_weights[selected])
        {
            selected_index = Some(index);
        }
    }
    let selected_index = selected_index?;
    let selected = targets[selected_index].try_select(selected_index, now_ms)?;
    state.current_weights[selected_index] =
        state.current_weights[selected_index].saturating_sub(total_weight);
    Some(selected)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::SmoothWeightedState;

    #[test]
    fn poisoned_state_is_recovered_without_losing_fixed_cardinality() {
        let state = Arc::new(SmoothWeightedState::new(3));
        let panicking = state.clone();
        assert!(std::thread::spawn(move || {
            let _guard = panicking.state.lock().expect("initial lock");
            panic!("poison smooth state for recovery test");
        })
        .join()
        .is_err());

        let (recovered, contended) = state.lock();
        assert!(!contended);
        assert_eq!(recovered.current_weights.len(), 3);
        assert_eq!(recovered.recovery_markers.len(), 3);
    }
}
