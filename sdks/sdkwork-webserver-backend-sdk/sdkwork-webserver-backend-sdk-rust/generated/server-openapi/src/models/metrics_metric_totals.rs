use serde::{Deserialize, Serialize};

use crate::models::{MetricsWindowValue};

/// One metric across every window. `metric` is one of the entity ids (`users`, `tenants`, `applications`, `agents`), one of the storage ids (`storage.used_bytes`, `storage.object_count`), or a metered dimension (`traffic.requests`, …), which is left open on the same terms as the traffic readings: a dimension this contract has not heard of reaches the surface instead of failing the response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MetricsMetricTotals {
    pub metric: String,

    pub unit: String,

    /// One entry per window, in the contract's window order. Reported in full rather than sparsely: a window that summed nothing is a real `0`, and a missing entry would leave a surface guessing between "zero" and "not measured".
    pub values: Vec<MetricsWindowValue>,
}
