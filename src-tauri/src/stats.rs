//! System stats for the overlay's System tab and the HUD: CPU and memory (sysinfo), GPU load and
//! video memory (Windows' own performance counters, the ones Task Manager shows), and network
//! speed. Nothing runs in the background: each call reads the counters once.

use serde::Serialize;
use std::{sync::Mutex, time::Instant};

#[derive(Serialize, Default)]
pub struct Snapshot {
    /// Percent.
    cpu: f32,
    /// Bytes.
    ram_used: u64,
    ram_total: u64,
    /// Percent (the busiest 3D engine use, summed across apps, at most 100).
    gpu: Option<f32>,
    /// Bytes of dedicated video memory in use.
    vram_used: Option<u64>,
    /// Bytes per second.
    net_down: u64,
    net_up: u64,
}

#[derive(Default)]
pub struct Stats(Mutex<Option<Inner>>);

struct Inner {
    sys: sysinfo::System,
    nets: sysinfo::Networks,
    last_net: Option<(Instant, u64, u64)>,
    #[cfg(windows)]
    gpu: Option<pdh::Query>,
}

impl Stats {
    /// CPU and memory percent, for the HUD.
    pub fn basic(&self) -> (f32, f32) {
        let s = self.read();
        let ram = if s.ram_total > 0 { s.ram_used as f32 / s.ram_total as f32 * 100.0 } else { 0.0 };
        (s.cpu, ram)
    }

    pub fn read(&self) -> Snapshot {
        let mut g = self.0.lock().unwrap();
        let i = g.get_or_insert_with(|| Inner {
            sys: sysinfo::System::new(),
            nets: sysinfo::Networks::new_with_refreshed_list(),
            last_net: None,
            #[cfg(windows)]
            gpu: pdh::Query::open(),
        });
        i.sys.refresh_cpu_usage();
        i.sys.refresh_memory();
        i.nets.refresh(true);
        let (rx, tx) = i.nets.iter().fold((0u64, 0u64), |(r, t), (_, n)| (r + n.total_received(), t + n.total_transmitted()));
        let now = Instant::now();
        let (net_down, net_up) = match i.last_net {
            Some((at, r0, t0)) => {
                let secs = now.duration_since(at).as_secs_f64().max(0.001);
                ((rx.saturating_sub(r0) as f64 / secs) as u64, (tx.saturating_sub(t0) as f64 / secs) as u64)
            }
            None => (0, 0),
        };
        i.last_net = Some((now, rx, tx));
        #[cfg(windows)]
        let (gpu, vram_used) = i.gpu.as_mut().map_or((None, None), |q| q.read());
        #[cfg(not(windows))]
        let (gpu, vram_used) = (None, None);
        Snapshot { cpu: i.sys.global_cpu_usage(), ram_used: i.sys.used_memory(), ram_total: i.sys.total_memory(), gpu, vram_used, net_down, net_up }
    }
}

/// Windows performance counters (PDH): GPU engine load and adapter memory.
#[cfg(windows)]
mod pdh {
    use windows_sys::Win32::System::Performance::*;

    pub struct Query {
        q: isize,
        engine: isize,
        memory: isize,
    }
    // The query handle is only used under the Stats mutex.
    unsafe impl Send for Query {}

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    impl Query {
        pub fn open() -> Option<Query> {
            unsafe {
                let mut q = 0;
                if PdhOpenQueryW(std::ptr::null(), 0, &mut q) != 0 {
                    return None;
                }
                let (mut engine, mut memory) = (0, 0);
                let ok = PdhAddEnglishCounterW(q, wide(r"\GPU Engine(*engtype_3D)\Utilization Percentage").as_ptr(), 0, &mut engine) == 0;
                let _ = PdhAddEnglishCounterW(q, wide(r"\GPU Adapter Memory(*)\Dedicated Usage").as_ptr(), 0, &mut memory);
                if !ok {
                    PdhCloseQuery(q);
                    return None;
                }
                PdhCollectQueryData(q); // rates need two samples; the first read primes it
                Some(Query { q, engine, memory })
            }
        }

        /// Sum of every instance of a counter.
        fn sum(&self, counter: isize) -> Option<f64> {
            if counter == 0 {
                return None;
            }
            unsafe {
                let (mut size, mut count) = (0u32, 0u32);
                PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
                if size == 0 {
                    return None;
                }
                let mut buf = vec![0u8; size as usize];
                let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
                    return None;
                }
                Some((0..count as usize).map(|k| (*items.add(k)).FmtValue.Anonymous.doubleValue).filter(|v| v.is_finite()).sum())
            }
        }

        pub fn read(&mut self) -> (Option<f32>, Option<u64>) {
            unsafe {
                if PdhCollectQueryData(self.q) != 0 {
                    return (None, None);
                }
            }
            (self.sum(self.engine).map(|v| v.min(100.0) as f32), self.sum(self.memory).map(|v| v as u64))
        }
    }

    impl Drop for Query {
        fn drop(&mut self) {
            unsafe { PdhCloseQuery(self.q) };
        }
    }
}
