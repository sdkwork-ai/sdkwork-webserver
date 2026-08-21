//! nginx `hash` / `hash … consistent` peer selection helpers.

use crc::{Crc, CRC_32_ISO_HDLC};

/// nginx `ngx_crc32` (IEEE / ISO-HDLC reflected CRC-32).
pub(super) const NGX_CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

#[derive(Debug, Clone, Copy)]
pub(super) struct ConsistentHashPoint {
    pub hash: u32,
    pub target_index: usize,
}

/// Build nginx ketama points: `crc32(HOST \0 PORT PREV_HASH)` × `weight * 160`.
pub(super) fn build_consistent_hash_points(
    targets: &[(usize, &str, usize)],
) -> Vec<ConsistentHashPoint> {
    let mut points = Vec::new();
    for &(target_index, server, weight) in targets {
        if weight == 0 {
            continue;
        }
        let (host, port) = split_server_host_port(server);
        let mut prev_hash = [0u8; 4];
        let npoints = weight.saturating_mul(160);
        for _ in 0..npoints {
            let mut digest = NGX_CRC32.digest();
            digest.update(host.as_bytes());
            digest.update(&[0]);
            digest.update(port.as_bytes());
            digest.update(&prev_hash);
            let hash = digest.finalize();
            points.push(ConsistentHashPoint {
                hash,
                target_index,
            });
            prev_hash = hash.to_le_bytes();
        }
    }
    points.sort_by(|left, right| {
        left.hash
            .cmp(&right.hash)
            .then(left.target_index.cmp(&right.target_index))
    });
    points.dedup_by(|left, right| left.hash == right.hash);
    points
}

pub(super) fn find_consistent_hash_point(points: &[ConsistentHashPoint], hash: u32) -> usize {
    if points.is_empty() {
        return 0;
    }
    match points.binary_search_by(|point| point.hash.cmp(&hash)) {
        Ok(index) => index,
        Err(index) if index >= points.len() => 0,
        Err(index) => index,
    }
}

/// nginx plain-hash step: `((crc32([REHASH] KEY) >> 16) & 0x7fff)`.
pub(super) fn nginx_hash_step(key: &[u8], rehash: usize) -> u32 {
    let mut digest = NGX_CRC32.digest();
    if rehash > 0 {
        let rehash_text = rehash.to_string();
        digest.update(rehash_text.as_bytes());
    }
    digest.update(key);
    let hash = digest.finalize();
    (hash >> 16) & 0x7fff
}

fn split_server_host_port(server: &str) -> (&str, &str) {
    let trimmed = server
        .strip_prefix("http://")
        .or_else(|| server.strip_prefix("https://"))
        .unwrap_or(server);
    if let Some(rest) = trimmed.strip_prefix("unix:") {
        return (rest, "");
    }
    if let Some((host, port)) = trimmed.rsplit_once(':') {
        if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) {
            return (host, port);
        }
    }
    (trimmed, "")
}

#[cfg(test)]
mod tests {
    use super::{build_consistent_hash_points, find_consistent_hash_point, nginx_hash_step};

    #[test]
    fn plain_hash_step_is_stable() {
        let first = nginx_hash_step(b"/api/v1", 0);
        let second = nginx_hash_step(b"/api/v1", 0);
        assert_eq!(first, second);
        assert_ne!(nginx_hash_step(b"/api/v1", 1), first);
    }

    #[test]
    fn consistent_ring_covers_targets() {
        let points =
            build_consistent_hash_points(&[(0, "10.0.0.1:8080", 1), (1, "10.0.0.2:8080", 1)]);
        assert!(points.len() >= 2);
        let index = find_consistent_hash_point(&points, 0);
        assert!(index < points.len());
    }
}
