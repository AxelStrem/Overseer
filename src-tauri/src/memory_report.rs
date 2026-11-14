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

/// What the container holds as its cgroup counts it: in all, and how much of that is memory the
/// programs asked for against the file cache, which the kernel takes back when it needs the room.
fn container() -> Option<String> {
    let read = |name: &str| std::fs::read_to_string(format!("/sys/fs/cgroup/{}", name)).ok();
    let stat_field = |stat: &str, field: &str| {
        stat.lines()
            .find_map(|line| line.strip_prefix(field)?.strip_prefix(' '))
            .and_then(|n| n.trim().parse::<usize>().ok())
    };
    // cgroup v2, then v1.
    let (current, stat, anon, file) = match read("memory.current") {
        Some(current) => (current, read("memory.stat")?, "anon", "file"),
        None => (read("memory/memory.usage_in_bytes")?, read("memory/memory.stat")?, "total_rss", "total_cache"),
    };
    let current = current.trim().parse::<usize>().ok()?;
    let mut said = format!("container {} MB", megabytes(current));
    if let (Some(anon), Some(file)) = (stat_field(&stat, anon), stat_field(&stat, file)) {
        said.push_str(&format!(" (anon {}, file {})", megabytes(anon), megabytes(file)));
    }
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
}
