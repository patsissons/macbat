//! Per-process energy: samples each process's cumulative energy counter and turns the deltas
//! into current power and running totals, per process and per app.
//!
//! Only the user's own processes are readable without root, and the counter covers CPU energy
//! only, so the sum is always well below total system power.

use crate::libproc;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// One process's energy use since the previous tick.
pub struct ProcDelta {
    pub pid: i32,
    /// Start time; distinguishes a reused pid from the process that had it before.
    pub start: u64,
    pub name: Arc<str>,
    /// The app the process belongs to, or its own name when it isn't inside an app bundle.
    pub group: Arc<str>,
    pub delta_nj: u64,
    pub lifetime_nj: u64,
}

#[derive(Default)]
pub struct ProcTick {
    pub readable: usize,
    /// Processes owned by root or other users, which can't be read without root.
    pub unreadable: usize,
    pub procs: Vec<ProcDelta>,
}

/// A process entry in `pipe --procs` output.
#[derive(Debug, Clone, Serialize)]
pub struct ProcSample {
    pub pid: i32,
    pub name: String,
    pub group: String,
    /// Average power over the last interval, W.
    pub watts: f64,
    /// Energy used since the process started, J.
    pub energy_j: f64,
}

impl ProcTick {
    /// Processes that used energy over the last `dt` seconds, highest power first.
    pub fn samples(&self, dt: f64) -> Vec<ProcSample> {
        let mut out: Vec<ProcSample> = self
            .procs
            .iter()
            .filter(|p| p.delta_nj > 0 && dt > 0.0)
            .map(|p| ProcSample {
                pid: p.pid,
                name: p.name.to_string(),
                group: p.group.to_string(),
                watts: p.delta_nj as f64 / 1e9 / dt,
                energy_j: p.lifetime_nj as f64 / 1e9,
            })
            .collect();
        out.sort_by(|a, b| b.watts.total_cmp(&a.watts));
        out
    }
}

struct Seen {
    start: u64,
    energy_nj: u64,
    name: Arc<str>,
    group: Arc<str>,
}

#[derive(Default)]
pub struct ProcSampler {
    seen: HashMap<i32, Seen>,
    primed: bool,
}

impl ProcSampler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sample(&mut self) -> ProcTick {
        let mut tick = ProcTick::default();
        let mut next = HashMap::with_capacity(self.seen.len());
        for pid in libproc::pids() {
            let Some(ru) = libproc::rusage(pid) else {
                tick.unreadable += 1;
                continue;
            };
            tick.readable += 1;
            let (start, energy_nj) = (ru.proc_start_abstime, ru.energy_nj);
            let (name, group, delta_nj) = match self.seen.remove(&pid) {
                Some(p) if p.start == start => (p.name, p.group, energy_nj.saturating_sub(p.energy_nj)),
                // New since the last tick, so everything it has used was used while watching.
                // Processes in the very first snapshot have history from before, which isn't.
                _ => {
                    let (name, group) = identify(pid);
                    (name, group, if self.primed { energy_nj } else { 0 })
                }
            };
            tick.procs.push(ProcDelta {
                pid,
                start,
                name: name.clone(),
                group: group.clone(),
                delta_nj,
                lifetime_nj: energy_nj,
            });
            next.insert(pid, Seen { start, energy_nj, name, group });
        }
        self.seen = next;
        self.primed = true;
        tick
    }
}

fn identify(pid: i32) -> (Arc<str>, Arc<str>) {
    let path = libproc::path(pid);
    let mut name = libproc::name(pid);
    if name.is_empty() {
        name = match path.as_deref().and_then(|p| p.rsplit('/').next()) {
            Some(base) if !base.is_empty() => base.to_string(),
            _ => format!("pid {pid}"),
        };
    }
    let name: Arc<str> = name.into();
    let group = path.as_deref().and_then(app_name).map(Arc::from).unwrap_or_else(|| name.clone());
    (name, group)
}

/// The outermost app bundle in `path`, so helpers nested in an app roll up into it:
/// `/Applications/Foo.app/Contents/Frameworks/Foo Helper.app/…` → `Foo`.
fn app_name(path: &str) -> Option<&str> {
    let end = path.find(".app/")?;
    let name = path[..end].rsplit('/').next()?;
    (!name.is_empty()).then_some(name)
}

/// Running numbers for one row of the process table: a process, or an app's processes combined.
pub struct Usage {
    pub name: Arc<str>,
    /// Set for per-process rows.
    pub pid: Option<i32>,
    /// Live processes behind this row.
    pub count: usize,
    pub now_w: f64,
    pub peak_w: f64,
    /// Energy used since macbat started watching, J.
    pub total_j: f64,
    start: u64,
    live: bool,
}

impl Usage {
    pub fn new(name: Arc<str>, pid: Option<i32>, start: u64) -> Self {
        Self { name, pid, count: 0, now_w: 0.0, peak_w: 0.0, total_j: 0.0, start, live: false }
    }
}

#[derive(Default)]
pub struct ProcStats {
    groups: HashMap<Arc<str>, Usage>,
    pids: HashMap<i32, Usage>,
    pub readable: usize,
    pub unreadable: usize,
    /// Seconds covered by the totals.
    pub elapsed_s: f64,
}

impl ProcStats {
    /// Folds in one tick covering the last `dt` seconds.
    pub fn apply(&mut self, tick: &ProcTick, dt: f64) {
        self.readable = tick.readable;
        self.unreadable = tick.unreadable;
        self.elapsed_s += dt;

        for u in self.groups.values_mut().chain(self.pids.values_mut()) {
            u.now_w = 0.0;
            u.count = 0;
            u.live = false;
        }
        for p in &tick.procs {
            let joules = p.delta_nj as f64 / 1e9;
            let watts = if dt > 0.0 { joules / dt } else { 0.0 };

            let g = self.groups.entry(p.group.clone()).or_insert_with(|| Usage::new(p.group.clone(), None, 0));
            g.now_w += watts;
            g.total_j += joules;
            g.count += 1;

            let u = self.pids.entry(p.pid).or_insert_with(|| Usage::new(p.name.clone(), Some(p.pid), p.start));
            if u.start != p.start {
                *u = Usage::new(p.name.clone(), Some(p.pid), p.start);
            }
            u.now_w = watts;
            u.total_j += joules;
            u.count = 1;
            u.live = true;
        }
        // Per-process rows go when the process does; an app keeps what its processes used.
        self.pids.retain(|_, u| u.live);
        self.groups.retain(|_, u| u.count > 0 || u.total_j > 0.0);
        for u in self.groups.values_mut().chain(self.pids.values_mut()) {
            u.peak_w = u.peak_w.max(u.now_w);
        }
    }

    pub fn rows(&self, by_app: bool) -> Vec<&Usage> {
        if by_app { self.groups.values().collect() } else { self.pids.values().collect() }
    }

    /// Power currently attributed to readable processes, W.
    pub fn now_w(&self) -> f64 {
        self.groups.values().map(|u| u.now_w).sum()
    }

    /// Energy attributed to readable processes since macbat started, J.
    pub fn total_j(&self) -> f64 {
        self.groups.values().map(|u| u.total_j).sum()
    }
}
