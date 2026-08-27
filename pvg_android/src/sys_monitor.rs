use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
struct ThreadSnapshot {
    name: String,
    utime: u64,
    stime: u64,
    voluntary_switches: u64,
    involuntary_switches: u64,
}

pub struct SystemMonitor {
    last_sample_time: Instant,
    last_snapshots: HashMap<u32, ThreadSnapshot>,
    ticks_per_sec: f64,
}

impl SystemMonitor {
    pub fn new() -> Self {
        Self {
            last_sample_time: Instant::now(),
            last_snapshots: HashMap::new(),
            ticks_per_sec: 100.0, // POSIX clock ticks on Linux/Android
        }
    }

    /// Reads /proc/self/task to log thread-level CPU usage breakdown
    pub fn log_1s_thread_profiler(&mut self) {
        let elapsed = self.last_sample_time.elapsed().as_secs_f64();
        if elapsed < 1.0 {
            return;
        }

        let task_dir = Path::new("/proc/self/task");
        let entries = match fs::read_dir(task_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut current_snapshots = HashMap::new();
        let mut report_lines = Vec::new();

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let tid_str = file_name.to_string_lossy();
            let tid: u32 = match tid_str.parse() {
                Ok(t) => t,
                Err(_) => continue,
            };

            let path = entry.path();
            let comm = fs::read_to_string(path.join("comm"))
                .unwrap_or_else(|_| "unknown".into())
                .trim()
                .to_string();

            let stat_content = fs::read_to_string(path.join("stat")).unwrap_or_default();
            let (utime, stime) = Self::parse_stat_times(&stat_content);

            let (vol_sw, invol_sw) = Self::parse_status_switches(&fs::read_to_string(path.join("status")).unwrap_or_default());

            let current = ThreadSnapshot {
                name: comm,
                utime,
                stime,
                voluntary_switches: vol_sw,
                involuntary_switches: invol_sw,
            };

            if let Some(prev) = self.last_snapshots.get(&tid) {
                let d_utime = current.utime.saturating_sub(prev.utime) as f64 / self.ticks_per_sec;
                let d_stime = current.stime.saturating_sub(prev.stime) as f64 / self.ticks_per_sec;

                let u_pct = (d_utime / elapsed) * 100.0;
                let s_pct = (d_stime / elapsed) * 100.0;
                let total_pct = u_pct + s_pct;

                let d_vol = current.voluntary_switches.saturating_sub(prev.voluntary_switches);
                let d_invol = current.involuntary_switches.saturating_sub(prev.involuntary_switches);

                if total_pct >= 0.5 || d_vol > 50 || d_invol > 10 {
                    report_lines.push((
                        total_pct,
                        format!(
                            "   │ TID {:<5} {:<20} │ User: {:>4.1}% │ Sys: {:>4.1}% │ Total: {:>5.1}% │ VolCtx: {:>4}/s │ InvolCtx: {:>3}/s",
                            tid, current.name, u_pct, s_pct, total_pct, d_vol, d_invol
                        ),
                    ));
                }
            }

            current_snapshots.insert(tid, current);
        }

        self.last_snapshots = current_snapshots;
        self.last_sample_time = Instant::now();

        report_lines.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        crate::log_info!("┌────────────────────────────────────────────────────────────────────────────────────────────────────────┐");
        crate::log_info!("│ 🔍 [KERNEL /proc THREAD-LEVEL CPU PROFILER (1s)]                                                       │");
        crate::log_info!("├────────────────────────────────────────────────────────────────────────────────────────────────────────┤");
        if report_lines.is_empty() {
            crate::log_info!("   │ All threads idle (< 0.5% CPU)");
        } else {
            for (_, line) in report_lines {
                crate::log_info!("{}", line);
            }
        }
        crate::log_info!("└────────────────────────────────────────────────────────────────────────────────────────────────────────┘");
    }

    fn parse_stat_times(stat: &str) -> (u64, u64) {
        if let Some(idx) = stat.rfind(')') {
            let rest = &stat[idx + 1..];
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 13 {
                let utime: u64 = parts[11].parse().unwrap_or(0);
                let stime: u64 = parts[12].parse().unwrap_or(0);
                return (utime, stime);
            }
        }
        (0, 0)
    }

    fn parse_status_switches(status: &str) -> (u64, u64) {
        let mut vol = 0;
        let mut invol = 0;
        for line in status.lines() {
            if line.starts_with("voluntary_ctxt_switches:") {
                vol = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            } else if line.starts_with("nonvoluntary_ctxt_switches:") {
                invol = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            }
        }
        (vol, invol)
    }
}