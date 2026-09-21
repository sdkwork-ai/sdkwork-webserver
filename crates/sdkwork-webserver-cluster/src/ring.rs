//! Consistent hash ring (ketama-style) over discovered instance ids.
//!
//! Used by [`crate::strategy::LoadBalancingStrategy::ConsistentHash`]: the
//! same request key maps to the same instance while membership is stable,
//! and only `~1/n` of keys remap when instances join or leave.

use std::sync::Arc;

use super::instance::DiscoveredInstance;

/// Virtual nodes per physical instance. 160 is the classic ketama default;
/// it keeps the variance of the load distribution low for fleets of tens of
/// nodes while keeping the ring cheap to build on snapshot swap.
const VNODES_PER_INSTANCE: u32 = 160;

/// An immutable hash ring built from one snapshot's routeable instances.
#[derive(Debug)]
pub struct ConsistentHashRing {
    ring: Vec<(u64, Arc<DiscoveredInstance>)>,
}

impl ConsistentHashRing {
    /// Builds a ring from routeable instances; empty when none are
    /// routeable.
    pub fn build(instances: &[Arc<DiscoveredInstance>]) -> Self {
        let mut ring = Vec::with_capacity(instances.len() * VNODES_PER_INSTANCE as usize);
        for instance in instances {
            if !instance.is_routeable() {
                continue;
            }
            for vnode in 0..VNODES_PER_INSTANCE {
                let key = format!("{}#{vnode}", instance.id);
                ring.push((fnv1a64(key.as_bytes()), Arc::clone(instance)));
            }
        }
        ring.sort_by_key(|(hash, _)| *hash);
        Self { ring }
    }

    /// Resolves the owner of `key`: the first vnode at or after the hash.
    pub fn resolve(&self, key: &[u8]) -> Option<Arc<DiscoveredInstance>> {
        if self.ring.is_empty() {
            return None;
        }
        let hash = fnv1a64(key);
        let index = self
            .ring
            .partition_point(|(vnode_hash, _)| *vnode_hash < hash);
        let bounded = index % self.ring.len();
        Some(Arc::clone(&self.ring[bounded].1))
    }

    /// Number of virtual nodes on the ring.
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    /// True when the ring holds no virtual nodes.
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

/// FNV-1a 64-bit with a splitmix64 finalizer: fast, dependency-free, and
/// well-distributed even for the short keys ring vnodes produce (raw FNV
/// avalanches too slowly on short inputs).
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    splitmix64_finalize(hash)
}

fn splitmix64_finalize(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(id: &str) -> Arc<DiscoveredInstance> {
        Arc::new(DiscoveredInstance::new(id, format!("https://{id}:443")))
    }

    #[test]
    fn ring_resolves_stably_for_stable_membership() {
        let instances = vec![instance("a"), instance("b"), instance("c")];
        let ring = ConsistentHashRing::build(&instances);
        assert_eq!(ring.len(), 3 * 160);
        let first = ring.resolve(b"request-key").expect("owner");
        for _ in 0..100 {
            assert_eq!(ring.resolve(b"request-key").expect("owner").id, first.id);
        }
    }

    #[test]
    fn ring_distributes_across_instances() {
        let instances = vec![instance("a"), instance("b"), instance("c")];
        let ring = ConsistentHashRing::build(&instances);
        let mut owners = std::collections::HashSet::new();
        for index in 0..300_u32 {
            if let Some(owner) = ring.resolve(index.to_string().as_bytes()) {
                owners.insert(owner.id.clone());
            }
        }
        assert_eq!(owners.len(), 3, "300 keys must spread over 3 instances");
    }

    #[test]
    fn removing_one_instance_removes_only_its_keys_share() {
        let instances = vec![instance("a"), instance("b"), instance("c")];
        let before = ConsistentHashRing::build(&instances);
        let after = ConsistentHashRing::build(&instances[..2]);
        let mut unchanged = 0_u32;
        for index in 0..300_u32 {
            let key = index.to_string();
            let old = before.resolve(key.as_bytes()).expect("owner");
            let new = after.resolve(key.as_bytes()).expect("owner");
            if old.id == new.id {
                unchanged += 1;
            }
        }
        // With 1/3 of capacity removed, ~2/3 of keys keep their owner; the
        // classic consistent-hashing guarantee (vs ~0% for naive modulo).
        assert!(
            unchanged >= 150,
            "consistency violated: only {unchanged}/300 keys kept their owner"
        );
    }

    #[test]
    fn empty_ring_resolves_nothing() {
        let ring = ConsistentHashRing::build(&[]);
        assert!(ring.is_empty());
        assert!(ring.resolve(b"any").is_none());
    }
}
