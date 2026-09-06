//! Application dispatch-load migration using a thread's creation epoch.

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub cpu: usize,
    pub loads: [u64; 3],
    pub idle_mask: u8,
}

/// Called only with a scheduler-lock snapshot at the timer context safe point.
/// Lifelong dispatch totals remain available for accounting/initial placement;
/// migration compares only dispatches since this thread was created.
pub(crate) fn migration_target(
    source_cpu: usize,
    totals: [u64; 3],
    baseline: [u64; 3],
    idle_mask: u8,
    start: usize,
    delta: u64,
) -> Option<Placement> {
    if !(1..=3).contains(&source_cpu) || !(1..=3).contains(&start) {
        return None;
    }
    let loads = core::array::from_fn(|index| totals[index].saturating_sub(baseline[index]));
    let idle_mask = idle_mask & 0xe;
    let mut cpu = start;
    let mut selected_load = u64::MAX;
    for offset in 0..3 {
        let candidate = 1 + (start - 1 + offset) % 3;
        if idle_mask != 0 && idle_mask & (1 << candidate) == 0 {
            continue;
        }
        if loads[candidate - 1] < selected_load {
            cpu = candidate;
            selected_load = loads[candidate - 1];
        }
    }
    if cpu == source_cpu || loads[source_cpu - 1].saturating_sub(loads[cpu - 1]) < delta {
        return None;
    }
    Some(Placement { cpu, loads, idle_mask })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_group_does_not_inherit_prior_native_dispatch_skew() {
        let baseline = [2_000_000, 5_000_000, 0];
        // New Python worker on AP3 creates 4096 dispatches while peers sleep.
        // Comparing lifetime totals would keep it on AP3 indefinitely.
        let next = migration_target(3, [2_000_002, 5_000_001, 4096], baseline, 0x6, 1, 64)
            .unwrap();
        assert_eq!(next, Placement { cpu: 2, loads: [2, 1, 4096], idle_mask: 0x6 });
    }

    #[test]
    fn equal_lifetime_and_fresh_work_choose_different_targets() {
        let baseline = [10_000, 20_000, 30_000];
        let totals = [10_112, 20_003, 30_001];
        assert_eq!(migration_target(1, totals, [0; 3], 0xc, 1, 64), None);
        assert_eq!(migration_target(1, totals, baseline, 0xc, 1, 64).unwrap().cpu, 3);
    }

    #[test]
    fn migration_still_requires_the_full_dispatch_delta() {
        assert_eq!(migration_target(1, [163, 100, 100], [100; 3], 0xc, 2, 64), None);
        assert_eq!(migration_target(1, [164, 100, 100], [100; 3], 0xc, 2, 64).unwrap().cpu, 2);
        assert_eq!(migration_target(1, [100; 3], [100; 3], 0xe, 2, 64), None);
    }

    #[test]
    fn idle_preference_and_round_robin_ties_are_preserved() {
        let totals = [100, 1, 10];
        assert_eq!(migration_target(1, totals, [0; 3], 0x8, 2, 64).unwrap().cpu, 3);
        assert_eq!(migration_target(1, totals, [0; 3], 0, 2, 64).unwrap().cpu, 2);
        assert_eq!(migration_target(1, [100, 1, 1], [0; 3], 0xc, 3, 64).unwrap().cpu, 3);
    }

    #[test]
    fn saturation_does_not_manufacture_dispatches_or_delta() {
        assert_eq!(migration_target(1, [0, 100, 100], [u64::MAX, 100, 100], 0xc, 2, 64), None);
        assert_eq!(migration_target(1, [u64::MAX; 3], [0; 3], 0xc, 2, 64), None);
        assert_eq!(migration_target(1, [u64::MAX; 3], [u64::MAX; 3], 0xc, 2, 64), None);
    }

    #[test]
    fn never_selects_cpu_zero_or_accepts_invalid_source() {
        assert_eq!(migration_target(0, [100, 0, 0], [0; 3], 0xe, 1, 64), None);
        assert_eq!(migration_target(1, [100, 0, 0], [0; 3], 0xe, 0, 64), None);
        assert_ne!(migration_target(1, [100, 0, 0], [0; 3], 1, 2, 64).unwrap().cpu, 0);
    }
}
