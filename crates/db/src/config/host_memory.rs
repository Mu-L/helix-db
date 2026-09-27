//! Memory ceiling of the current process, for memory-proportional defaults.
//!
//! The ceiling is the tightest cgroup memory limit on the process's cgroup
//! and its ancestors (cgroup v2 `memory.max`, or cgroup v1
//! `memory.limit_in_bytes`), capped by physical memory. A container therefore
//! sizes its defaults from its own limit rather than from the host's RAM. The
//! ceiling is read once per process; configuration that must not depend on
//! the host sets its budgets explicitly instead.

use std::path::Path;
use std::sync::OnceLock;

/// Returns the memory this process may use, in bytes.
///
/// Returns `None` only when neither a cgroup limit nor the physical memory
/// size is readable, for example on a platform without `sysconf`.
pub(crate) fn memory_ceiling_bytes() -> Option<u64> {
    static CEILING: OnceLock<Option<u64>> = OnceLock::new();
    *CEILING.get_or_init(|| {
        let cgroup = std::fs::read_to_string("/proc/self/cgroup")
            .ok()
            .and_then(|membership| cgroup_limit_bytes(&membership, Path::new("/sys/fs/cgroup")));
        cgroup.into_iter().chain(physical_memory_bytes()).min()
    })
}

/// Returns the tightest memory limit over the process's cgroup hierarchies.
///
/// `membership` is the content of `/proc/self/cgroup` and `root` the cgroup
/// filesystem mount. Each line is `id:controllers:path`. The unified v2
/// hierarchy lists no controllers and exposes `memory.max`; a v1 hierarchy
/// listing the `memory` controller is mounted at `root/memory` and exposes
/// `memory.limit_in_bytes`. A limit on any ancestor also binds the process,
/// and a container whose cgroup namespace or bind mount hides its path sees
/// its own limit at the mount root, so every cgroup from the path up to the
/// root is read. Unlimited (`max`), missing, and malformed values are skipped.
fn cgroup_limit_bytes(membership: &str, root: &Path) -> Option<u64> {
    membership
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, ':');
            let (_, controllers, path) = (fields.next()?, fields.next()?, fields.next()?);
            let (mount, limit_file) = match controllers {
                "" => (root.to_path_buf(), "memory.max"),
                listed if listed.split(',').any(|controller| controller == "memory") => {
                    (root.join("memory"), "memory.limit_in_bytes")
                }
                _ => return None,
            };
            Path::new(path.trim_start_matches('/'))
                .ancestors()
                .filter_map(|cgroup| {
                    std::fs::read_to_string(mount.join(cgroup).join(limit_file))
                        .ok()?
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
                .min()
        })
        .min()
}

/// Returns the host's physical memory, in bytes.
#[cfg(unix)]
fn physical_memory_bytes() -> Option<u64> {
    // SAFETY: `sysconf` only reads system configuration values and has no
    // memory-safety preconditions; failures are reported as -1.
    let (pages, page_size) = unsafe {
        (
            libc::sysconf(libc::_SC_PHYS_PAGES),
            libc::sysconf(libc::_SC_PAGESIZE),
        )
    };
    u64::try_from(pages)
        .ok()?
        .checked_mul(u64::try_from(page_size).ok()?)
        .filter(|bytes| *bytes > 0)
}

/// Returns the host's physical memory, which this platform cannot report.
#[cfg(not(unix))]
fn physical_memory_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes one cgroup limit file below `root`.
    fn limit(root: &Path, cgroup: &str, file: &str, value: &str) {
        let directory = root.join(cgroup);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(file), value).unwrap();
    }

    #[test]
    fn cgroup_v2_limit_is_the_tightest_ancestor() {
        let root = tempfile::tempdir().unwrap();
        limit(root.path(), "", "memory.max", "max\n");
        limit(root.path(), "kubepods", "memory.max", "1073741824\n");
        limit(root.path(), "kubepods/pod", "memory.max", "max\n");
        limit(
            root.path(),
            "kubepods/pod/container",
            "memory.max",
            "2147483648\n",
        );

        assert_eq!(
            cgroup_limit_bytes("0::/kubepods/pod/container\n", root.path()),
            Some(1 << 30)
        );
        assert_eq!(cgroup_limit_bytes("0::/\n", root.path()), None);
        assert_eq!(cgroup_limit_bytes("0::/unknown/path\n", root.path()), None);
    }

    #[test]
    fn namespaced_cgroup_reads_its_limit_at_the_mount_root() {
        let root = tempfile::tempdir().unwrap();
        limit(root.path(), "", "memory.max", "536870912\n");
        assert_eq!(cgroup_limit_bytes("0::/\n", root.path()), Some(512 << 20));

        // A v1 container sees the host's cgroup path but its own limit at the mount.
        let v1 = tempfile::tempdir().unwrap();
        limit(v1.path(), "memory", "memory.limit_in_bytes", "268435456\n");
        assert_eq!(
            cgroup_limit_bytes(
                "5:cpu,cpuacct:/docker/abc\n4:cpuset,memory:/docker/abc\n",
                v1.path()
            ),
            Some(256 << 20)
        );
    }

    #[test]
    fn unrelated_or_malformed_hierarchies_have_no_limit() {
        let root = tempfile::tempdir().unwrap();
        limit(
            root.path(),
            "memory",
            "memory.limit_in_bytes",
            "not-a-number\n",
        );
        limit(root.path(), "cpu", "memory.limit_in_bytes", "1024\n");

        assert_eq!(cgroup_limit_bytes("", root.path()), None);
        assert_eq!(cgroup_limit_bytes("malformed\n", root.path()), None);
        assert_eq!(cgroup_limit_bytes("3:cpu:/\n", root.path()), None);
        assert_eq!(cgroup_limit_bytes("4:memory:/\n", root.path()), None);
    }

    #[test]
    fn process_memory_ceiling_is_positive_and_stable() {
        let ceiling = memory_ceiling_bytes();
        #[cfg(unix)]
        assert!(ceiling.is_some_and(|bytes| bytes > 0));
        assert_eq!(memory_ceiling_bytes(), ceiling);
        assert!(physical_memory_bytes().is_none_or(|physical| ceiling <= Some(physical)));
    }
}
