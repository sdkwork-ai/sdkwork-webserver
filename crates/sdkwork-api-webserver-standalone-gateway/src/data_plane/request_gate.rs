use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::resource_pressure::ResourcePressureController;

pub(super) struct RequestAdmissionGate {
    total: Arc<Semaphore>,
    business: Arc<Semaphore>,
    total_capacity: AtomicUsize,
    business_capacity: AtomicUsize,
    resource_pressure: Arc<ResourcePressureController>,
}

#[derive(Debug)]
pub(super) struct RequestAdmissionPermit {
    total: Option<OwnedSemaphorePermit>,
    business: Option<OwnedSemaphorePermit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestAdmissionRejection {
    Saturated,
    ResourcePressure,
}

impl RequestAdmissionGate {
    pub(super) fn new(
        maximum_requests: usize,
        operations_reserve: usize,
        resource_pressure: Arc<ResourcePressureController>,
    ) -> Self {
        Self {
            total: Arc::new(Semaphore::new(maximum_requests)),
            business: Arc::new(Semaphore::new(
                maximum_requests.saturating_sub(operations_reserve),
            )),
            total_capacity: AtomicUsize::new(maximum_requests),
            business_capacity: AtomicUsize::new(
                maximum_requests.saturating_sub(operations_reserve),
            ),
            resource_pressure,
        }
    }

    /// Reconciles both capacities to a published generation's limits (a
    /// `maxConcurrentRequests` change hot-applies). A shrinking capacity only
    /// drops currently-available permits; the remainder converges as in-flight
    /// requests release theirs.
    pub(super) fn resize(&self, maximum_requests: usize, operations_reserve: usize) {
        let total_target = maximum_requests;
        let business_target = maximum_requests.saturating_sub(operations_reserve);
        let total_current = self.total_capacity.swap(total_target, Ordering::AcqRel);
        match total_target.cmp(&total_current) {
            std::cmp::Ordering::Greater => self.total.add_permits(total_target - total_current),
            std::cmp::Ordering::Less => {
                let drop = (total_current - total_target).min(self.total.available_permits());
                if drop > 0 {
                    self.total.forget_permits(drop);
                }
            }
            std::cmp::Ordering::Equal => {}
        }
        let business_current = self
            .business_capacity
            .swap(business_target, Ordering::AcqRel);
        match business_target.cmp(&business_current) {
            std::cmp::Ordering::Greater => self
                .business
                .add_permits(business_target - business_current),
            std::cmp::Ordering::Less => {
                let drop =
                    (business_current - business_target).min(self.business.available_permits());
                if drop > 0 {
                    self.business.forget_permits(drop);
                }
            }
            std::cmp::Ordering::Equal => {}
        }
    }

    pub(super) fn try_begin(&self) -> Result<RequestAdmissionPermit, RequestAdmissionRejection> {
        self.total
            .clone()
            .try_acquire_owned()
            .map(|permit| RequestAdmissionPermit {
                total: Some(permit),
                business: None,
            })
            .map_err(|_| RequestAdmissionRejection::Saturated)
    }

    pub(super) fn classify(
        &self,
        permit: &mut RequestAdmissionPermit,
        operations_reserved: bool,
    ) -> Result<(), RequestAdmissionRejection> {
        if permit.total.is_none() {
            return Err(RequestAdmissionRejection::Saturated);
        }
        if operations_reserved {
            return Ok(());
        }
        if self.resource_pressure.is_pressured() {
            permit.release();
            return Err(RequestAdmissionRejection::ResourcePressure);
        }
        permit.business = match self.business.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                permit.release();
                return Err(RequestAdmissionRejection::Saturated);
            }
        };
        Ok(())
    }

    #[cfg(test)]
    fn available_total(&self) -> usize {
        self.total.available_permits()
    }
}

impl RequestAdmissionPermit {
    fn release(&mut self) {
        self.business.take();
        self.total.take();
    }

    #[cfg(test)]
    pub(super) fn single(permit: OwnedSemaphorePermit) -> Self {
        Self {
            total: Some(permit),
            business: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RequestAdmissionGate, RequestAdmissionRejection};
    use crate::data_plane::resource_pressure::ResourcePressureController;

    #[test]
    fn operations_capacity_is_reserved_and_total_capacity_remains_bounded() {
        let pressure = ResourcePressureController::new(true);
        let gate = RequestAdmissionGate::new(3, 1, pressure);

        let mut first = gate.try_begin().expect("first request total permit");
        gate.classify(&mut first, false)
            .expect("first business request");
        let mut second = gate.try_begin().expect("second request total permit");
        gate.classify(&mut second, false)
            .expect("second business request");

        let mut rejected = gate.try_begin().expect("operations reserve remains");
        assert_eq!(
            gate.classify(&mut rejected, false)
                .expect_err("business capacity is exhausted"),
            RequestAdmissionRejection::Saturated
        );
        assert_eq!(
            gate.available_total(),
            1,
            "rejected business request releases total reserve immediately"
        );
        let mut operation = gate
            .try_begin()
            .expect("released operations reserve remains");
        gate.classify(&mut operation, true)
            .expect("validated operation uses the reserved capacity");
        assert_eq!(gate.available_total(), 0);
        assert_eq!(
            gate.try_begin().expect_err("total capacity is bounded"),
            RequestAdmissionRejection::Saturated
        );

        drop((first, second, rejected, operation));
        assert_eq!(gate.available_total(), 3);
    }
}
