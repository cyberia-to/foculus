// ---
// tags: sync, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Pure helpers for `node` (which is gated behind the `net` feature), split
//! out so they can be unit-tested without pulling in iroh/tokio.

/// Compute deterministic (k, n) from the number of alive peers.
/// All nodes using the same algorithm + same device set = same result.
pub fn compute_params(alive_devices: usize, redundancy_f: usize) -> (usize, usize) {
    if alive_devices < 2 {
        return (1, 1);
    }
    let n = alive_devices.min(8).next_power_of_two();
    let f = redundancy_f.min(n - 1);
    let k = n - f;
    (k, n)
}

/// Format a byte count for display. `0` means "unlimited" (see
/// `node::add_peer`'s `capacity` parameter, and `placement`'s handling of
/// the same convention).
pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "unlimited".to_string();
    }
    const GB: u64 = 1_000_000_000;
    const MB: u64 = 1_000_000;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_params_below_two_devices_is_a_single_unreplicated_shard() {
        assert_eq!(compute_params(0, 0), (1, 1));
        assert_eq!(compute_params(1, 3), (1, 1));
    }

    #[test]
    fn compute_params_n_is_the_next_power_of_two_capped_at_eight() {
        assert_eq!(compute_params(2, 0).1, 2);
        assert_eq!(compute_params(3, 0).1, 4);
        assert_eq!(compute_params(5, 0).1, 8);
        assert_eq!(compute_params(8, 0).1, 8);
        assert_eq!(compute_params(9, 0).1, 8, "capped at 8 regardless of more alive devices");
        assert_eq!(compute_params(1_000, 0).1, 8);
    }

    #[test]
    fn compute_params_redundancy_is_capped_at_n_minus_one() {
        // n = 4 (alive_devices = 4); redundancy_f capped at n - 1 = 3, so
        // k never drops below 1.
        assert_eq!(compute_params(4, 0), (4, 4));
        assert_eq!(compute_params(4, 1), (3, 4));
        assert_eq!(compute_params(4, 3), (1, 4));
        assert_eq!(compute_params(4, 100), (1, 4));
    }

    #[test]
    fn format_bytes_zero_is_unlimited() {
        assert_eq!(format_bytes(0), "unlimited");
    }

    #[test]
    fn format_bytes_picks_the_largest_whole_unit() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(999_999), "999999 B");
        assert_eq!(format_bytes(1_000_000), "1.0 MB");
        assert_eq!(format_bytes(1_500_000), "1.5 MB");
        assert_eq!(format_bytes(999_999_999), "1000.0 MB");
        assert_eq!(format_bytes(1_000_000_000), "1.0 GB");
        assert_eq!(format_bytes(10_000_000_000), "10.0 GB");
    }
}
