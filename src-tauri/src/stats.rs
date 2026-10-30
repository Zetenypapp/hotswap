//! System stats for the overlay (System tab, HUD, stats widget). Nothing runs in the background:
//! each call reads everything once. Sources, all already on Windows (no extra libraries):
//! - CPU, memory, network and the running game's CPU and memory: sysinfo.
//! - GPU load, video memory, disk speed and the game's GPU load: Windows' performance counters
//!   (the ones Task Manager shows).
//! - GPU temperature, power, clock and fan: the graphics driver's own library, loaded if present:
//!   NVIDIA's nvml.dll or AMD's atiadlxx.dll.
//! - CPU temperature and power: Windows can't read these without a kernel driver, so they come
//!   from LibreHardwareMonitor's local web server when it's running.

use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Serialize, Default, Clone)]
pub struct Game {
    /// Percent of the whole CPU.
    cpu: f32,
    /// Bytes.
    ram: u64,
    /// Percent.
    gpu: Option<f32>,
}

#[derive(Serialize, Default)]
pub struct Snapshot {
    cpu: f32,
    cpu_temp: Option<f32>,
    cpu_power: Option<f32>,
    ram_used: u64,
    ram_total: u64,
    gpu_name: Option<String>,
    gpu: Option<f32>,
    gpu_temp: Option<f32>,
    gpu_hotspot: Option<f32>,
    gpu_power: Option<f32>,
    gpu_clock: Option<f32>,
    gpu_fan: Option<f32>,
    vram_used: Option<u64>,
    vram_total: Option<u64>,
    disk_read: Option<u64>,
    disk_write: Option<u64>,
    net_down: u64,
    net_up: u64,
    game: Option<Game>,
    /// Where CPU temperature and power came from, or why they're missing.
    cpu_sensors: &'static str,
}

#[derive(Default)]
pub struct Stats(Mutex<Option<Inner>>);

struct Inner {
    sys: sysinfo::System,
    nets: sysinfo::Networks,
    last_net: Option<(Instant, u64, u64)>,
    #[cfg(windows)]
    pdh: Option<pdh::Query>,
    #[cfg(windows)]
    gpu: Option<gpu::Driver>,
    lhm: Lhm,
}

/// LibreHardwareMonitor's web server (Options > Remote Web Server), polled for CPU sensors.
#[derive(Default)]
struct Lhm {
    retry_at: Option<Instant>,
}

impl Lhm {
    fn read(&mut self) -> (Option<f32>, Option<f32>, &'static str) {
        if self.retry_at.is_some_and(|t| Instant::now() < t) {
            return (None, None, "lhm-off");
        }
        let v: Option<serde_json::Value> = ureq::get("http://127.0.0.1:8085/data.json").timeout(Duration::from_millis(250)).call().ok().and_then(|r| r.into_json().ok());
        let Some(v) = v else {
            self.retry_at = Some(Instant::now() + Duration::from_secs(15));
            return (None, None, "lhm-off");
        };
        self.retry_at = None;
        let (mut temp, mut power) = (None, None);
        fn walk(n: &serde_json::Value, temp: &mut Option<f32>, power: &mut Option<f32>) {
            let id = n["SensorId"].as_str().unwrap_or("").to_lowercase();
            let text = n["Text"].as_str().unwrap_or("").to_lowercase();
            let num = n["Value"].as_str().and_then(|s| s.split_whitespace().next()).and_then(|s| s.replace(',', ".").parse::<f32>().ok());
            if id.contains("cpu") && id.contains("/temperature/") && (text.contains("package") || text.contains("tctl") || text.contains("tdie")) && temp.is_none() {
                *temp = num;
            }
            if id.contains("cpu") && id.contains("/power/") && text.contains("package") && power.is_none() {
                *power = num;
            }
            if let Some(c) = n["Children"].as_array() {
                for k in c {
                    walk(k, temp, power);
                }
            }
        }
        walk(&v, &mut temp, &mut power);
        (temp, power, "lhm")
    }
}

impl Stats {
    /// CPU and memory percent, for the HUD's short form.
    pub fn basic(&self) -> (f32, f32) {
        let mut g = self.0.lock().unwrap();
        let i = g.get_or_insert_with(Inner::new);
        i.sys.refresh_cpu_usage();
        i.sys.refresh_memory();
        let ram = if i.sys.total_memory() > 0 { i.sys.used_memory() as f32 / i.sys.total_memory() as f32 * 100.0 } else { 0.0 };
        (i.sys.global_cpu_usage(), ram)
    }

    /// Everything, plus the game with process ID `game` when given.
    pub fn read(&self, game: Option<u32>) -> Snapshot {
        let mut g = self.0.lock().unwrap();
        let i = g.get_or_insert_with(Inner::new);
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
        let mut s = Snapshot { cpu: i.sys.global_cpu_usage(), ram_used: i.sys.used_memory(), ram_total: i.sys.total_memory(), net_down, net_up, ..Default::default() };
        // The game's own CPU and memory.
        if let Some(pid) = game {
            let p = sysinfo::Pid::from_u32(pid);
            i.sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[p]), true, sysinfo::ProcessRefreshKind::nothing().with_cpu().with_memory());
            if let Some(proc_) = i.sys.process(p) {
                let cores = i.sys.cpus().len().max(1) as f32;
                s.game = Some(Game { cpu: proc_.cpu_usage() / cores, ram: proc_.memory(), gpu: None });
            }
        }
        #[cfg(windows)]
        {
            if let Some(q) = i.pdh.as_mut() {
                let r = q.read(game);
                s.gpu = r.gpu;
                s.vram_used = r.vram;
                s.disk_read = r.disk_read;
                s.disk_write = r.disk_write;
                if let (Some(gm), Some(g)) = (s.game.as_mut(), r.game_gpu) {
                    gm.gpu = Some(g);
                }
            }
            if let Some(d) = i.gpu.as_mut() {
                let r = d.read();
                s.gpu_name = Some(d.name.clone());
                s.gpu_temp = r.temp;
                s.gpu_hotspot = r.hotspot;
                s.gpu_power = r.power;
                s.gpu_clock = r.clock;
                s.gpu_fan = r.fan;
                s.vram_total = r.vram_total;
                if s.gpu.is_none() {
                    s.gpu = r.load;
                }
            }
        }
        let (t, p, src) = i.lhm.read();
        s.cpu_temp = t;
        s.cpu_power = p;
        s.cpu_sensors = src;
        s
    }
}

impl Inner {
    fn new() -> Inner {
        Inner {
            sys: sysinfo::System::new(),
            nets: sysinfo::Networks::new_with_refreshed_list(),
            last_net: None,
            #[cfg(windows)]
            pdh: pdh::Query::open(),
            #[cfg(windows)]
            gpu: gpu::Driver::open(),
            lhm: Lhm::default(),
        }
    }
}

/// Dev aid: the AMD driver's sensors.
#[cfg(windows)]
pub fn gpu_sensors() -> Vec<(usize, i32)> {
    gpu::Driver::open().map(|d| d.dump()).unwrap_or_default()
}

/// Windows performance counters (PDH).
#[cfg(windows)]
mod pdh {
    use windows_sys::Win32::System::Performance::*;

    pub struct Query {
        q: isize,
        engine: isize,
        memory: isize,
        disk_read: isize,
        disk_write: isize,
    }
    // The handles are only used under the Stats mutex.
    unsafe impl Send for Query {}

    pub struct Reading {
        pub gpu: Option<f32>,
        pub game_gpu: Option<f32>,
        pub vram: Option<u64>,
        pub disk_read: Option<u64>,
        pub disk_write: Option<u64>,
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn add(q: isize, path: &str) -> isize {
        let mut c = 0;
        unsafe { PdhAddEnglishCounterW(q, wide(path).as_ptr(), 0, &mut c) };
        c
    }

    impl Query {
        pub fn open() -> Option<Query> {
            unsafe {
                let mut q = 0;
                if PdhOpenQueryW(std::ptr::null(), 0, &mut q) != 0 {
                    return None;
                }
                let me = Query {
                    q,
                    engine: add(q, r"\GPU Engine(*engtype_3D)\Utilization Percentage"),
                    memory: add(q, r"\GPU Adapter Memory(*)\Dedicated Usage"),
                    disk_read: add(q, r"\PhysicalDisk(_Total)\Disk Read Bytes/sec"),
                    disk_write: add(q, r"\PhysicalDisk(_Total)\Disk Write Bytes/sec"),
                };
                PdhCollectQueryData(q); // rates need two samples; the first read primes them
                Some(me)
            }
        }

        /// Every instance of a counter as (instance name, value).
        fn all(&self, counter: isize) -> Vec<(String, f64)> {
            if counter == 0 {
                return vec![];
            }
            unsafe {
                let (mut size, mut count) = (0u32, 0u32);
                PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
                if size == 0 {
                    return vec![];
                }
                let mut buf = vec![0u8; size as usize];
                let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
                    return vec![];
                }
                (0..count as usize)
                    .map(|k| {
                        let it = &*items.add(k);
                        let mut n = 0;
                        while *it.szName.add(n) != 0 {
                            n += 1;
                        }
                        (String::from_utf16_lossy(std::slice::from_raw_parts(it.szName, n)), it.FmtValue.Anonymous.doubleValue)
                    })
                    .filter(|(_, v)| v.is_finite())
                    .collect()
            }
        }

        fn one(&self, counter: isize) -> Option<u64> {
            self.all(counter).first().map(|(_, v)| *v as u64)
        }

        pub fn read(&mut self, game: Option<u32>) -> Reading {
            unsafe { PdhCollectQueryData(self.q) };
            let engines = self.all(self.engine);
            let gpu = (!engines.is_empty()).then(|| engines.iter().map(|e| e.1).sum::<f64>().min(100.0) as f32);
            // Engine instances are named "pid_1234_luid_..._engtype_3D".
            let game_gpu = game.map(|pid| {
                let tag = format!("pid_{pid}_");
                engines.iter().filter(|e| e.0.starts_with(&tag)).map(|e| e.1).sum::<f64>().min(100.0) as f32
            });
            let mem = self.all(self.memory);
            Reading {
                gpu,
                game_gpu,
                vram: (!mem.is_empty()).then(|| mem.iter().map(|m| m.1).sum::<f64>() as u64),
                disk_read: self.one(self.disk_read),
                disk_write: self.one(self.disk_write),
            }
        }
    }

    impl Drop for Query {
        fn drop(&mut self) {
            unsafe { PdhCloseQuery(self.q) };
        }
    }
}

/// GPU sensors from the graphics driver's own library: NVIDIA's NVML or AMD's ADL.
#[cfg(windows)]
mod gpu {
    use std::ffi::c_void;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    #[derive(Default)]
    pub struct Reading {
        pub load: Option<f32>,
        pub temp: Option<f32>,
        pub hotspot: Option<f32>,
        pub power: Option<f32>,
        pub clock: Option<f32>,
        pub fan: Option<f32>,
        pub vram_total: Option<u64>,
    }

    pub struct Driver {
        pub name: String,
        kind: Kind,
    }
    unsafe impl Send for Driver {}

    enum Kind {
        Nvidia { dev: *mut c_void, temp: NvTemp, power: NvU32, clock: NvClock, fan: NvU32, util: NvUtil, mem: NvMem },
        Amd { ctx: *mut c_void, adapter: i32, query: AdlQuery },
    }

    type NvU32 = unsafe extern "C" fn(*mut c_void, *mut u32) -> i32;
    type NvTemp = unsafe extern "C" fn(*mut c_void, i32, *mut u32) -> i32;
    type NvClock = unsafe extern "C" fn(*mut c_void, i32, *mut u32) -> i32;
    type NvUtil = unsafe extern "C" fn(*mut c_void, *mut [u32; 2]) -> i32;
    type NvMem = unsafe extern "C" fn(*mut c_void, *mut [u64; 3]) -> i32;
    type AdlQuery = unsafe extern "C" fn(*mut c_void, i32, *mut PmLog) -> i32;

    /// ADLPMLogDataOutput: size, then 256 (supported, value) pairs.
    #[repr(C)]
    pub struct PmLog {
        size: i32,
        sensors: [[i32; 2]; 256],
    }
    // Sensor numbers from AMD's adl_defines.h.
    const PMLOG_CLK_GFXCLK: usize = 1;
    const PMLOG_TEMPERATURE_EDGE: usize = 8;
    const PMLOG_FAN_PERCENTAGE: usize = 15;
    const PMLOG_INFO_ACTIVITY_GFX: usize = 19;
    const PMLOG_ASIC_POWER: usize = 23;
    const PMLOG_TEMPERATURE_HOTSPOT: usize = 27;
    /// Newer cards (RX 7000) report the whole board's power here instead of ASIC_POWER.
    const PMLOG_BOARD_POWER: usize = 73;

    unsafe fn sym<T>(lib: *mut c_void, name: &str) -> Option<T> {
        let n: Vec<u8> = name.bytes().chain(Some(0)).collect();
        GetProcAddress(lib as _, n.as_ptr()).map(|f| std::mem::transmute_copy(&f))
    }

    fn load(name: &str) -> *mut c_void {
        let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe { LoadLibraryW(w.as_ptr()) as *mut c_void }
    }

    /// ADL asks the caller for an allocator; its few allocations live as long as HotSwap.
    unsafe extern "system" fn adl_alloc(size: i32) -> *mut c_void {
        Box::into_raw(vec![0u8; size.max(1) as usize].into_boxed_slice()) as *mut c_void
    }

    impl Driver {
        pub fn open() -> Option<Driver> {
            unsafe { Self::nvidia().or_else(|| Self::amd()) }
        }

        unsafe fn nvidia() -> Option<Driver> {
            let lib = load("nvml.dll");
            if lib.is_null() {
                return None;
            }
            let init: unsafe extern "C" fn() -> i32 = sym(lib, "nvmlInit_v2")?;
            if init() != 0 {
                return None;
            }
            let by_index: unsafe extern "C" fn(u32, *mut *mut c_void) -> i32 = sym(lib, "nvmlDeviceGetHandleByIndex_v2")?;
            let mut dev = std::ptr::null_mut();
            if by_index(0, &mut dev) != 0 {
                return None;
            }
            let get_name: unsafe extern "C" fn(*mut c_void, *mut u8, u32) -> i32 = sym(lib, "nvmlDeviceGetName")?;
            let mut buf = [0u8; 96];
            get_name(dev, buf.as_mut_ptr(), buf.len() as u32);
            let name = String::from_utf8_lossy(&buf[..buf.iter().position(|&b| b == 0).unwrap_or(0)]).to_string();
            Some(Driver {
                name,
                kind: Kind::Nvidia {
                    dev,
                    temp: sym(lib, "nvmlDeviceGetTemperature")?,
                    power: sym(lib, "nvmlDeviceGetPowerUsage")?,
                    clock: sym(lib, "nvmlDeviceGetClockInfo")?,
                    fan: sym(lib, "nvmlDeviceGetFanSpeed")?,
                    util: sym(lib, "nvmlDeviceGetUtilizationRates")?,
                    mem: sym(lib, "nvmlDeviceGetMemoryInfo")?,
                },
            })
        }

        unsafe fn amd() -> Option<Driver> {
            let lib = load("atiadlxx.dll");
            if lib.is_null() {
                return None;
            }
            let create: unsafe extern "C" fn(unsafe extern "system" fn(i32) -> *mut c_void, i32, *mut *mut c_void) -> i32 = sym(lib, "ADL2_Main_Control_Create")?;
            let mut ctx = std::ptr::null_mut();
            if create(adl_alloc, 1, &mut ctx) != 0 {
                return None;
            }
            let count_fn: unsafe extern "C" fn(*mut c_void, *mut i32) -> i32 = sym(lib, "ADL2_Adapter_NumberOfAdapters_Get")?;
            let query: AdlQuery = sym(lib, "ADL2_New_QueryPMLogData_Get")?;
            let mut count = 0;
            count_fn(ctx, &mut count);
            // The first adapter that reports a temperature (one GPU shows up once per output).
            let mut log: Box<PmLog> = Box::new(std::mem::zeroed());
            let adapter = (0..count.clamp(0, 32)).find(|&a| {
                *log = std::mem::zeroed();
                query(ctx, a, &mut *log) == 0 && log.sensors[PMLOG_TEMPERATURE_EDGE][0] != 0
            })?;
            // The adapter's name, from ADL's AdapterInfo list (1572 bytes each, name at 0x118).
            let mut name = "AMD Radeon".to_string();
            if let Some(info) = sym::<unsafe extern "C" fn(*mut c_void, *mut u8, i32) -> i32>(lib, "ADL2_Adapter_AdapterInfo_Get") {
                const SIZE: usize = 1572;
                let mut buf = vec![0u8; SIZE * count.max(1) as usize];
                if info(ctx, buf.as_mut_ptr(), buf.len() as i32) == 0 {
                    let rec = &buf[adapter as usize * SIZE..];
                    let n = &rec[0x118..0x118 + 256];
                    let s = String::from_utf8_lossy(&n[..n.iter().position(|&b| b == 0).unwrap_or(0)]).trim().to_string();
                    if !s.is_empty() {
                        name = s;
                    }
                }
            }
            Some(Driver { name, kind: Kind::Amd { ctx, adapter, query } })
        }

        /// Dev aid: every sensor AMD's driver fills in, as (number, value).
        pub fn dump(&self) -> Vec<(usize, i32)> {
            unsafe {
                let Kind::Amd { ctx, adapter, query } = &self.kind else { return vec![] };
                let mut log: Box<PmLog> = Box::new(std::mem::zeroed());
                query(*ctx, *adapter, &mut *log);
                (0..256).filter(|&k| log.sensors[k][0] != 0).map(|k| (k, log.sensors[k][1])).collect()
            }
        }

        pub fn read(&mut self) -> Reading {
            unsafe {
                match &self.kind {
                    Kind::Nvidia { dev, temp, power, clock, fan, util, mem } => {
                        let mut r = Reading::default();
                        let mut v = 0u32;
                        if temp(*dev, 0, &mut v) == 0 {
                            r.temp = Some(v as f32);
                        }
                        if power(*dev, &mut v) == 0 {
                            r.power = Some(v as f32 / 1000.0);
                        }
                        if clock(*dev, 0, &mut v) == 0 {
                            r.clock = Some(v as f32);
                        }
                        if fan(*dev, &mut v) == 0 {
                            r.fan = Some(v as f32);
                        }
                        let mut u = [0u32; 2];
                        if util(*dev, &mut u) == 0 {
                            r.load = Some(u[0] as f32);
                        }
                        let mut m = [0u64; 3];
                        if mem(*dev, &mut m) == 0 {
                            r.vram_total = Some(m[0]);
                        }
                        r
                    }
                    Kind::Amd { ctx, adapter, query } => {
                        let mut log: Box<PmLog> = Box::new(std::mem::zeroed());
                        if query(*ctx, *adapter, &mut *log) != 0 {
                            return Reading::default();
                        }
                        let get = |k: usize| (log.sensors[k][0] != 0).then(|| log.sensors[k][1] as f32);
                        Reading {
                            load: get(PMLOG_INFO_ACTIVITY_GFX),
                            temp: get(PMLOG_TEMPERATURE_EDGE),
                            hotspot: get(PMLOG_TEMPERATURE_HOTSPOT),
                            power: get(PMLOG_ASIC_POWER).or_else(|| get(PMLOG_BOARD_POWER)),
                            clock: get(PMLOG_CLK_GFXCLK),
                            fan: get(PMLOG_FAN_PERCENTAGE),
                            vram_total: None,
                        }
                    }
                }
            }
        }
    }
}
