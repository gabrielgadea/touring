//! CPU topology detection via `/sys/devices/system/cpu`.
//!
//! Classifies CPUs into P-cores (Performance) and E-cores (Efficiency) by
//! reading the `cluster_id` sysfs attribute and applying a cluster-size
//! heuristic tuned for Intel hybrid (Alder/Raptor/Meteor Lake) and similar.
//!
//! ## Heuristic
//!
//! | Cluster size | Classification | Rationale |
//! |---|---|---|
//! | 1 | P-core | Older non-HT single-CPU cluster |
//! | 2 | P-core | HT pair — one physical P-core, two logical CPUs |
//! | 4 | E-core | Intel E-core quad cluster |
//! | > 4 | P-core | Conservative — large clusters seen on big-iron P-only CPUs |
//!
//! On the i9-14900HX (24 physical cores, 32 logical CPUs):
//! - cluster_ids 0,8,16,24,32,40,48,56 → 2 CPUs each → 16 P-core threads (cpu0-15)
//! - cluster_ids 64,72,80,88 → 4 CPUs each → 16 E-core threads (cpu16-31)

use std::collections::HashMap;
use std::fs;

use crate::sentinel::error::TopologyError;

/// Classification result for all online CPUs.
#[derive(Debug, Clone)]
pub struct TopologyMap {
    /// CPU IDs classified as Performance cores.
    pub p_cores: Vec<u32>,
    /// CPU IDs classified as Efficiency cores.
    pub e_cores: Vec<u32>,
    /// Total online CPUs (= p_cores.len() + e_cores.len()).
    pub total: u32,
}

/// Read the `cluster_id` for a single logical CPU from sysfs.
///
/// Returns `Ok(None)` when the attribute file does not exist (kernel/platform
/// does not expose cluster topology — e.g., older kernels or VM guests).
///
/// # Errors
///
/// Returns `TopologyError::SysfsUnreadable` if the file exists but cannot be
/// read (permission denied, I/O error).
/// Returns `TopologyError::ParseError` if the file content cannot be parsed as
/// a `u32`.
pub fn read_cluster_id(cpu_id: u32) -> Result<Option<u32>, TopologyError> {
    let path = format!("/sys/devices/system/cpu/cpu{}/topology/cluster_id", cpu_id);
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(TopologyError::SysfsUnreadable(e)),
    };
    let trimmed = raw.trim();
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| TopologyError::ParseError {
            cpu_id,
            field: "cluster_id",
            value: trimmed.to_string(),
        })
}

/// List all online logical CPUs by parsing `/sys/devices/system/cpu/online`.
///
/// The format is a comma-separated list of ranges, e.g. `"0-3,5,7-9"`.
///
/// # Errors
///
/// Returns `TopologyError::SysfsUnreadable` if the file cannot be read.
/// Returns `TopologyError::ParseError` if any token cannot be parsed as `u32`.
pub fn list_online_cpus() -> Result<Vec<u32>, TopologyError> {
    let path = "/sys/devices/system/cpu/online";
    let raw = fs::read_to_string(path).map_err(TopologyError::SysfsUnreadable)?;
    parse_cpu_range_list(raw.trim())
}

/// Parse a single CPU-id `token` as `u32`, tagging a parse failure with `field`
/// so the error names which position (range start/end, or a bare id) was bad.
/// Shared by both arms of [`parse_cpu_range_list`].
fn parse_cpu(token: &str, field: &'static str) -> Result<u32, TopologyError> {
    token
        .trim()
        .parse::<u32>()
        .map_err(|_| TopologyError::ParseError {
            cpu_id: 0,
            field,
            value: token.to_string(),
        })
}

/// Parse a CPU range list string (e.g. `"0-3"`, `"0,2-4,7"`) into a sorted
/// `Vec<u32>`.
///
/// This is a pure function used both in production and in unit tests.
fn parse_cpu_range_list(s: &str) -> Result<Vec<u32>, TopologyError> {
    let mut cpus = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start, end)) = part.split_once('-') {
            let a = parse_cpu(start, "online_range_start")?;
            let b = parse_cpu(end, "online_range_end")?;
            for cpu in a..=b {
                cpus.push(cpu);
            }
        } else {
            cpus.push(parse_cpu(part, "online_single")?);
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    Ok(cpus)
}

/// Classify CPUs into P/E cores based on cluster membership sizes.
///
/// This is a **pure function** — it performs no I/O and is safe to use in
/// tests with synthetic data.
///
/// # Algorithm
///
/// 1. Group `cpu_to_cluster` pairs by `cluster_id`.
/// 2. For each cluster:
///    - size == 1 → P (single non-HT core)
///    - size == 2 → P (HT pair)
///    - size == 4 → E (Intel E-core quad)
///    - size > 4  → P (conservative — big-iron P-only CPUs)
/// 3. Sort both `p_cores` and `e_cores` ascending.
///
/// # Parameters
///
/// `cpu_to_cluster`: slice of `(cpu_id, cluster_id)` pairs. May contain
/// duplicates — they are handled gracefully (same CPU added twice is
/// harmless after dedup on sort).
pub fn classify_by_cluster_size(cpu_to_cluster: &[(u32, u32)]) -> TopologyMap {
    if cpu_to_cluster.is_empty() {
        return TopologyMap {
            p_cores: Vec::new(),
            e_cores: Vec::new(),
            total: 0,
        };
    }

    // Group CPUs by cluster_id.
    let mut cluster_to_cpus: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(cpu_id, cluster_id) in cpu_to_cluster {
        cluster_to_cpus.entry(cluster_id).or_default().push(cpu_id);
    }

    let mut p_cores: Vec<u32> = Vec::new();
    let mut e_cores: Vec<u32> = Vec::new();

    for cpus in cluster_to_cpus.values() {
        match cpus.len() {
            4 => e_cores.extend_from_slice(cpus),
            // size 1, 2, or >4 → P-core
            _ => p_cores.extend_from_slice(cpus),
        }
    }

    p_cores.sort_unstable();
    p_cores.dedup();
    e_cores.sort_unstable();
    e_cores.dedup();

    let total = (p_cores.len() + e_cores.len()) as u32;
    TopologyMap {
        p_cores,
        e_cores,
        total,
    }
}

/// The degenerate topology where every online CPU is a Performance core.
///
/// The shared fallback for both `detect_topology` cases in which cluster
/// topology can't discriminate P from E cores (no `cluster_id` attributes at
/// all, or a single cluster spanning every CPU) — classify conservatively as
/// all-Performance rather than guessing.
fn all_performance(cpus: &[u32]) -> TopologyMap {
    TopologyMap {
        p_cores: cpus.to_vec(),
        e_cores: Vec::new(),
        total: cpus.len() as u32,
    }
}

/// Detect CPU topology from `/sys`, classifying each online CPU as P or E.
///
/// # Fallback
///
/// When `cluster_id` attributes are unavailable (VM, older kernel, non-Intel
/// hardware) or when all CPUs share a single cluster, every CPU is classified
/// as Performance and a `tracing::warn!` is emitted.
///
/// # Errors
///
/// Returns `TopologyError::NoCpus` if no online CPUs are found.
/// Propagates `TopologyError::SysfsUnreadable` / `TopologyError::ParseError`
/// on I/O or parse failures.
pub fn detect_topology() -> Result<TopologyMap, TopologyError> {
    let cpus = list_online_cpus()?;
    if cpus.is_empty() {
        return Err(TopologyError::NoCpus);
    }

    let mut cpu_to_cluster: Vec<(u32, u32)> = Vec::with_capacity(cpus.len());
    for &cpu_id in &cpus {
        match read_cluster_id(cpu_id) {
            Ok(Some(cluster_id)) => cpu_to_cluster.push((cpu_id, cluster_id)),
            Ok(None) => {} // cluster_id attribute absent — skip
            Err(e) => {
                tracing::warn!(
                    cpu_id,
                    error = %e,
                    "failed to read cluster_id, skipping cpu"
                );
            }
        }
    }

    // Fallback: no cluster_id info available at all.
    if cpu_to_cluster.is_empty() {
        tracing::warn!(
            cpu_count = cpus.len(),
            "cluster_id unavailable, classifying all as Performance"
        );
        return Ok(all_performance(&cpus));
    }

    // Fallback: single cluster containing all CPUs — ambiguous topology.
    let unique_clusters: std::collections::HashSet<u32> =
        cpu_to_cluster.iter().map(|&(_, c)| c).collect();
    if unique_clusters.len() == 1 && cpu_to_cluster.len() == cpus.len() {
        tracing::warn!(
            cluster_id = unique_clusters.iter().next().copied().unwrap_or(0),
            cpu_count = cpus.len(),
            "cluster_id unavailable, classifying all as Performance"
        );
        return Ok(all_performance(&cpus));
    }

    Ok(classify_by_cluster_size(&cpu_to_cluster))
}

// ─── Unit Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build i9-14900HX fixture:
    /// - 8 P-core clusters (size 2) → cpu0..cpu15
    /// - 4 E-core clusters (size 4) → cpu16..cpu31
    fn i9_14900hx_fixture() -> Vec<(u32, u32)> {
        let mut pairs = Vec::new();
        // P-core clusters: cluster_ids 0,8,16,24,32,40,48,56 → 2 CPUs each
        let p_cluster_ids = [0u32, 8, 16, 24, 32, 40, 48, 56];
        for (i, &cluster_id) in p_cluster_ids.iter().enumerate() {
            let base = (i as u32) * 2;
            pairs.push((base, cluster_id));
            pairs.push((base + 1, cluster_id));
        }
        // E-core clusters: cluster_ids 64,72,80,88 → 4 CPUs each
        let e_cluster_ids = [64u32, 72, 80, 88];
        for (i, &cluster_id) in e_cluster_ids.iter().enumerate() {
            let base = 16 + (i as u32) * 4;
            pairs.push((base, cluster_id));
            pairs.push((base + 1, cluster_id));
            pairs.push((base + 2, cluster_id));
            pairs.push((base + 3, cluster_id));
        }
        pairs
    }

    #[test]
    fn classify_i9_14900hx_16p_16e() {
        let pairs = i9_14900hx_fixture();
        let topo = classify_by_cluster_size(&pairs);

        assert_eq!(
            topo.p_cores.len(),
            16,
            "expected 16 P-core threads, got {}",
            topo.p_cores.len()
        );
        assert_eq!(
            topo.e_cores.len(),
            16,
            "expected 16 E-core threads, got {}",
            topo.e_cores.len()
        );
        assert_eq!(topo.total, 32);

        // P-cores should be cpu0..cpu15
        for i in 0u32..16 {
            assert!(topo.p_cores.contains(&i), "cpu{i} should be P-core");
        }
        // E-cores should be cpu16..cpu31
        for i in 16u32..32 {
            assert!(topo.e_cores.contains(&i), "cpu{i} should be E-core");
        }
    }

    #[test]
    fn classify_single_cpu_per_cluster_all_p() {
        // Older non-HT system: one CPU per cluster → all P-cores
        let pairs: Vec<(u32, u32)> = (0u32..4).map(|i| (i, i * 100)).collect();
        let topo = classify_by_cluster_size(&pairs);
        assert_eq!(topo.p_cores.len(), 4);
        assert!(
            topo.e_cores.is_empty(),
            "no E-cores expected for size-1 clusters"
        );
    }

    #[test]
    fn classify_empty_input_returns_empty() {
        let topo = classify_by_cluster_size(&[]);
        assert!(topo.p_cores.is_empty());
        assert!(topo.e_cores.is_empty());
        assert_eq!(topo.total, 0);
    }

    #[test]
    fn parse_cpu_range_consecutive() {
        let cpus = parse_cpu_range_list("0-3").expect("parse should succeed");
        assert_eq!(cpus, vec![0, 1, 2, 3]);
    }

    #[test]
    fn parse_cpu_range_mixed() {
        let cpus = parse_cpu_range_list("0,2-4,7").expect("parse should succeed");
        assert_eq!(cpus, vec![0, 2, 3, 4, 7]);
    }

    #[test]
    fn parse_cpu_range_single_cpu() {
        let cpus = parse_cpu_range_list("5").expect("parse should succeed");
        assert_eq!(cpus, vec![5]);
    }

    #[test]
    fn parse_cpu_range_full_system_32() {
        let cpus = parse_cpu_range_list("0-31").expect("parse should succeed");
        assert_eq!(cpus.len(), 32);
        assert_eq!(cpus[0], 0);
        assert_eq!(cpus[31], 31);
    }

    #[test]
    fn parse_cpu_range_deduplicates() {
        // Shouldn't happen in practice but must not panic or double-count.
        let cpus = parse_cpu_range_list("0-2,1-3").expect("parse should succeed");
        // After dedup: 0,1,2,3
        assert_eq!(cpus, vec![0, 1, 2, 3]);
    }

    /// Size-2 clusters (HT pairs) → P-cores; size-4 clusters → E-cores;
    /// size-1 clusters → P-cores; size-8 clusters → P-cores (> 4 → conservative P).
    #[test]
    fn classify_large_cluster_treated_as_p() {
        // 8-CPU single cluster → all P
        let pairs: Vec<(u32, u32)> = (0u32..8).map(|cpu| (cpu, 42)).collect();
        let topo = classify_by_cluster_size(&pairs);
        assert_eq!(topo.p_cores.len(), 8);
        assert!(topo.e_cores.is_empty());
    }
}
