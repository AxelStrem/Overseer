//! One line saying where the server's memory is: what the process holds, what the container it
//! runs in holds and who else in it holds that, and what each of the server's caches is keeping.
//!
//! A host's memory graph is one number for the whole container - the server, the bot beside it,
//! git, and on some hosts the file cache as well - and that number alone cannot say which of them
//! to look at. This is printed to the server's log, where the host shows it, so the answer is
//! there without an endpoint to secure or a shell to open.
//!
//! What it reads of the system is Linux's: `/proc` and the cgroup's files. Anywhere a file is not
//! there, the part of the line it would give is left out.

fn megabytes(bytes: usize) -> usize {
    (bytes + 512 * 1024) / (1024 * 1024)
}

/// A field of a `/proc/<pid>/status` file, in bytes - the file says kilobytes.
fn status_field(status: &str, field: &str) -> Option<usize> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|rest| rest.split_whitespace().next()?.parse::<usize>().ok())
        .map(|kb| kb * 1024)
}

fn status_name(status: &str) -> Option<String> {
    status.lines().find_map(|line| line.strip_prefix("Name:")).map(|name| name.trim().to_string())
}

/// The process's own resident memory, and the most it has held.
fn own() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let rss = status_field(&status, "VmRSS")?;
    let peak = status_field(&status, "VmHWM").map_or(String::new(), |hwm| format!(" (peak {})", megabytes(hwm)));
    Some(format!("rss {} MB{}", megabytes(rss), peak))
}

/// Below this a part of the container's memory is not named, so the line stays short.
const SMALL_PART: usize = 4 * 1024 * 1024;

/// What the container holds as its cgroup counts it: in all, and what that is made of.
fn container() -> Option<String> {
    let read = |name: &str| std::fs::read_to_string(format!("/sys/fs/cgroup/{}", name)).ok();
    // cgroup v2, then v1.
    match read("memory.current") {
        Some(current) => container_line(&current, &read("memory.stat")?, true),
        None => container_line(&read("memory/memory.usage_in_bytes")?, &read("memory/memory.stat")?, false),
    }
}

/// The line from the cgroup's total and its `memory.stat`. Under v1 it says only the programs'
/// own memory against the file cache. Under v2 it says the parts the total is made of - `anon`,
/// `file`, `kernel`, `sock`, and what is left of the total besides them - so that each part of
/// the figure a host bills has a name. `file` is told apart into what the kernel takes back
/// cheaply (`inactive`) and what it would rather keep, and `kernel` into its slab, mostly the
/// kernel's records of files and folders, and how much of that it can take back.
fn container_line(current: &str, stat: &str, v2: bool) -> Option<String> {
    let current = current.trim().parse::<usize>().ok()?;
    let fields: std::collections::HashMap<&str, usize> = stat
        .lines()
        .filter_map(|line| {
            let (name, n) = line.split_once(' ')?;
            Some((name, n.trim().parse::<usize>().ok()?))
        })
        .collect();
    let field = |name: &str| fields.get(name).copied();
    let mut said = format!("container {} MB", megabytes(current));
    let (anon, file) = if v2 { ("anon", "file") } else { ("total_rss", "total_cache") };
    let (Some(anon), Some(file)) = (field(anon), field(file)) else { return Some(said) };
    if !v2 {
        said.push_str(&format!(" (anon {}, file {})", megabytes(anon), megabytes(file)));
        return Some(said);
    }

    // Each part with what it is made of, those too small to matter left out.
    let part = |name: &str, bytes: usize, within: &[(&str, usize)]| {
        let within: Vec<String> = within
            .iter()
            .filter(|(_, bytes)| *bytes >= SMALL_PART)
            .map(|(name, bytes)| format!("{} {}", name, megabytes(*bytes)))
            .collect();
        let within = if within.is_empty() { String::new() } else { format!(" [{}]", within.join(", ")) };
        format!("{} {}{}", name, megabytes(bytes), within)
    };
    let get = |name: &str| field(name).unwrap_or(0);
    // `kernel` is there from Linux 5.18; before it, its parts are counted one by one.
    let kernel = field("kernel").unwrap_or_else(|| {
        ["slab", "kernel_stack", "pagetables", "percpu", "vmalloc"].iter().map(|name| get(name)).sum()
    });
    let sock = get("sock");
    let other = current.saturating_sub(anon + file + kernel + sock);

    // `shmem` and the dirty pages are counted within `file` already, and so within the total.
    let mut parts = vec![
        format!("anon {}", megabytes(anon)),
        part(
            "file",
            file,
            &[
                ("inactive", get("inactive_file")),
                ("active", get("active_file")),
                ("dirty", get("file_dirty") + get("file_writeback")),
                ("shmem", get("shmem")),
            ],
        ),
    ];
    if kernel >= SMALL_PART {
        parts.push(part(
            "kernel",
            kernel,
            &[
                ("slab", get("slab")),
                ("reclaimable", get("slab_reclaimable")),
                ("pagetables", get("pagetables")),
                ("stack", get("kernel_stack")),
                ("percpu", get("percpu")),
            ],
        ));
    }
    for (name, bytes) in [("sock", sock), ("other", other)] {
        if bytes >= SMALL_PART {
            parts.push(format!("{} {}", name, megabytes(bytes)));
        }
    }
    said.push_str(&format!(" ({})", parts.join(", ")));
    Some(said)
}

/// The resident memory of the other processes it can see, added up by name, largest first.
fn others() -> Option<String> {
    let me = std::process::id().to_string();
    let mut by_name: Vec<(String, usize)> = Vec::new();
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let pid = entry.file_name().to_string_lossy().to_string();
        if pid == me || !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else { continue };
        // Kernel threads have no resident memory line, and are not what is being looked for.
        let (Some(name), Some(rss)) = (status_name(&status), status_field(&status, "VmRSS")) else { continue };
        match by_name.iter_mut().find(|(held, _)| *held == name) {
            Some((_, total)) => *total += rss,
            None => by_name.push((name, rss)),
        }
    }
    by_name.sort_by(|a, b| b.1.cmp(&a.1));
    let listed: Vec<String> =
        by_name.iter().take(6).map(|(name, rss)| format!("{} {}", name, megabytes(*rss))).collect();
    Some(format!("others: {}", if listed.is_empty() { "none".to_string() } else { listed.join(", ") }))
}

/// What each cache is keeping, in megabytes, and how many things.
fn caches() -> String {
    let (documents, held) = crate::document_cache::held();
    let side = megabytes(crate::bounded::limit());
    let part = |name: &str, (count, bytes): (usize, usize)| format!("{} {} MB in {}", name, megabytes(bytes), count);
    [
        format!(
            "documents {}/{} MB in {}",
            megabytes(held),
            megabytes(crate::document_cache::budget_bytes()),
            documents
        ),
        format!("{} (each of the next three up to {} MB)", part("parts", crate::parts::read_parts_held()), side),
        part("mounted", crate::actions::mounted_held()),
        part("bases", crate::bases::parsed_held()),
        part("superseded", crate::document_cache::superseded_held()),
        part("handed out", crate::app_api::handed_out_held()),
    ]
    .join(", ")
}

/// The whole line, without a prefix.
pub fn line() -> String {
    let mut parts: Vec<String> = [own(), container(), others()].into_iter().flatten().collect();
    parts.push(caches());
    parts.join(" | ")
}

/// How often the line is printed, from `OVERSEER_MEMORY_LOG` in minutes - `0` for never - or
/// every fifteen minutes when it says nothing.
pub fn every() -> Option<std::time::Duration> {
    let minutes = std::env::var("OVERSEER_MEMORY_LOG")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(15);
    (minutes > 0).then(|| std::time::Duration::from_secs(minutes * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_field_is_read_in_bytes() {
        let status = "Name:\toverseer-server\nVmHWM:\t  470000 kB\nVmRSS:\t  300000 kB\n";
        assert_eq!(status_field(status, "VmRSS"), Some(300000 * 1024));
        assert_eq!(status_field(status, "VmHWM"), Some(470000 * 1024));
        assert_eq!(status_name(status).as_deref(), Some("overseer-server"));
        assert_eq!(status_field(status, "VmSwap"), None);
    }

    const MB: usize = 1024 * 1024;

    /// A v2 `memory.stat` captured from a container, trimmed to the fields the line reads.
    const STAT_V2: &str = "anon 223293440\nfile 2234544128\nkernel 134746112\nkernel_stack 507904\n\
pagetables 2154496\npercpu 5160\nsock 0\nvmalloc 16384\nshmem 0\nfile_mapped 108990464\n\
file_dirty 348160\nfile_writeback 0\ninactive_anon 223543296\nactive_anon 36864\n\
inactive_file 944623616\nactive_file 1289863168\nslab_reclaimable 130831592\n\
slab_unreclaimable 1184912\nslab 132016504\n";

    #[test]
    fn a_v2_container_is_told_in_parts_that_add_up_to_it() {
        let line = container_line("2595667968\n", STAT_V2, true).unwrap();
        assert_eq!(
            line,
            "container 2475 MB (anon 213, file 2131 [inactive 901, active 1230], \
             kernel 129 [slab 126, reclaimable 125])"
        );
        let total = 2595667968usize;
        let named = 223293440 + 2234544128 + 134746112;
        assert!(total - named < SMALL_PART);
    }

    #[test]
    fn what_the_named_parts_leave_of_the_total_is_shown_when_large() {
        let line = container_line(&(1606 * MB).to_string(), "anon 0\nfile 0\nkernel 0\nsock 6291456\n", true).unwrap();
        assert_eq!(line, "container 1606 MB (anon 0, file 0, sock 6, other 1600)");
    }

    #[test]
    fn before_the_kernel_field_its_parts_are_added_up() {
        let stat = format!("anon {}\nfile {}\nslab {}\nslab_reclaimable {}\nkernel_stack {}\npagetables {}\n", 100 * MB, 50 * MB, 300 * MB, 290 * MB, 2 * MB, 10 * MB);
        let line = container_line(&(462 * MB).to_string(), &stat, true).unwrap();
        assert_eq!(line, "container 462 MB (anon 100, file 50, kernel 312 [slab 300, reclaimable 290, pagetables 10])");
    }

    #[test]
    fn dirty_and_shared_pages_are_named_within_the_file_cache() {
        let stat = format!("anon 0\nfile {}\nkernel 0\ninactive_file {}\nactive_file {}\nfile_dirty {}\nfile_writeback {}\nshmem {}\n", 100 * MB, 40 * MB, 60 * MB, 3 * MB, 2 * MB, 20 * MB);
        let line = container_line(&(100 * MB).to_string(), &stat, true).unwrap();
        assert_eq!(line, "container 100 MB (anon 0, file 100 [inactive 40, active 60, dirty 5, shmem 20])");
    }

    #[test]
    fn a_v1_container_reads_as_programs_against_file_cache() {
        let stat = format!("cache {}\nrss {}\ntotal_cache {}\ntotal_rss {}\n", MB, MB, 479 * MB, 774 * MB);
        let line = container_line(&(1606 * MB).to_string(), &stat, false).unwrap();
        assert_eq!(line, "container 1606 MB (anon 774, file 479)");
    }
}
