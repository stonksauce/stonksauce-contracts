// One tree per shard. CPI create is capped at 10 KiB, so a shard must fit.
pub const MAX_LEAVES: usize = 512;
pub const MAX_INDEX: u32 = 511;

pub fn add(tree: &mut [u64; MAX_LEAVES], index: u32, amount: u64) {
    let n = MAX_INDEX as usize;
    let mut i = index as usize;
    while i >= 1 && i <= n {
        tree[i] = tree[i].saturating_add(amount);
        i += i & i.wrapping_neg();
    }
}

pub fn sub(tree: &mut [u64; MAX_LEAVES], index: u32, amount: u64) {
    let n = MAX_INDEX as usize;
    let mut i = index as usize;
    while i >= 1 && i <= n {
        tree[i] = tree[i].saturating_sub(amount);
        i += i & i.wrapping_neg();
    }
}

/// Sum of weights at indices `1..=index`.
pub fn prefix_sum(tree: &[u64; MAX_LEAVES], index: u32) -> u64 {
    let mut i = (index as usize).min(MAX_INDEX as usize);
    let mut sum = 0u64;
    while i > 0 {
        sum = sum.saturating_add(tree[i]);
        i &= i - 1;
    }
    sum
}

/// Weight of the single leaf at `index`.
pub fn leaf(tree: &[u64; MAX_LEAVES], index: u32) -> u64 {
    if index == 0 {
        return 0;
    }
    prefix_sum(tree, index).saturating_sub(prefix_sum(tree, index - 1))
}

/// Replaces the tree with one whose leaves are `weights`, in O(n).
pub fn rebuild(tree: &mut [u64; MAX_LEAVES], weights: &[u64; MAX_LEAVES]) {
    let n = MAX_INDEX as usize;
    tree[0] = 0;
    tree[1..=n].copy_from_slice(&weights[1..=n]);
    for i in 1..=n {
        let parent = i + (i & i.wrapping_neg());
        if parent <= n {
            tree[parent] = tree[parent].saturating_add(tree[i]);
        }
    }
}

/// Smallest index whose prefix sum exceeds `target`. Zero-weight leaves are
/// never returned while `target < prefix_sum(MAX_INDEX)`.
pub fn find(tree: &[u64; MAX_LEAVES], mut target: u64) -> u32 {
    let n = MAX_INDEX as usize;
    let mut idx = 0usize;
    let mut bit = 1usize << n.ilog2();
    while bit > 0 {
        let next = idx + bit;
        if next <= n && tree[next] <= target {
            target -= tree[next];
            idx = next;
        }
        bit >>= 1;
    }
    (idx + 1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_with(weights: &[(u32, u64)]) -> [u64; MAX_LEAVES] {
        let mut tree = [0u64; MAX_LEAVES];
        for &(i, w) in weights {
            add(&mut tree, i, w);
        }
        tree
    }

    #[test]
    fn find_maps_tickets_to_owners() {
        let tree = tree_with(&[(1, 50), (2, 30), (3, 20)]);
        for (ticket, expect) in [(0, 1), (49, 1), (50, 2), (79, 2), (80, 3), (99, 3)] {
            assert_eq!(find(&tree, ticket), expect, "ticket {ticket}");
        }
        assert_eq!(prefix_sum(&tree, MAX_INDEX), 100);
    }

    #[test]
    fn zero_weight_leaf_is_never_drawn() {
        let mut tree = tree_with(&[(1, 10), (2, 10), (3, 10)]);
        sub(&mut tree, 2, 10);
        for ticket in 0..20 {
            assert_ne!(find(&tree, ticket), 2);
        }
        assert_eq!(prefix_sum(&tree, MAX_INDEX), 20);
    }

    #[test]
    fn last_slot_is_reachable() {
        let tree = tree_with(&[(1, 5), (MAX_INDEX, 7)]);
        assert_eq!(find(&tree, 4), 1);
        assert_eq!(find(&tree, 5), MAX_INDEX);
        assert_eq!(find(&tree, 11), MAX_INDEX);
    }

    #[test]
    fn amounts_above_i64_max_stay_consistent() {
        let big = u64::MAX / 2 + 10;
        let mut tree = tree_with(&[(4, big)]);
        assert_eq!(prefix_sum(&tree, MAX_INDEX), big);
        sub(&mut tree, 4, big);
        assert_eq!(prefix_sum(&tree, MAX_INDEX), 0);
    }

    #[test]
    fn rebuild_matches_incremental_adds() {
        let mut weights = [0u64; MAX_LEAVES];
        for i in 1..=MAX_INDEX as usize {
            weights[i] = (i as u64 * 7919) % 1000;
        }
        let mut incremental = [0u64; MAX_LEAVES];
        for i in 1..=MAX_INDEX {
            add(&mut incremental, i, weights[i as usize]);
        }
        let mut rebuilt = [123u64; MAX_LEAVES];
        rebuild(&mut rebuilt, &weights);
        assert_eq!(rebuilt, incremental);
    }

    #[test]
    fn leaf_reads_back_single_weights() {
        let tree = tree_with(&[(1, 50), (2, 30), (7, 20), (MAX_INDEX, 9)]);
        assert_eq!(leaf(&tree, 1), 50);
        assert_eq!(leaf(&tree, 2), 30);
        assert_eq!(leaf(&tree, 3), 0);
        assert_eq!(leaf(&tree, 7), 20);
        assert_eq!(leaf(&tree, MAX_INDEX), 9);
        assert_eq!(leaf(&tree, 0), 0);
    }

    #[test]
    fn index_zero_is_ignored() {
        let mut tree = [0u64; MAX_LEAVES];
        add(&mut tree, 0, 99);
        assert_eq!(tree, [0u64; MAX_LEAVES]);
    }
}
