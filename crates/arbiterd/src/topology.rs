//! Which logical CPUs Windows gets, in order. Guest cores are handed out a whole physical core
//! (all SMT siblings) at a time, so host and guest never share a core's execution units:
//! on a 4C/8T laptop where siblings are (0,4) (1,5) (2,6) (3,7), a naive "highest numbers" split
//! would give Windows 4-7 and Linux 0-3 — every physical core shared. CPU 0 stays with the host
//! as long as possible (it takes most interrupts).

use std::fs;

/// Sibling groups -> logical CPUs in the order the guest should take them:
/// physical cores ordered by their lowest CPU id, highest first; siblings stay adjacent.
pub fn guest_core_order(mut groups: Vec<Vec<u32>>) -> Vec<u32> {
    for g in &mut groups {
        g.sort_unstable();
        g.dedup();
    }
    groups.retain(|g| !g.is_empty());
    groups.sort_unstable();
    groups.dedup();
    groups.into_iter().rev().flatten().collect()
}

/// Parse a kernel CPU list such as `0-3,8,10-11`.
pub fn parse_cpu_list(s: &str) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for part in s.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
                if a > b {
                    return None;
                }
                out.extend(a..=b);
            }
            None => out.push(part.parse().ok()?),
        }
    }
    Some(out)
}

/// Read the online CPUs and their SMT siblings from sysfs. Falls back to one group per CPU
/// (no SMT information) if a sibling list is missing.
pub fn read_guest_core_order() -> Option<Vec<u32>> {
    let online = parse_cpu_list(&fs::read_to_string("/sys/devices/system/cpu/online").ok()?)?;
    let groups = online
        .iter()
        .map(|&c| {
            fs::read_to_string(format!(
                "/sys/devices/system/cpu/cpu{c}/topology/thread_siblings_list"
            ))
            .ok()
            .and_then(|s| parse_cpu_list(&s))
            .map(|sib| sib.into_iter().filter(|x| online.contains(x)).collect())
            .unwrap_or_else(|| vec![c])
        })
        .collect();
    Some(guest_core_order(groups))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smt_siblings_stay_together_and_cpu0_is_last() {
        // i5-1035G1 layout: siblings (0,4) (1,5) (2,6) (3,7), reported once per CPU.
        let groups = (0..8).map(|c| vec![c % 4, c % 4 + 4]).collect();
        let order = guest_core_order(groups);
        assert_eq!(order, vec![3, 7, 2, 6, 1, 5, 0, 4]);
        // 4 guest cores = two whole physical cores; host keeps the other two.
        let mut guest = order[..4].to_vec();
        guest.sort();
        assert_eq!(guest, vec![2, 3, 6, 7]);
    }

    #[test]
    fn adjacent_sibling_numbering() {
        let groups = vec![vec![0, 1], vec![2, 3], vec![0, 1], vec![2, 3]];
        assert_eq!(guest_core_order(groups), vec![2, 3, 0, 1]);
    }

    #[test]
    fn no_smt() {
        let groups = (0..4).map(|c| vec![c]).collect();
        assert_eq!(guest_core_order(groups), vec![3, 2, 1, 0]);
    }

    #[test]
    fn cpu_lists() {
        assert_eq!(
            parse_cpu_list("0-3,8,10-11\n"),
            Some(vec![0, 1, 2, 3, 8, 10, 11])
        );
        assert_eq!(parse_cpu_list("5"), Some(vec![5]));
        assert_eq!(parse_cpu_list("3-1"), None);
        assert_eq!(parse_cpu_list("x"), None);
    }
}
