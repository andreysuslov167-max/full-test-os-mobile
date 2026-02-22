// tests/mobile_stress_advanced/mod.rs
#![cfg(target_os = "android")]

use std::time::{Duration, Instant, SystemTime};
use std::thread;
use std::sync::{Arc, Mutex, RwLock, atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}};
use std::fs::{self, File, OpenOptions};
use std::io::{Write, Read, Seek, SeekFrom, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, VecDeque, BTreeMap, HashSet};
use std::process::{Command, Stdio};
use std::f32::consts::PI;
use rand::{Rng, SeedableRng, distributions::Alphanumeric};
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Local};
use rayon::prelude::*;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use console::{style, Term};
use notify::{Watcher, RecursiveMode, RecommendedWatcher};
use backtrace::Backtrace;

// ============= Продвинутые структуры данных =============

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StressTestConfig {
    name: String,
    description: String,
    duration: Duration,
    intensity: StressIntensity,
    components: Vec<SystemComponent>,
    thresholds: ThresholdConfig,
    profile: TestProfile,
    safety_limits: SafetyLimits,
    reporting: ReportingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum StressIntensity {
    Light,      // 25% нагрузки
    Medium,     // 50% нагрузки
    Heavy,      // 75% нагрузки
    Extreme,    // 90% нагрузки
    Custom(f32), // Произвольный процент
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum SystemComponent {
    CPU,
    Memory,
    Filesystem,
    Network,
    GPU,
    Sensors,
    Battery,
    Thermal,
    IO,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThresholdConfig {
    max_cpu_temp_celsius: f32,
    max_battery_temp_celsius: f32,
    max_cpu_usage_percent: f32,
    max_memory_usage_mb: u64,
    max_disk_usage_percent: f32,
    min_fps: f32,
    max_frame_time_ms: f32,
    max_battery_drain_percent_per_minute: f32,
    max_thermal_throttling_seconds: u64,
    max_process_count: usize,
    max_thread_count: usize,
    max_open_files: usize,
    min_network_speed_kbps: u64,
    max_network_latency_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SafetyLimits {
    max_temperature_celsius: f32,
    max_battery_drain_percent: f32,
    max_memory_pressure_mb: u64,
    max_disk_write_gb: u64,
    enable_emergency_stop: bool,
    auto_recover: bool,
    recovery_timeout_secs: u64,
    max_consecutive_failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReportingConfig {
    realtime_updates: bool,
    save_metrics_interval_secs: u64,
    generate_html_report: bool,
    upload_to_cloud: bool,
    notify_on_threshold: bool,
    screenshot_on_failure: bool,
    record_video: bool,
    log_level: LogLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TestProfile {
    Gaming,
    Browsing,
    VideoPlayback,
    Navigation,
    Camera,
    Mixed,
    Custom(Vec<WorkloadProfile>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkloadProfile {
    name: String,
    cpu_weight: f32,
    memory_weight: f32,
    io_weight: f32,
    network_weight: f32,
    gpu_weight: f32,
    duration_secs: u64,
    pattern: LoadPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum LoadPattern {
    Constant,
    Sine { amplitude: f32, period_secs: u64 },
    Square { high_duration_secs: u64, low_duration_secs: u64 },
    Sawtooth { max: f32, period_secs: u64 },
    Random { min: f32, max: f32, change_interval_ms: u64 },
    Spikes { spike_duration_ms: u64, interval_secs: u64, intensity: f32 },
}

#[derive(Debug, Clone, Serialize)]
struct StressMetrics {
    timestamp: DateTime<Utc>,
    phase: String,
    
    // CPU Metrics
    cpu: CpuMetrics,
    cpu_history: VecDeque<f32>,
    
    // Memory Metrics
    memory: MemoryMetrics,
    memory_history: VecDeque<u64>,
    
    // Thermal Metrics
    thermal: ThermalMetrics,
    thermal_history: VecDeque<f32>,
    
    // Battery Metrics
    battery: BatteryMetrics,
    battery_history: VecDeque<f32>,
    
    // Performance Metrics
    performance: PerformanceMetrics,
    
    // System Metrics
    system: SystemMetrics,
    
    // Stress-specific Metrics
    stress_load: f32,
    operations_completed: u64,
    errors_this_phase: u32,
    warnings_this_phase: u32,
}

#[derive(Debug, Clone, Serialize)]
struct CpuMetrics {
    usage_percent: f32,
    per_core_usage: Vec<f32>,
    frequency_mhz: Vec<u32>,
    temperature_celsius: f32,
    governor: String,
    times_user: u64,
    times_nice: u64,
    times_system: u64,
    times_idle: u64,
    times_iowait: u64,
    times_irq: u64,
    times_softirq: u64,
    times_steal: u64,
    times_guest: u64,
    times_guest_nice: u64,
    context_switches: u64,
    processes: u64,
    procs_running: u64,
    procs_blocked: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryMetrics {
    total_kb: u64,
    free_kb: u64,
    available_kb: u64,
    buffers_kb: u64,
    cached_kb: u64,
    swap_cached_kb: u64,
    active_kb: u64,
    inactive_kb: u64,
    active_anon_kb: u64,
    inactive_anon_kb: u64,
    active_file_kb: u64,
    inactive_file_kb: u64,
    unevictable_kb: u64,
    mlocked_kb: u64,
    swap_total_kb: u64,
    swap_free_kb: u64,
    dirty_kb: u64,
    writeback_kb: u64,
    anon_pages_kb: u64,
    mapped_kb: u64,
    shmem_kb: u64,
    slab_kb: u64,
    sreclaimable_kb: u64,
    sunreclaim_kb: u64,
    kernel_stack_kb: u64,
    page_tables_kb: u64,
    nfs_unstable_kb: u64,
    bounce_kb: u64,
    writeback_tmp_kb: u64,
    commit_limit_kb: u64,
    committed_as_kb: u64,
    vmalloc_total_kb: u64,
    vmalloc_used_kb: u64,
    vmalloc_chunk_kb: u64,
    percpu_kb: u64,
    hardware_corrupted_kb: u64,
    anon_huge_pages_kb: u64,
    shmem_huge_pages_kb: u64,
    shmem_pmd_mapped_kb: u64,
    file_huge_pages_kb: u64,
    file_pmd_mapped_kb: u64,
    cma_total_kb: u64,
    cma_free_kb: u64,
}

#[derive(Debug, Clone, Serialize)]
struct ThermalMetrics {
    temperature_celsius: f32,
    thermal_zones: Vec<ThermalZone>,
    throttling_level: u32,
    throttling_start_time: Option<DateTime<Utc>>,
    throttling_duration_secs: u64,
    cooling_device_state: Vec<CoolingDevice>,
    max_temp_reached: f32,
    min_temp_reached: f32,
    avg_temp_last_minute: f32,
}

#[derive(Debug, Clone, Serialize)]
struct ThermalZone {
    name: String,
    type_: String,
    temperature_celsius: f32,
    policy: String,
    available_governors: Vec<String>,
    governor: String,
    trip_points: Vec<TripPoint>,
}

#[derive(Debug, Clone, Serialize)]
struct TripPoint {
    type_: String,
    temperature_celsius: f32,
    hysteresis_celsius: f32,
}

#[derive(Debug, Clone, Serialize)]
struct CoolingDevice {
    name: String,
    type_: String,
    max_state: u32,
    cur_state: u32,
}

#[derive(Debug, Clone, Serialize)]
struct BatteryMetrics {
    level_percent: f32,
    temperature_celsius: f32,
    voltage_uv: u32,
    current_ua: i32,
    capacity_ah: f32,
    charge_counter_ah: f32,
    energy_now_wh: f32,
    energy_full_wh: f32,
    power_now_mw: i32,
    status: String,
    health: String,
    technology: String,
    cycle_count: u32,
    serial_number: String,
    manufacture_date: String,
    temperature_history: VecDeque<f32>,
    current_history: VecDeque<i32>,
}

#[derive(Debug, Clone, Serialize)]
struct PerformanceMetrics {
    fps: f32,
    frame_time_ms: f32,
    jank_count: u32,
    gpu_usage_percent: f32,
    render_time_ms: f32,
    draw_calls: u32,
    triangles_drawn: u32,
    texture_memory_kb: u64,
    shader_compiles: u32,
    vsync_count: u32,
    missed_vsync: u32,
    input_latency_ms: f32,
    touch_response_ms: f32,
    scroll_jank: u32,
}

#[derive(Debug, Clone, Serialize)]
struct SystemMetrics {
    uptime_secs: u64,
    load_average_1: f32,
    load_average_5: f32,
    load_average_15: f32,
    total_processes: usize,
    running_processes: usize,
    total_threads: usize,
    open_files: usize,
    file_descriptors: usize,
    inodes_used: usize,
    inodes_total: usize,
    disk_reads: u64,
    disk_writes: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
    disk_io_time_ms: u64,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
    network_rx_packets: u64,
    network_tx_packets: u64,
    network_rx_errors: u64,
    network_tx_errors: u64,
    network_rx_dropped: u64,
    network_tx_dropped: u64,
}

// ============= Продвинутый стресс-генератор =============

struct AdvancedStressGenerator {
    config: StressTestConfig,
    metrics: Arc<RwLock<VecDeque<StressMetrics>>>,
    stop_signal: Arc<AtomicBool>,
    emergency_stop: Arc<AtomicBool>,
    error_count: Arc<AtomicU32>,
    operations_count: Arc<AtomicU64>,
    phase_progress: Arc<AtomicUsize>,
    workers: Vec<WorkerThread>,
    monitors: Vec<MonitorThread>,
    file_handles: Arc<Mutex<Vec<File>>>,
    temp_files: Arc<Mutex<Vec<PathBuf>>>,
    network_connections: Arc<Mutex<Vec<std::net::TcpStream>>>,
    progress_bars: MultiProgress,
}

struct WorkerThread {
    id: usize,
    component: SystemComponent,
    handle: Option<thread::JoinHandle<()>>,
    load_pattern: LoadPattern,
    intensity: f32,
}

struct MonitorThread {
    id: usize,
    name: String,
    handle: Option<thread::JoinHandle<()>>,
}

impl AdvancedStressGenerator {
    fn new(config: StressTestConfig) -> Self {
        let progress_bars = MultiProgress::new();
        
        Self {
            config,
            metrics: Arc::new(RwLock::new(VecDeque::with_capacity(10000))),
            stop_signal: Arc::new(AtomicBool::new(false)),
            emergency_stop: Arc::new(AtomicBool::new(false)),
            error_count: Arc::new(AtomicU32::new(0)),
            operations_count: Arc::new(AtomicU64::new(0)),
            phase_progress: Arc::new(AtomicUsize::new(0)),
            workers: Vec::new(),
            monitors: Vec::new(),
            file_handles: Arc::new(Mutex::new(Vec::new())),
            temp_files: Arc::new(Mutex::new(Vec::new())),
            network_connections: Arc::new(Mutex::new(Vec::new())),
            progress_bars,
        }
    }
    
    fn run(&mut self) -> Result<StressTestReport, String> {
        println!("\n{}", "=".repeat(80));
        println!("{}", style(format!("🔥 STRESS TEST: {}", self.config.name)).bold().cyan());
        println!("{}", style(format!("📝 {}", self.config.description)).dim());
        println!("{}", "=".repeat(80));
        
        let start_time = Utc::now();
        let mut phase_results = Vec::new();
        
        // Инициализация прогресс-баров
        let main_pb = self.create_progress_bar("Overall Progress", self.config.duration.as_secs() as u64);
        let cpu_pb = self.create_progress_bar("CPU Stress", 100);
        let mem_pb = self.create_progress_bar("Memory Stress", 100);
        let io_pb = self.create_progress_bar("I/O Stress", 100);
        
        // Запуск мониторов
        self.start_monitors();
        
        // Запуск воркеров для каждого компонента
        for component in &self.config.components {
            self.start_worker(component.clone())?;
        }
        
        // Основной цикл стресс-теста
        let start_instant = Instant::now();
        let update_interval = Duration::from_secs(1);
        let mut last_update = Instant::now();
        
        while start_instant.elapsed() < self.config.duration {
            // Проверка аварийной остановки
            if self.emergency_stop.load(Ordering::Relaxed) {
                println!("\n{}", style("⚠️ EMERGENCY STOP ACTIVATED!").red().bold());
                break;
            }
            
            // Обновление прогресса
            let elapsed = start_instant.elapsed();
            let progress = (elapsed.as_secs_f64() / self.config.duration.as_secs_f64() * 100.0) as usize;
            self.phase_progress.store(progress, Ordering::Relaxed);
            
            main_pb.set_position(elapsed.as_secs() as u64);
            
            // Сбор метрик
            if last_update.elapsed() >= update_interval {
                let metrics = self.collect_metrics("running")?;
                self.metrics.write().unwrap().push_back(metrics);
                
                // Обновление UI
                self.update_ui(&cpu_pb, &mem_pb, &io_pb);
                
                // Проверка пороговых значений
                self.check_thresholds()?;
                
                last_update = Instant::now();
            }
            
            thread::sleep(Duration::from_millis(100));
        }
        
        // Остановка всех воркеров
        self.stop_all_workers();
        
        // Сбор финальных метрик
        let final_metrics = self.collect_metrics("completed")?;
        self.metrics.write().unwrap().push_back(final_metrics);
        
        // Завершение мониторов
        self.stop_monitors();
        
        let end_time = Utc::now();
        
        // Генерация отчета
        let report = self.generate_report(start_time, end_time)?;
        
        println!("\n{}", "=".repeat(80));
        println!("{}", style("✅ STRESS TEST COMPLETED").green().bold());
        println!("{}", "=".repeat(80));
        
        Ok(report)
    }
    
    fn create_progress_bar(&self, message: &str, length: u64) -> ProgressBar {
        let pb = self.progress_bars.add(ProgressBar::new(length));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("█▓▒░")
        );
        pb.set_message(message.to_string());
        pb
    }
    
    fn start_worker(&mut self, component: SystemComponent) -> Result<(), String> {
        let id = self.workers.len();
        let stop = self.stop_signal.clone();
        let emergency = self.emergency_stop.clone();
        let metrics = self.metrics.clone();
        let ops = self.operations_count.clone();
        let errors = self.error_count.clone();
        let intensity = self.get_component_intensity(&component);
        let pattern = self.get_load_pattern(&component);
        
        let handle = thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let worker_start = Instant::now();
            
            while !stop.load(Ordering::Relaxed) && !emergency.load(Ordering::Relaxed) {
                // Вычисление текущей нагрузки по паттерну
                let current_load = Self::calculate_load(&pattern, worker_start.elapsed(), intensity);
                
                match component {
                    SystemComponent::CPU => {
                        Self::stress_cpu(current_load, &ops, &errors);
                    }
                    SystemComponent::Memory => {
                        Self::stress_memory(current_load, &ops, &errors);
                    }
                    SystemComponent::Filesystem => {
                        Self::stress_filesystem(current_load, &ops, &errors);
                    }
                    SystemComponent::Network => {
                        Self::stress_network(current_load, &ops, &errors);
                    }
                    SystemComponent::GPU => {
                        Self::stress_gpu(current_load, &ops, &errors);
                    }
                    SystemComponent::Sensors => {
                        Self::stress_sensors(current_load, &ops, &errors);
                    }
                    SystemComponent::Battery => {
                        Self::stress_battery(current_load, &ops, &errors);
                    }
                    SystemComponent::Thermal => {
                        Self::stress_thermal(current_load, &ops, &errors);
                    }
                    SystemComponent::IO => {
                        Self::stress_io(current_load, &ops, &errors);
                    }
                    SystemComponent::All => {
                        Self::stress_all(current_load, &ops, &errors);
                    }
                }
                
                thread::sleep(Duration::from_micros(100));
            }
        });
        
        self.workers.push(WorkerThread {
            id,
            component,
            handle: Some(handle),
            load_pattern: pattern,
            intensity,
        });
        
        println!("{} Worker {} started for {:?}", 
            style("▶").green(), id, component);
        
        Ok(())
    }
    
    fn get_component_intensity(&self, component: &SystemComponent) -> f32 {
        match self.config.intensity {
            StressIntensity::Light => 0.25,
            StressIntensity::Medium => 0.5,
            StressIntensity::Heavy => 0.75,
            StressIntensity::Extreme => 0.9,
            StressIntensity::Custom(p) => p,
        }
    }
    
    fn get_load_pattern(&self, component: &SystemComponent) -> LoadPattern {
        match self.config.profile {
            TestProfile::Gaming => LoadPattern::Spikes {
                spike_duration_ms: 100,
                interval_secs: 5,
                intensity: 1.0,
            },
            TestProfile::Browsing => LoadPattern::Random {
                min: 0.2,
                max: 0.6,
                change_interval_ms: 500,
            },
            TestProfile::VideoPlayback => LoadPattern::Constant,
            TestProfile::Navigation => LoadPattern::Sawtooth {
                max: 0.8,
                period_secs: 30,
            },
            TestProfile::Camera => LoadPattern::Sine {
                amplitude: 0.3,
                period_secs: 10,
            },
            TestProfile::Mixed => LoadPattern::Random {
                min: 0.1,
                max: 0.9,
                change_interval_ms: 1000,
            },
            TestProfile::Custom(ref profiles) => {
                if let Some(p) = profiles.first() {
                    p.pattern.clone()
                } else {
                    LoadPattern::Constant
                }
            }
        }
    }
    
    fn calculate_load(pattern: &LoadPattern, elapsed: Duration, base_intensity: f32) -> f32 {
        let t = elapsed.as_secs_f32();
        
        match pattern {
            LoadPattern::Constant => base_intensity,
            
            LoadPattern::Sine { amplitude, period_secs } => {
                let period = *period_secs as f32;
                base_intensity + amplitude * (2.0 * PI * t / period).sin()
            }
            
            LoadPattern::Square { high_duration_secs, low_duration_secs } => {
                let cycle = *high_duration_secs as f32 + *low_duration_secs as f32;
                let phase = t % cycle;
                if phase < *high_duration_secs as f32 {
                    base_intensity
                } else {
                    base_intensity * 0.1
                }
            }
            
            LoadPattern::Sawtooth { max, period_secs } => {
                let period = *period_secs as f32;
                let phase = (t % period) / period;
                base_intensity * phase * max
            }
            
            LoadPattern::Random { min, max, change_interval_ms } => {
                static mut LAST_CHANGE: f32 = 0.0;
                static mut CURRENT_LOAD: f32 = 0.5;
                
                unsafe {
                    if t - LAST_CHANGE > *change_interval_ms as f32 / 1000.0 {
                        CURRENT_LOAD = rand::thread_rng().gen_range(*min..=*max);
                        LAST_CHANGE = t;
                    }
                    CURRENT_LOAD * base_intensity
                }
            }
            
            LoadPattern::Spikes { spike_duration_ms, interval_secs, intensity } => {
                let spike_start = (t / *interval_secs as f32).floor() * *interval_secs as f32;
                if t - spike_start < *spike_duration_ms as f32 / 1000.0 {
                    base_intensity * intensity
                } else {
                    base_intensity * 0.2
                }
            }
        }
    }
    
    fn stress_cpu(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        let iterations = (load * 1000.0) as u64;
        
        for _ in 0..iterations {
            // Разные типы вычислений для разных ядер CPU
            let _ = (0..1000).map(|i| {
                let x = i as f64;
                x.sin() * x.cos() + x.tan() * x.sqrt()
            }).sum::<f64>();
            
            // FPU нагрузка
            let mut a = 1.0;
            for _ in 0..100 {
                a = (a * 1.0001).sin() * a.cos();
            }
            black_box(a);
            
            // Целочисленные операции
            let mut b = 1u64;
            for _ in 0..1000 {
                b = b.wrapping_mul(123456789).wrapping_add(987654321);
            }
            black_box(b);
            
            ops.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    fn stress_memory(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        let size = (load * 10.0 * 1024.0 * 1024.0) as usize; // до 10MB
        
        // Разные паттерны доступа к памяти
        let mut vec1 = vec![0u8; size];
        let mut vec2 = vec![0u8; size];
        
        // Последовательный доступ
        for i in 0..vec1.len() {
            vec1[i] = vec1[i].wrapping_add(1);
        }
        
        // Случайный доступ
        let mut rng = rand::thread_rng();
        for _ in 0..1000 {
            let idx = rng.gen_range(0..vec1.len());
            vec1[idx] = vec2[idx];
        }
        
        // Копирование памяти
        vec2.copy_from_slice(&vec1);
        
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_filesystem(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        let temp_dir = PathBuf::from("/data/local/tmp/stress_test");
        fs::create_dir_all(&temp_dir).ok();
        
        let file_size = (load * 1024.0 * 1024.0) as usize;
        let data = vec![rand::random::<u8>(); file_size];
        
        // Запись
        let file_path = temp_dir.join(format!("stress_{}.tmp", rand::random::<u32>()));
        if let Ok(mut file) = File::create(&file_path) {
            let _ = file.write_all(&data);
            let _ = file.sync_all();
        }
        
        // Чтение
        if let Ok(mut file) = File::open(&file_path) {
            let mut buffer = vec![0; file_size];
            let _ = file.read_exact(&mut buffer);
        }
        
        // Удаление
        let _ = fs::remove_file(&file_path);
        
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_network(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        // Имитация сетевой нагрузки без реальных запросов
        let packet_size = (load * 1024.0) as usize;
        let _packet = vec![rand::random::<u8>(); packet_size];
        
        // Имитация сетевых задержек
        thread::sleep(Duration::from_micros((load * 1000.0) as u64));
        
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_gpu(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        // Имитация GPU нагрузки через интенсивные вычисления
        let size = (load * 100.0) as usize;
        
        let mut matrix_a = vec![vec![0.0f32; size]; size];
        let mut matrix_b = vec![vec![0.0f32; size]; size];
        let mut result = vec![vec![0.0f32; size]; size];
        
        for i in 0..size {
            for j in 0..size {
                matrix_a[i][j] = rand::random();
                matrix_b[i][j] = rand::random();
            }
        }
        
        // Умножение матриц (тяжелая нагрузка)
        for i in 0..size {
            for j in 0..size {
                for k in 0..size {
                    result[i][j] += matrix_a[i][k] * matrix_b[k][j];
                }
            }
        }
        
        black_box(result);
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_sensors(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        // Чтение сенсоров через sysfs
        let sensor_paths = vec![
            "/sys/class/thermal/thermal_zone0/temp",
            "/sys/class/power_supply/battery/temp",
            "/sys/class/input/input0/device/orientation",
        ];
        
        for path in sensor_paths {
            if let Ok(content) = fs::read_to_string(path) {
                let _ = content.trim().parse::<f32>();
            }
        }
        
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_battery(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        // Мониторинг батареи
        let battery_path = "/sys/class/power_supply/battery";
        
        let files = vec![
            "capacity",
            "voltage_now",
            "current_now",
            "temp",
            "status",
        ];
        
        for file in files {
            let path = format!("{}/{}", battery_path, file);
            if let Ok(content) = fs::read_to_string(path) {
                let _ = content.trim().parse::<f32>();
            }
        }
        
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_thermal(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        // Мониторинг термальных зон
        let thermal_path = "/sys/class/thermal";
        
        if let Ok(entries) = fs::read_dir(thermal_path) {
            for entry in entries.filter_map(Result::ok) {
                let temp_path = entry.path().join("temp");
                if let Ok(content) = fs::read_to_string(temp_path) {
                    let _ = content.trim().parse::<f32>();
                }
            }
        }
        
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_io(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        // I/O нагрузка
        let io_size = (load * 1024.0 * 1024.0) as u64;
        let temp_dir = PathBuf::from("/data/local/tmp/io_stress");
        fs::create_dir_all(&temp_dir).ok();
        
        let file_path = temp_dir.join("io_test.dat");
        
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&file_path) 
        {
            // Случайная запись
            let data = vec![rand::random::<u8>(); 4096];
            for offset in (0..io_size).step_by(4096) {
                let _ = file.seek(SeekFrom::Start(offset));
                let _ = file.write_all(&data);
            }
            
            // Случайное чтение
            let mut buffer = vec![0; 4096];
            for offset in (0..io_size).step_by(4096) {
                let _ = file.seek(SeekFrom::Start(offset));
                let _ = file.read_exact(&mut buffer);
            }
            
            // fsync
            let _ = file.sync_all();
        }
        
        let _ = fs::remove_file(&file_path);
        
        ops.fetch_add(1, Ordering::Relaxed);
    }
    
    fn stress_all(load: f32, ops: &Arc<AtomicU64>, errors: &Arc<AtomicU32>) {
        Self::stress_cpu(load * 0.2, ops, errors);
        Self::stress_memory(load * 0.2, ops, errors);
        Self::stress_filesystem(load * 0.2, ops, errors);
        Self::stress_network(load * 0.1, ops, errors);
        Self::stress_gpu(load * 0.1, ops, errors);
        Self::stress_sensors(load * 0.05, ops, errors);
        Self::stress_battery(load * 0.05, ops, errors);
        Self::stress_thermal(load * 0.05, ops, errors);
        Self::stress_io(load * 0.05, ops, errors);
    }
    
    fn start_monitors(&mut self) {
        let monitors = vec![
            ("CPU Monitor", Self::monitor_cpu),
            ("Memory Monitor", Self::monitor_memory),
            ("Thermal Monitor", Self::monitor_thermal),
            ("Battery Monitor", Self::monitor_battery),
            ("IO Monitor", Self::monitor_io),
            ("Network Monitor", Self::monitor_network),
        ];
        
        for (id, (name, func)) in monitors.iter().enumerate() {
            let stop = self.stop_signal.clone();
            let metrics = self.metrics.clone();
            
            let handle = thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    func(&metrics);
                    thread::sleep(Duration::from_secs(1));
                }
            });
            
            self.monitors.push(MonitorThread {
                id,
                name: name.to_string(),
                handle: Some(handle),
            });
        }
    }
    
    fn monitor_cpu(metrics: &Arc<RwLock<VecDeque<StressMetrics>>>) {
        if let Ok(cpu_metrics) = read_cpu_stats() {
            if let Ok(mut metrics_guard) = metrics.write() {
                if let Some(last) = metrics_guard.back_mut() {
                    last.cpu = cpu_metrics;
                }
            }
        }
    }
    
    fn monitor_memory(metrics: &Arc<RwLock<VecDeque<StressMetrics>>>) {
        if let Ok(mem_metrics) = read_memory_stats() {
            if let Ok(mut metrics_guard) = metrics.write() {
                if let Some(last) = metrics_guard.back_mut() {
                    last.memory = mem_metrics;
                }
            }
        }
    }
    
    fn monitor_thermal(metrics: &Arc<RwLock<VecDeque<StressMetrics>>>) {
        if let Ok(thermal_metrics) = read_thermal_stats() {
            if let Ok(mut metrics_guard) = metrics.write() {
                if let Some(last) = metrics_guard.back_mut() {
                    last.thermal = thermal_metrics;
                    last.thermal_history.push_back(thermal_metrics.temperature_celsius);
                    if last.thermal_history.len() > 60 {
                        last.thermal_history.pop_front();
                    }
                }
            }
        }
    }
    
    fn monitor_battery(metrics: &Arc<RwLock<VecDeque<StressMetrics>>>) {
        if let Ok(battery_metrics) = read_battery_stats() {
            if let Ok(mut metrics_guard) = metrics.write() {
                if let Some(last) = metrics_guard.back_mut() {
                    last.battery = battery_metrics.clone();
                    last.battery_history.push_back(battery_metrics.level_percent);
                    if last.battery_history.len() > 60 {
                        last.battery_history.pop_front();
                    }
                }
            }
        }
    }
    
    fn monitor_io(metrics: &Arc<RwLock<VecDeque<StressMetrics>>>) {
        if let Ok(io_stats) = read_io_stats() {
            if let Ok(mut metrics_guard) = metrics.write() {
                if let Some(last) = metrics_guard.back_mut() {
                    last.system.disk_reads = io_stats.0;
                    last.system.disk_writes = io_stats.1;
                }
            }
        }
    }
    
    fn monitor_network(metrics: &Arc<RwLock<VecDeque<StressMetrics>>>) {
        if let Ok(net_stats) = read_network_stats() {
            if let Ok(mut metrics_guard) = metrics.write() {
                if let Some(last) = metrics_guard.back_mut() {
                    last.system.network_rx_bytes = net_stats.0;
                    last.system.network_tx_bytes = net_stats.1;
                }
            }
        }
    }
    
    fn collect_metrics(&self, phase: &str) -> Result<StressMetrics, String> {
        let cpu = read_cpu_stats()?;
        let memory = read_memory_stats()?;
        let thermal = read_thermal_stats()?;
        let battery = read_battery_stats()?;
        let system = read_system_stats()?;
        
        let metrics = StressMetrics {
            timestamp: Utc::now(),
            phase: phase.to_string(),
            
            cpu: cpu.clone(),
            cpu_history: VecDeque::new(),
            
            memory: memory.clone(),
            memory_history: VecDeque::new(),
            
            thermal: thermal.clone(),
            thermal_history: VecDeque::new(),
            
            battery: battery.clone(),
            battery_history: VecDeque::new(),
            
            performance: PerformanceMetrics {
                fps: 60.0,
                frame_time_ms: 16.6,
                jank_count: 0,
                gpu_usage_percent: rand::thread_rng().gen_range(0..100),
                render_time_ms: 10.0,
                draw_calls: 100,
                triangles_drawn: 10000,
                texture_memory_kb: 100 * 1024,
                shader_compiles: 0,
                vsync_count: 60,
                missed_vsync: 0,
                input_latency_ms: 10.0,
                touch_response_ms: 20.0,
                scroll_jank: 0,
            },
            
            system,
            
            stress_load: self.phase_progress.load(Ordering::Relaxed) as f32 / 100.0,
            operations_completed: self.operations_count.load(Ordering::Relaxed),
            errors_this_phase: self.error_count.load(Ordering::Relaxed),
            warnings_this_phase: 0,
        };
        
        Ok(metrics)
    }
    
    fn check_thresholds(&self) -> Result<(), String> {
        let metrics = self.metrics.read().unwrap();
        if let Some(last) = metrics.back() {
            let thresholds = &self.config.thresholds;
            
            // Проверка температуры CPU
            if last.cpu.temperature_celsius > thresholds.max_cpu_temp_celsius {
                println!("{} CPU temperature too high: {:.1}°C > {:.1}°C",
                    style("⚠").yellow(),
                    last.cpu.temperature_celsius,
                    thresholds.max_cpu_temp_celsius);
                self.error_count.fetch_add(1, Ordering::Relaxed);
            }
            
            // Проверка температуры батареи
            if last.battery.temperature_celsius > thresholds.max_battery_temp_celsius {
                println!("{} Battery temperature too high: {:.1}°C > {:.1}°C",
                    style("⚠").yellow(),
                    last.battery.temperature_celsius,
                    thresholds.max_battery_temp_celsius);
                self.error_count.fetch_add(1, Ordering::Relaxed);
            }
            
            // Проверка использования CPU
            if last.cpu.usage_percent > thresholds.max_cpu_usage_percent {
                println!("{} CPU usage too high: {:.1}% > {:.1}%",
                    style("⚠").yellow(),
                    last.cpu.usage_percent,
                    thresholds.max_cpu_usage_percent);
            }
            
            // Проверка памяти
            if last.memory.used_kb / 1024 > thresholds.max_memory_usage_mb {
                println!("{} Memory usage too high: {}MB > {}MB",
                    style("⚠").yellow(),
                    last.memory.used_kb / 1024,
                    thresholds.max_memory_usage_mb);
            }
            
            // Проверка безопасности
            if last.battery.temperature_celsius > self.config.safety_limits.max_temperature_celsius {
                println!("{} CRITICAL TEMPERATURE! Emergency stop.",
                    style("🔥").red().bold());
                self.emergency_stop.store(true, Ordering::Relaxed);
            }
            
            // Проверка количества ошибок
            if self.error_count.load(Ordering::Relaxed) > self.config.safety_limits.max_consecutive_failures {
                println!("{} Too many consecutive failures! Emergency stop.",
                    style("❌").red().bold());
                self.emergency_stop.store(true, Ordering::Relaxed);
            }
        }
        
        Ok(())
    }
    
    fn update_ui(&self, cpu_pb: &ProgressBar, mem_pb: &ProgressBar, io_pb: &ProgressBar) {
        if let Ok(metrics) = self.metrics.read() {
            if let Some(last) = metrics.back() {
                cpu_pb.set_position((last.cpu.usage_percent * 100.0) as u64);
                cpu_pb.set_message(format!("CPU: {:.1}% @ {:.1}°C", 
                    last.cpu.usage_percent, last.cpu.temperature_celsius));
                
                let mem_percent = last.memory.used_kb as f32 / last.memory.total_kb as f32 * 100.0;
                mem_pb.set_position(mem_percent as u64);
                mem_pb.set_message(format!("Memory: {:.1}%", mem_percent));
                
                io_pb.set_position((last.stress_load * 100.0) as u64);
                io_pb.set_message(format!("Ops: {} | Errors: {}", 
                    last.operations_completed, last.errors_this_phase));
            }
        }
    }
    
    fn stop_all_workers(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        
        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
                println!("{} Worker {} stopped", style("◼").red(), worker.id);
            }
        }
    }
    
    fn stop_monitors(&mut self) {
        for monitor in &mut self.monitors {
            if let Some(handle) = monitor.handle.take() {
                let _ = handle.join();
            }
        }
    }
    
    fn generate_report(&self, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> Result<StressTestReport, String> {
        let metrics: Vec<StressMetrics> = self.metrics.read().unwrap().iter().cloned().collect();
        
        // Расчет статистики
        let mut cpu_usages = Vec::new();
        let mut memory_usages = Vec::new();
        let mut temperatures = Vec::new();
        let mut battery_levels = Vec::new();
        
        for m in &metrics {
            cpu_usages.push(m.cpu.usage_percent);
            memory_usages.push(m.memory.used_kb as f64 / 1024.0 / 1024.0);
            temperatures.push(m.thermal.temperature_celsius);
            battery_levels.push(m.battery.level_percent);
        }
        
        let avg_cpu = cpu_usages.iter().sum::<f32>() / cpu_usages.len() as f32;
        let max_cpu = cpu_usages.iter().fold(0.0, |a, &b| a.max(b));
        let min_cpu = cpu_usages.iter().fold(100.0, |a, &b| a.min(b));
        
        let avg_memory = memory_usages.iter().sum::<f64>() / memory_usages.len() as f64;
        let max_memory = memory_usages.iter().fold(0.0, |a, &b| a.max(b));
        
        let avg_temp = temperatures.iter().sum::<f32>() / temperatures.len() as f32;
        let max_temp = temperatures.iter().fold(0.0, |a, &b| a.max(b));
        
        let battery_start = battery_levels.first().unwrap_or(&0.0);
        let battery_end = battery_levels.last().unwrap_or(&0.0);
        let battery_drain = battery_start - battery_end;
        
        let total_ops = self.operations_count.load(Ordering::Relaxed);
        let total_errors = self.error_count.load(Ordering::Relaxed);
        
        let report = StressTestReport {
            test_name: self.config.name.clone(),
            description: self.config.description.clone(),
            start_time,
            end_time,
            duration: end_time - start_time,
            intensity: self.config.intensity.clone(),
            
            summary: TestSummary {
                total_operations: total_ops,
                operations_per_second: total_ops as f64 / (end_time - start_time).num_seconds() as f64,
                total_errors,
                error_rate: total_errors as f64 / total_ops as f64,
                
                avg_cpu_usage: avg_cpu,
                max_cpu_usage: max_cpu,
                min_cpu_usage: min_cpu,
                
                avg_memory_mb: avg_memory,
                max_memory_mb: max_memory,
                
                avg_temperature: avg_temp,
                max_temperature: max_temp,
                min_temperature: temperatures.iter().fold(100.0, |a, &b| a.min(b)),
                
                battery_drain_percent: battery_drain,
                battery_start_percent: *battery_start,
                battery_end_percent: *battery_end,
                
                thermal_throttling_events: metrics.iter().filter(|m| m.thermal.throttling_level > 0).count(),
                emergency_stops: if self.emergency_stop.load(Ordering::Relaxed) { 1 } else { 0 },
            },
            
            detailed_metrics: metrics,
            thresholds_exceeded: self.get_thresholds_exceeded(),
            
            device_info: read_device_info().unwrap_or_default(),
            
            passed: total_errors == 0 && !self.emergency_stop.load(Ordering::Relaxed),
        };
        
        Ok(report)
    }
    
    fn get_thresholds_exceeded(&self) -> Vec<String> {
        let mut exceeded = Vec::new();
        let metrics = self.metrics.read().unwrap();
        
        if let Some(last) = metrics.back() {
            if last.cpu.temperature_celsius > self.config.thresholds.max_cpu_temp_celsius {
                exceeded.push(format!("CPU temperature: {:.1}°C", last.cpu.temperature_celsius));
            }
            if last.battery.temperature_celsius > self.config.thresholds.max_battery_temp_celsius {
                exceeded.push(format!("Battery temperature: {:.1}°C", last.battery.temperature_celsius));
            }
            if last.cpu.usage_percent > self.config.thresholds.max_cpu_usage_percent {
                exceeded.push(format!("CPU usage: {:.1}%", last.cpu.usage_percent));
            }
            if last.memory.used_kb / 1024 > self.config.thresholds.max_memory_usage_mb {
                exceeded.push(format!("Memory usage: {}MB", last.memory.used_kb / 1024));
            }
        }
        
        exceeded
    }
}

#[derive(Debug, Clone, Serialize)]
struct StressTestReport {
    test_name: String,
    description: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    duration: Duration,
    intensity: StressIntensity,
    
    summary: TestSummary,
    detailed_metrics: Vec<StressMetrics>,
    thresholds_exceeded: Vec<String>,
    device_info: DeviceInfo,
    
    passed: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TestSummary {
    total_operations: u64,
    operations_per_second: f64,
    total_errors: u32,
    error_rate: f64,
    
    avg_cpu_usage: f32,
    max_cpu_usage: f32,
    min_cpu_usage: f32,
    
    avg_memory_mb: f64,
    max_memory_mb: f64,
    
    avg_temperature: f32,
    max_temperature: f32,
    min_temperature: f32,
    
    battery_drain_percent: f32,
    battery_start_percent: f32,
    battery_end_percent: f32,
    
    thermal_throttling_events: usize,
    emergency_stops: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
struct DeviceInfo {
    manufacturer: String,
    model: String,
    android_version: String,
    kernel_version: String,
    cpu_cores: usize,
    cpu_max_freq_mhz: u32,
    total_ram_mb: u64,
    total_storage_mb: u64,
    battery_capacity_mah: u32,
}

// ============= Вспомогательные функции чтения статистики =============

fn read_cpu_stats() -> Result<CpuMetrics, String> {
    let mut metrics = CpuMetrics {
        usage_percent: 0.0,
        per_core_usage: Vec::new(),
        frequency_mhz: Vec::new(),
        temperature_celsius: 0.0,
        governor: "unknown".to_string(),
        times_user: 0,
        times_nice: 0,
        times_system: 0,
        times_idle: 0,
        times_iowait: 0,
        times_irq: 0,
        times_softirq: 0,
        times_steal: 0,
        times_guest: 0,
        times_guest_nice: 0,
        context_switches: 0,
        processes: 0,
        procs_running: 0,
        procs_blocked: 0,
    };
    
    // Чтение /proc/stat
    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        for line in stat.lines() {
            if line.starts_with("cpu ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 8 {
                    metrics.times_user = parts[1].parse().unwrap_or(0);
                    metrics.times_nice = parts[2].parse().unwrap_or(0);
                    metrics.times_system = parts[3].parse().unwrap_or(0);
                    metrics.times_idle = parts[4].parse().unwrap_or(0);
                    metrics.times_iowait = parts.get(5).unwrap_or(&"0").parse().unwrap_or(0);
                    metrics.times_irq = parts.get(6).unwrap_or(&"0").parse().unwrap_or(0);
                    metrics.times_softirq = parts.get(7).unwrap_or(&"0").parse().unwrap_or(0);
                    metrics.times_steal = parts.get(8).unwrap_or(&"0").parse().unwrap_or(0);
                    metrics.times_guest = parts.get(9).unwrap_or(&"0").parse().unwrap_or(0);
                    metrics.times_guest_nice = parts.get(10).unwrap_or(&"0").parse().unwrap_or(0);
                    
                    let total = metrics.times_user + metrics.times_nice + metrics.times_system + 
                                metrics.times_idle + metrics.times_iowait + metrics.times_irq + 
                                metrics.times_softirq + metrics.times_steal;
                    let idle = metrics.times_idle + metrics.times_iowait;
                    
                    if total > 0 {
                        metrics.usage_percent = (total - idle) as f32 / total as f32 * 100.0;
                    }
                }
            } else if line.starts_with("ctxt ") {
                metrics.context_switches = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            } else if line.starts_with("processes ") {
                metrics.processes = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            } else if line.starts_with("procs_running ") {
                metrics.procs_running = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            } else if line.starts_with("procs_blocked ") {
                metrics.procs_blocked = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            }
        }
    }
    
    // Чтение частот CPU
    for cpu in 0..num_cpus::get() {
        let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", cpu);
        if let Ok(freq) = fs::read_to_string(path) {
            if let Ok(freq_num) = freq.trim().parse::<u32>() {
                metrics.frequency_mhz.push(freq_num / 1000);
            }
        }
    }
    
    // Чтение температуры
    if let Ok(temp) = fs::read_to_string("/sys/class/thermal/thermal_zone0/temp") {
        metrics.temperature_celsius = temp.trim().parse::<f32>().unwrap_or(0.0) / 1000.0;
    }
    
    Ok(metrics)
}

fn read_memory_stats() -> Result<MemoryMetrics, String> {
    let mut metrics = MemoryMetrics {
        total_kb: 0,
        free_kb: 0,
        available_kb: 0,
        buffers_kb: 0,
        cached_kb: 0,
        swap_cached_kb: 0,
        active_kb: 0,
        inactive_kb: 0,
        active_anon_kb: 0,
        inactive_anon_kb: 0,
        active_file_kb: 0,
        inactive_file_kb: 0,
        unevictable_kb: 0,
        mlocked_kb: 0,
        swap_total_kb: 0,
        swap_free_kb: 0,
        dirty_kb: 0,
        writeback_kb: 0,
        anon_pages_kb: 0,
        mapped_kb: 0,
        shmem_kb: 0,
        slab_kb: 0,
        sreclaimable_kb: 0,
        sunreclaim_kb: 0,
        kernel_stack_kb: 0,
        page_tables_kb: 0,
        nfs_unstable_kb: 0,
        bounce_kb: 0,
        writeback_tmp_kb: 0,
        commit_limit_kb: 0,
        committed_as_kb: 0,
        vmalloc_total_kb: 0,
        vmalloc_used_kb: 0,
        vmalloc_chunk_kb: 0,
        percpu_kb: 0,
        hardware_corrupted_kb: 0,
        anon_huge_pages_kb: 0,
        shmem_huge_pages_kb: 0,
        shmem_pmd_mapped_kb: 0,
        file_huge_pages_kb: 0,
        file_pmd_mapped_kb: 0,
        cma_total_kb: 0,
        cma_free_kb: 0,
    };
    
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let value = parts[1].parse::<u64>().unwrap_or(0);
                
                match parts[0] {
                    "MemTotal:" => metrics.total_kb = value,
                    "MemFree:" => metrics.free_kb = value,
                    "MemAvailable:" => metrics.available_kb = value,
                    "Buffers:" => metrics.buffers_kb = value,
                    "Cached:" => metrics.cached_kb = value,
                    "SwapCached:" => metrics.swap_cached_kb = value,
                    "Active:" => metrics.active_kb = value,
                    "Inactive:" => metrics.inactive_kb = value,
                    "Active(anon):" => metrics.active_anon_kb = value,
                    "Inactive(anon):" => metrics.inactive_anon_kb = value,
                    "Active(file):" => metrics.active_file_kb = value,
                    "Inactive(file):" => metrics.inactive_file_kb = value,
                    "Unevictable:" => metrics.unevictable_kb = value,
                    "Mlocked:" => metrics.mlocked_kb = value,
                    "SwapTotal:" => metrics.swap_total_kb = value,
                    "SwapFree:" => metrics.swap_free_kb = value,
                    "Dirty:" => metrics.dirty_kb = value,
                    "Writeback:" => metrics.writeback_kb = value,
                    "AnonPages:" => metrics.anon_pages_kb = value,
                    "Mapped:" => metrics.mapped_kb = value,
                    "Shmem:" => metrics.shmem_kb = value,
                    "Slab:" => metrics.slab_kb = value,
                    "SReclaimable:" => metrics.sreclaimable_kb = value,
                    "SUnreclaim:" => metrics.sunreclaim_kb = value,
                    "KernelStack:" => metrics.kernel_stack_kb = value,
                    "PageTables:" => metrics.page_tables_kb = value,
                    "NFS_Unstable:" => metrics.nfs_unstable_kb = value,
                    "Bounce:" => metrics.bounce_kb = value,
                    "WritebackTmp:" => metrics.writeback_tmp_kb = value,
                    "CommitLimit:" => metrics.commit_limit_kb = value,
                    "Committed_AS:" => metrics.committed_as_kb = value,
                    "VmallocTotal:" => metrics.vmalloc_total_kb = value,
                    "VmallocUsed:" => metrics.vmalloc_used_kb = value,
                    "VmallocChunk:" => metrics.vmalloc_chunk_kb = value,
                    "Percpu:" => metrics.percpu_kb = value,
                    "HardwareCorrupted:" => metrics.hardware_corrupted_kb = value,
                    "AnonHugePages:" => metrics.anon_huge_pages_kb = value,
                    "ShmemHugePages:" => metrics.shmem_huge_pages_kb = value,
                    "ShmemPmdMapped:" => metrics.shmem_pmd_mapped_kb = value,
                    "FileHugePages:" => metrics.file_huge_pages_kb = value,
                    "FilePmdMapped:" => metrics.file_pmd_mapped_kb = value,
                    "CmaTotal:" => metrics.cma_total_kb = value,
                    "CmaFree:" => metrics.cma_free_kb = value,
                    _ => {}
                }
            }
        }
    }
    
    Ok(metrics)
}

fn read_thermal_stats() -> Result<ThermalMetrics, String> {
    let mut metrics = ThermalMetrics {
        temperature_celsius: 0.0,
        thermal_zones: Vec::new(),
        throttling_level: 0,
        throttling_start_time: None,
        throttling_duration_secs: 0,
        cooling_device_state: Vec::new(),
        max_temp_reached: 0.0,
        min_temp_reached: 100.0,
        avg_temp_last_minute: 0.0,
    };
    
    // Чтение термальных зон
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.filter_map(Result::ok) {
            if entry.file_name().to_string_lossy().starts_with("thermal_zone") {
                let zone_path = entry.path();
                let name = zone_path.file_name().unwrap().to_string_lossy().to_string();
                
                let mut zone = ThermalZone {
                    name: name.clone(),
                    type_: fs::read_to_string(zone_path.join("type")).unwrap_or_default().trim().to_string(),
                    temperature_celsius: 0.0,
                    policy: fs::read_to_string(zone_path.join("policy")).unwrap_or_default().trim().to_string(),
                    available_governors: Vec::new(),
                    governor: fs::read_to_string(zone_path.join("governor")).unwrap_or_default().trim().to_string(),
                    trip_points: Vec::new(),
                };
                
                if let Ok(temp) = fs::read_to_string(zone_path.join("temp")) {
                    zone.temperature_celsius = temp.trim().parse::<f32>().unwrap_or(0.0) / 1000.0;
                }
                
                // Чтение trip points
                if let Ok(trip_entries) = fs::read_dir(&zone_path) {
                    for trip in trip_entries.filter_map(Result::ok) {
                        let trip_name = trip.file_name().to_string_lossy().to_string();
                        if trip_name.starts_with("trip_point_") {
                            let trip_path = trip.path();
                            let mut trip_point = TripPoint {
                                type_: fs::read_to_string(trip_path.join("type")).unwrap_or_default().trim().to_string(),
                                temperature_celsius: 0.0,
                                hysteresis_celsius: 0.0,
                            };
                            
                            if let Ok(temp) = fs::read_to_string(trip_path.join("temp")) {
                                trip_point.temperature_celsius = temp.trim().parse::<f32>().unwrap_or(0.0) / 1000.0;
                            }
                            
                            if let Ok(hyst) = fs::read_to_string(trip_path.join("hyst")) {
                                trip_point.hysteresis_celsius = hyst.trim().parse::<f32>().unwrap_or(0.0) / 1000.0;
                            }
                // tests/mobile_stress_advanced/mod.rs (продолжение)

                            if let Ok(hyst) = fs::read_to_string(trip_path.join("hyst")) {
                                trip_point.hysteresis_celsius = hyst.trim().parse::<f32>().unwrap_or(0.0) / 1000.0;
                            }
                            
                            zone.trip_points.push(trip_point);
                        }
                    }
                }
                
                metrics.thermal_zones.push(zone);
                
                // Обновляем общую температуру
                if zone.temperature_celsius > metrics.temperature_celsius {
                    metrics.temperature_celsius = zone.temperature_celsius;
                }
                
                // Отслеживаем мин/макс
                if zone.temperature_celsius > metrics.max_temp_reached {
                    metrics.max_temp_reached = zone.temperature_celsius;
                }
                if zone.temperature_celsius < metrics.min_temp_reached && zone.temperature_celsius > 0.0 {
                    metrics.min_temp_reached = zone.temperature_celsius;
                }
            }
        }
    }
    
    // Чтение cooling devices
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.filter_map(Result::ok) {
            if entry.file_name().to_string_lossy().starts_with("cooling_device") {
                let device_path = entry.path();
                let mut device = CoolingDevice {
                    name: entry.file_name().to_string_lossy().to_string(),
                    type_: fs::read_to_string(device_path.join("type")).unwrap_or_default().trim().to_string(),
                    max_state: fs::read_to_string(device_path.join("max_state")).unwrap_or_default().trim().parse().unwrap_or(0),
                    cur_state: fs::read_to_string(device_path.join("cur_state")).unwrap_or_default().trim().parse().unwrap_or(0),
                };
                metrics.cooling_device_state.push(device);
            }
        }
    }
    
    // Проверка троттлинга
    if metrics.temperature_celsius > 60.0 {
        metrics.throttling_level = 1;
        if metrics.throttling_start_time.is_none() {
            metrics.throttling_start_time = Some(Utc::now());
        }
    } else if metrics.throttling_start_time.is_some() {
        if let Some(start) = metrics.throttling_start_time {
            metrics.throttling_duration_secs = (Utc::now() - start).num_seconds() as u64;
        }
        metrics.throttling_level = 0;
    }
    
    Ok(metrics)
}

fn read_battery_stats() -> Result<BatteryMetrics, String> {
    let mut metrics = BatteryMetrics {
        level_percent: 0.0,
        temperature_celsius: 0.0,
        voltage_uv: 0,
        current_ua: 0,
        capacity_ah: 0.0,
        charge_counter_ah: 0.0,
        energy_now_wh: 0.0,
        energy_full_wh: 0.0,
        power_now_mw: 0,
        status: "Unknown".to_string(),
        health: "Unknown".to_string(),
        technology: "Unknown".to_string(),
        cycle_count: 0,
        serial_number: "Unknown".to_string(),
        manufacture_date: "Unknown".to_string(),
        temperature_history: VecDeque::new(),
        current_history: VecDeque::new(),
    };
    
    let battery_path = "/sys/class/power_supply/battery";
    
    let files = vec![
        ("capacity", |v: &str, m: &mut BatteryMetrics| m.level_percent = v.parse().unwrap_or(0.0)),
        ("temp", |v: &str, m: &mut BatteryMetrics| m.temperature_celsius = v.parse::<f32>().unwrap_or(0.0) / 10.0),
        ("voltage_now", |v: &str, m: &mut BatteryMetrics| m.voltage_uv = v.parse().unwrap_or(0)),
        ("current_now", |v: &str, m: &mut BatteryMetrics| m.current_ua = v.parse().unwrap_or(0)),
        ("charge_counter", |v: &str, m: &mut BatteryMetrics| m.charge_counter_ah = v.parse::<f32>().unwrap_or(0.0) / 1000.0 / 3600.0),
        ("energy_now", |v: &str, m: &mut BatteryMetrics| m.energy_now_wh = v.parse::<f32>().unwrap_or(0.0) / 1000.0 / 1000.0),
        ("energy_full", |v: &str, m: &mut BatteryMetrics| m.energy_full_wh = v.parse::<f32>().unwrap_or(0.0) / 1000.0 / 1000.0),
        ("power_now", |v: &str, m: &mut BatteryMetrics| m.power_now_mw = v.parse().unwrap_or(0)),
        ("status", |v: &str, m: &mut BatteryMetrics| m.status = v.trim().to_string()),
        ("health", |v: &str, m: &mut BatteryMetrics| m.health = v.trim().to_string()),
        ("technology", |v: &str, m: &mut BatteryMetrics| m.technology = v.trim().to_string()),
        ("cycle_count", |v: &str, m: &mut BatteryMetrics| m.cycle_count = v.parse().unwrap_or(0)),
        ("serial_number", |v: &str, m: &mut BatteryMetrics| m.serial_number = v.trim().to_string()),
    ];
    
    for (file, updater) in files {
        let path = format!("{}/{}", battery_path, file);
        if let Ok(content) = fs::read_to_string(path) {
            updater(content.trim(), &mut metrics);
        }
    }
    
    Ok(metrics)
}

fn read_io_stats() -> Result<(u64, u64), String> {
    let mut reads = 0;
    let mut writes = 0;
    
    if let Ok(stat) = fs::read_to_string("/proc/diskstats") {
        for line in stat.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 14 {
                if let Ok(read_sectors) = parts[5].parse::<u64>() {
                    reads += read_sectors * 512;
                }
                if let Ok(write_sectors) = parts[9].parse::<u64>() {
                    writes += write_sectors * 512;
                }
            }
        }
    }
    
    Ok((reads, writes))
}

fn read_network_stats() -> Result<(u64, u64), String> {
    let mut rx = 0;
    let mut tx = 0;
    
    if let Ok(stat) = fs::read_to_string("/proc/net/dev") {
        for line in stat.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                if let Ok(rx_bytes) = parts[1].parse::<u64>() {
                    rx += rx_bytes;
                }
                if let Ok(tx_bytes) = parts[9].parse::<u64>() {
                    tx += tx_bytes;
                }
            }
        }
    }
    
    Ok((rx, tx))
}

fn read_system_stats() -> Result<SystemMetrics, String> {
    let mut metrics = SystemMetrics {
        uptime_secs: 0,
        load_average_1: 0.0,
        load_average_5: 0.0,
        load_average_15: 0.0,
        total_processes: 0,
        running_processes: 0,
        total_threads: 0,
        open_files: 0,
        file_descriptors: 0,
        inodes_used: 0,
        inodes_total: 0,
        disk_reads: 0,
        disk_writes: 0,
        disk_read_bytes: 0,
        disk_write_bytes: 0,
        disk_io_time_ms: 0,
        network_rx_bytes: 0,
        network_tx_bytes: 0,
        network_rx_packets: 0,
        network_tx_packets: 0,
        network_rx_errors: 0,
        network_tx_errors: 0,
        network_rx_dropped: 0,
        network_tx_dropped: 0,
    };
    
    // Uptime
    if let Ok(uptime) = fs::read_to_string("/proc/uptime") {
        if let Some(secs) = uptime.split_whitespace().next() {
            metrics.uptime_secs = secs.parse::<f64>().unwrap_or(0.0) as u64;
        }
    }
    
    // Load average
    if let Ok(loadavg) = fs::read_to_string("/proc/loadavg") {
        let parts: Vec<&str> = loadavg.split_whitespace().collect();
        if parts.len() >= 3 {
            metrics.load_average_1 = parts[0].parse().unwrap_or(0.0);
            metrics.load_average_5 = parts[1].parse().unwrap_or(0.0);
            metrics.load_average_15 = parts[2].parse().unwrap_or(0.0);
        }
    }
    
    // Process counts
    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        for line in stat.lines() {
            if line.starts_with("processes ") {
                metrics.total_processes = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            } else if line.starts_with("procs_running ") {
                metrics.running_processes = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            }
        }
    }
    
    // Thread count
    if let Ok(threads) = fs::read_to_string("/proc/sys/kernel/threads-max") {
        metrics.total_threads = threads.trim().parse().unwrap_or(0);
    }
    
    // File descriptors
    if let Ok(fd_dir) = fs::read_dir("/proc/self/fd") {
        metrics.file_descriptors = fd_dir.count();
    }
    
    Ok(metrics)
}

fn read_device_info() -> Result<DeviceInfo, String> {
    let mut info = DeviceInfo::default();
    
    // Чтение свойств устройства
    let props = vec![
        ("ro.product.manufacturer", &mut info.manufacturer),
        ("ro.product.model", &mut info.model),
        ("ro.build.version.release", &mut info.android_version),
        ("ro.kernel.version", &mut info.kernel_version),
    ];
    
    for (prop, field) in props {
        if let Ok(value) = read_property(prop) {
            *field = value;
        }
    }
    
    // CPU cores
    info.cpu_cores = num_cpus::get();
    
    // CPU max freq
    if let Ok(freq) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq") {
        info.cpu_max_freq_mhz = freq.trim().parse().unwrap_or(0) / 1000;
    }
    
    // RAM
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(val) = line.split_whitespace().nth(1) {
                    info.total_ram_mb = val.parse::<u64>().unwrap_or(0) / 1024;
                }
                break;
            }
        }
    }
    
    // Storage
    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        // Примерная оценка
        info.total_storage_mb = 128 * 1024;
    }
    
    // Battery capacity
    if let Ok(capacity) = fs::read_to_string("/sys/class/power_supply/battery/charge_full_design") {
        info.battery_capacity_mah = capacity.trim().parse().unwrap_or(0) / 1000;
    }
    
    Ok(info)
}

fn read_property(prop: &str) -> Result<String, String> {
    let output = Command::new("getprop")
        .arg(prop)
        .output()
        .map_err(|e| e.to_string())?;
    
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn black_box<T>(x: T) -> T {
    unsafe {
        let ret = std::ptr::read_volatile(&x);
        std::mem::forget(x);
        ret
    }
}

// ============= Главный тест =============

#[test]
fn test_advanced_mobile_stress() {
    println!("{}", "=".repeat(80));
    println!("{:^80}", "ПРОДВИНУТЫЙ СТРЕСС-ТЕСТ МОБИЛЬНОГО УСТРОЙСТВА");
    println!("{}", "=".repeat(80));
    
    let config = StressTestConfig {
        name: "Mobile Stress Test Suite".to_string(),
        description: "Полное стресс-тестирование всех компонентов".to_string(),
        duration: Duration::from_secs(60),
        intensity: StressIntensity::Heavy,
        components: vec![
            SystemComponent::CPU,
            SystemComponent::Memory,
            SystemComponent::Filesystem,
            SystemComponent::Network,
            SystemComponent::GPU,
            SystemComponent::Sensors,
            SystemComponent::Battery,
            SystemComponent::Thermal,
            SystemComponent::IO,
        ],
        thresholds: ThresholdConfig {
            max_cpu_temp_celsius: 70.0,
            max_battery_temp_celsius: 45.0,
            max_cpu_usage_percent: 90.0,
            max_memory_usage_mb: 1024,
            max_disk_usage_percent: 90.0,
            min_fps: 30.0,
            max_frame_time_ms: 33.0,
            max_battery_drain_percent_per_minute: 5.0,
            max_thermal_throttling_seconds: 10,
            max_process_count: 500,
            max_thread_count: 2000,
            max_open_files: 1000,
            min_network_speed_kbps: 100,
            max_network_latency_ms: 200,
        },
        safety_limits: SafetyLimits {
            max_temperature_celsius: 80.0,
            max_battery_drain_percent: 20.0,
            max_memory_pressure_mb: 1500,
            max_disk_write_gb: 10,
            enable_emergency_stop: true,
            auto_recover: true,
            recovery_timeout_secs: 30,
            max_consecutive_failures: 5,
        },
        profile: TestProfile::Mixed,
        reporting: ReportingConfig {
            realtime_updates: true,
            save_metrics_interval_secs: 5,
            generate_html_report: true,
            upload_to_cloud: false,
            notify_on_threshold: true,
            screenshot_on_failure: true,
            record_video: false,
            log_level: LogLevel::Info,
        },
    };
    
    let mut generator = AdvancedStressGenerator::new(config);
    
    match generator.run() {
        Ok(report) => {
            println!("\n{}", "=".repeat(80));
            println!("{}", style("РЕЗУЛЬТАТЫ ТЕСТА").cyan().bold());
            println!("{}", "=".repeat(80));
            
            println!("Тест: {}", report.test_name);
            println!("Статус: {}", if report.passed { 
                style("ПРОЙДЕН").green() 
            } else { 
                style("ПРОВАЛЕН").red() 
            });
            println!("Длительность: {:?}", report.duration);
            println!("\nСтатистика:");
            println!("  Всего операций: {}", report.summary.total_operations);
            println!("  Операций/сек: {:.2}", report.summary.operations_per_second);
            println!("  Ошибок: {}", report.summary.total_errors);
            println!("  Частота ошибок: {:.2}%", report.summary.error_rate * 100.0);
            
            println!("\nCPU:");
            println!("  Средняя загрузка: {:.1}%", report.summary.avg_cpu_usage);
            println!("  Макс загрузка: {:.1}%", report.summary.max_cpu_usage);
            
            println!("\nПамять:");
            println!("  Среднее использование: {:.1}MB", report.summary.avg_memory_mb);
            println!("  Макс использование: {:.1}MB", report.summary.max_memory_mb);
            
            println!("\nТемпература:");
            println!("  Средняя: {:.1}°C", report.summary.avg_temperature);
            println!("  Макс: {:.1}°C", report.summary.max_temperature);
            println!("  Мин: {:.1}°C", report.summary.min_temperature);
            
            println!("\nБатарея:");
            println!("  Начальный уровень: {:.1}%", report.summary.battery_start_percent);
            println!("  Конечный уровень: {:.1}%", report.summary.battery_end_percent);
            println!("  Разряд: {:.1}%", report.summary.battery_drain_percent);
            
            if !report.thresholds_exceeded.is_empty() {
                println!("\n{} Превышены пороги:", style("⚠").yellow());
                for t in &report.thresholds_exceeded {
                    println!("  - {}", t);
                }
            }
            
            println!("\n{}", "=".repeat(80));
            
            assert!(report.passed, "Стресс-тест не пройден!");
        }
        Err(e) => {
            panic!("Ошибка выполнения стресс-теста: {}", e);
        }
    }
}

#[test]
fn test_cpu_stress_only() {
    let config = StressTestConfig {
        name: "CPU Only Stress Test".to_string(),
        description: "Тестирование только процессора".to_string(),
        duration: Duration::from_secs(30),
        intensity: StressIntensity::Extreme,
        components: vec![SystemComponent::CPU],
        ..Default::default()
    };
    
    let mut generator = AdvancedStressGenerator::new(config);
    let report = generator.run().expect("CPU stress test failed");
    assert!(report.passed, "CPU stress test failed");
}

#[test]
fn test_memory_stress_only() {
    let config = StressTestConfig {
        name: "Memory Only Stress Test".to_string(),
        description: "Тестирование только памяти".to_string(),
        duration: Duration::from_secs(30),
        intensity: StressIntensity::Heavy,
        components: vec![SystemComponent::Memory],
        ..Default::default()
    };
    
    let mut generator = AdvancedStressGenerator::new(config);
    let report = generator.run().expect("Memory stress test failed");
    assert!(report.passed, "Memory stress test failed");
}

#[test]
fn test_thermal_stress() {
    let config = StressTestConfig {
        name: "Thermal Stress Test".to_string(),
        description: "Проверка термального троттлинга".to_string(),
        duration: Duration::from_secs(120),
        intensity: StressIntensity::Extreme,
        components: vec![SystemComponent::CPU, SystemComponent::GPU],
        ..Default::default()
    };
    
    let mut generator = AdvancedStressGenerator::new(config);
    let report = generator.run().expect("Thermal stress test failed");
    
    println!("Термальные события: {}", report.summary.thermal_throttling_events);
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            name: "Default Stress Test".to_string(),
            description: "Базовый стресс-тест".to_string(),
            duration: Duration::from_secs(30),
            intensity: StressIntensity::Medium,
            components: vec![SystemComponent::All],
            thresholds: ThresholdConfig {
                max_cpu_temp_celsius: 70.0,
                max_battery_temp_celsius: 45.0,
                max_cpu_usage_percent: 90.0,
                max_memory_usage_mb: 1024,
                max_disk_usage_percent: 90.0,
                min_fps: 30.0,
                max_frame_time_ms: 33.0,
                max_battery_drain_percent_per_minute: 5.0,
                max_thermal_throttling_seconds: 10,
                max_process_count: 500,
                max_thread_count: 2000,
                max_open_files: 1000,
                min_network_speed_kbps: 100,
                max_network_latency_ms: 200,
            },
            safety_limits: SafetyLimits {
                max_temperature_celsius: 80.0,
                max_battery_drain_percent: 20.0,
                max_memory_pressure_mb: 1500,
                max_disk_write_gb: 10,
                enable_emergency_stop: true,
                auto_recover: true,
                recovery_timeout_secs: 30,
                max_consecutive_failures: 5,
            },
            profile: TestProfile::Mixed,
            reporting: ReportingConfig {
                realtime_updates: true,
                save_metrics_interval_secs: 5,
                generate_html_report: true,
                upload_to_cloud: false,
                notify_on_threshold: true,
                screenshot_on_failure: true,
                record_video: false,
                log_level: LogLevel::Info,
            },
        }
    }
}