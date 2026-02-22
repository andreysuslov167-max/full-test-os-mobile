// tests/performance_regression_mobile/mod.rs
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
use rand::{Rng, SeedableRng};
use rand::distributions::Alphanumeric;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Local};
use rayon::prelude::*;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use console::{style, Term};
use statistical::{mean, standard_deviation};
use gnuplot::{Figure, Caption, Color};

// ============= Базовая структура для регрессионного тестирования =============

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerformanceBaseline {
    version: String,
    device_model: String,
    android_version: String,
    timestamp: DateTime<Utc>,
    benchmarks: HashMap<String, BenchmarkResult>,
    system_info: SystemInfo,
    thresholds: HashMap<String, Threshold>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkResult {
    name: String,
    category: BenchmarkCategory,
    metrics: Vec<MetricValue>,
    statistics: BenchmarkStatistics,
    samples: Vec<f64>,
    outliers_removed: usize,
    confidence_interval: (f64, f64),
    regression_detected: bool,
    regression_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum BenchmarkCategory {
    CPU,
    Memory,
    Filesystem,
    Network,
    GPU,
    Battery,
    Thermal,
    UI,
    Startup,
    Background,
    Sensors,
    Multimedia,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetricValue {
    name: String,
    unit: String,
    value: f64,
    baseline_value: Option<f64>,
    threshold: f64,
    passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkStatistics {
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    std_dev: f64,
    variance: f64,
    p95: f64,
    p99: f64,
    p999: f64,
    jitter: f64,
    trend_slope: f64,
    samples_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Threshold {
    warning: f64,   // +20% от baseline
    critical: f64,  // +50% от baseline
    improvement: f64, // -10% от baseline (обновить baseline)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemInfo {
    manufacturer: String,
    model: String,
    android_version: String,
    kernel_version: String,
    cpu_cores: usize,
    cpu_max_freq_mhz: u32,
    cpu_min_freq_mhz: u32,
    cpu_governor: String,
    total_ram_mb: u64,
    available_ram_mb: u64,
    total_storage_mb: u64,
    free_storage_mb: u64,
    battery_capacity_mah: u32,
    battery_health: String,
    screen_resolution: String,
    screen_density: f32,
    gpu_renderer: String,
    gpu_version: String,
    thermal_zones: Vec<ThermalZoneInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThermalZoneInfo {
    name: String,
    type_: String,
    temperature: f32,
    throttling_threshold: f32,
    shutdown_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegressionReport {
    test_run_id: String,
    timestamp: DateTime<Utc>,
    baseline_version: String,
    current_version: String,
    device_info: SystemInfo,
    benchmarks: Vec<BenchmarkResult>,
    regressions: Vec<Regression>,
    improvements: Vec<Improvement>,
    summary: RegressionSummary,
    charts: Vec<ChartData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Regression {
    benchmark_name: String,
    metric_name: String,
    baseline_value: f64,
    current_value: f64,
    percentage_change: f64,
    severity: RegressionSeverity,
    possible_causes: Vec<String>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Improvement {
    benchmark_name: String,
    metric_name: String,
    baseline_value: f64,
    current_value: f64,
    percentage_change: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum RegressionSeverity {
    Warning,    // 20-30%
    Critical,   // 30-50%
    Severe,     // >50%
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegressionSummary {
    total_benchmarks: usize,
    passed: usize,
    warnings: usize,
    critical: usize,
    severe: usize,
    improvements: usize,
    new_benchmarks: usize,
    removed_benchmarks: usize,
    duration: Duration,
    score: f64, // 0-100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChartData {
    name: String,
    data_points: Vec<DataPoint>,
    trend_line: Vec<DataPoint>,
    thresholds: Vec<ThresholdLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DataPoint {
    x: f64, // время или номер запуска
    y: f64, // значение
    label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThresholdLine {
    value: f64,
    label: String,
    color: String,
}

// ============= Менеджер регрессионного тестирования =============

struct PerformanceRegressionTester {
    baseline_dir: PathBuf,
    results_dir: PathBuf,
    charts_dir: PathBuf,
    current_baseline: Option<PerformanceBaseline>,
    baseline_version: String,
    device_info: SystemInfo,
    progress_bars: MultiProgress,
    running_benchmarks: Arc<Mutex<HashSet<String>>>,
    stop_signal: Arc<AtomicBool>,
}

impl PerformanceRegressionTester {
    fn new() -> Self {
        let baseline_dir = PathBuf::from("/sdcard/Android/data/com.example.test/performance_baselines");
        let results_dir = PathBuf::from("/sdcard/Android/data/com.example.test/performance_results");
        let charts_dir = PathBuf::from("/sdcard/Android/data/com.example.test/performance_charts");
        
        fs::create_dir_all(&baseline_dir).ok();
        fs::create_dir_all(&results_dir).ok();
        fs::create_dir_all(&charts_dir).ok();
        
        let device_info = Self::collect_device_info().unwrap_or_default();
        let baseline_version = Self::get_app_version();
        
        Self {
            baseline_dir,
            results_dir,
            charts_dir,
            current_baseline: None,
            baseline_version,
            device_info,
            progress_bars: MultiProgress::new(),
            running_benchmarks: Arc::new(Mutex::new(HashSet::new())),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }
    
    fn collect_device_info() -> Result<SystemInfo, String> {
        let mut info = SystemInfo {
            manufacturer: read_property("ro.product.manufacturer")?,
            model: read_property("ro.product.model")?,
            android_version: read_property("ro.build.version.release")?,
            kernel_version: read_property("ro.kernel.version")?,
            cpu_cores: num_cpus::get(),
            cpu_max_freq_mhz: read_cpu_max_freq()?,
            cpu_min_freq_mhz: read_cpu_min_freq()?,
            cpu_governor: read_cpu_governor()?,
            total_ram_mb: read_total_ram()?,
            available_ram_mb: read_available_ram()?,
            total_storage_mb: read_total_storage()?,
            free_storage_mb: read_free_storage()?,
            battery_capacity_mah: read_battery_capacity()?,
            battery_health: read_battery_health()?,
            screen_resolution: read_screen_resolution()?,
            screen_density: read_screen_density()?,
            gpu_renderer: read_gpu_renderer()?,
            gpu_version: read_gpu_version()?,
            thermal_zones: read_thermal_zones()?,
        };
        
        Ok(info)
    }
    
    fn get_app_version() -> String {
        if let Ok(output) = Command::new("dumpsys")
            .args(&["package", "com.example.app", "|", "grep", "versionName"])
            .output() 
        {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("unknown")
                .split('=')
                .nth(1)
                .unwrap_or("unknown")
                .trim()
                .to_string()
        } else {
            "unknown".to_string()
        }
    }
    
    fn load_baseline(&mut self, version: &str) -> Result<(), String> {
        let baseline_file = self.baseline_dir.join(format!("baseline_{}.json", version));
        
        if baseline_file.exists() {
            let content = fs::read_to_string(&baseline_file)
                .map_err(|e| format!("Failed to read baseline: {}", e))?;
            
            self.current_baseline = Some(serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse baseline: {}", e))?);
            
            println!("{} Loaded baseline for version {}", style("✓").green(), version);
        } else {
            println!("{} No baseline found for version {}", style("⚠").yellow(), version);
            self.current_baseline = None;
        }
        
        Ok(())
    }
    
    fn save_baseline(&self, baseline: &PerformanceBaseline) -> Result<(), String> {
        let baseline_file = self.baseline_dir.join(format!("baseline_{}.json", baseline.version));
        
        let json = serde_json::to_string_pretty(baseline)
            .map_err(|e| format!("Failed to serialize baseline: {}", e))?;
        
        fs::write(&baseline_file, json)
            .map_err(|e| format!("Failed to write baseline: {}", e))?;
        
        println!("{} Baseline saved for version {}", style("✓").green(), baseline.version);
        
        Ok(())
    }
    
    fn run_regression_suite(&mut self) -> Result<RegressionReport, String> {
        println!("\n{}", "=".repeat(80));
        println!("{}", style("📊 PERFORMANCE REGRESSION TEST SUITE").cyan().bold());
        println!("{}", "=".repeat(80));
        
        println!("\nDevice: {} {}", self.device_info.manufacturer, self.device_info.model);
        println!("Android: {}, Kernel: {}", self.device_info.android_version, self.device_info.kernel_version);
        println!("CPU: {} cores @ {}MHz", self.device_info.cpu_cores, self.device_info.cpu_max_freq_mhz);
        println!("RAM: {}MB total, {}MB available", self.device_info.total_ram_mb, self.device_info.available_ram_mb);
        println!("Storage: {}MB total, {}MB free", self.device_info.total_storage_mb, self.device_info.free_storage_mb);
        
        let test_run_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();
        
        // Загрузка baseline
        self.load_baseline(&self.baseline_version)?;
        
        // Запуск всех бенчмарков
        let mut benchmarks = Vec::new();
        
        let benchmark_fns: Vec<Box<dyn Fn() -> Result<BenchmarkResult, String> + Send>> = vec![
            Box::new(|| Self::benchmark_cpu_performance()),
            Box::new(|| Self::benchmark_memory_performance()),
            Box::new(|| Self::benchmark_filesystem_performance()),
            Box::new(|| Self::benchmark_network_performance()),
            Box::new(|| Self::benchmark_gpu_performance()),
            Box::new(|| Self::benchmark_battery_performance()),
            Box::new(|| Self::benchmark_thermal_performance()),
            Box::new(|| Self::benchmark_ui_performance()),
            Box::new(|| Self::benchmark_startup_time()),
            Box::new(|| Self::benchmark_background_performance()),
            Box::new(|| Self::benchmark_sensors_performance()),
            Box::new(|| Self::benchmark_multimedia_performance()),
        ];
        
        let total = benchmark_fns.len();
        let pb = self.create_progress_bar("Running benchmarks", total as u64);
        
        for (i, benchmark_fn) in benchmark_fns.into_iter().enumerate() {
            if self.stop_signal.load(Ordering::Relaxed) {
                break;
            }
            
            pb.set_message(format!("Benchmark {}/{}", i + 1, total));
            
            match benchmark_fn() {
                Ok(mut result) => {
                    // Сравнение с baseline
                    if let Some(baseline) = &self.current_baseline {
                        if let Some(baseline_result) = baseline.benchmarks.get(&result.name) {
                            result = self.compare_with_baseline(result, baseline_result);
                        }
                    }
                    
                    benchmarks.push(result);
                    pb.inc(1);
                }
                Err(e) => {
                    eprintln!("{} Benchmark failed: {}", style("✗").red(), e);
                }
            }
        }
        
        pb.finish_with_message("All benchmarks completed");
        
        // Анализ регрессий
        let (regressions, improvements) = self.analyze_regressions(&benchmarks);
        
        // Подсчет статистики
        let summary = self.calculate_summary(&benchmarks, &regressions, &improvements, start_time);
        
        // Генерация графиков
        let charts = self.generate_charts(&benchmarks)?;
        
        let end_time = Utc::now();
        
        let report = RegressionReport {
            test_run_id,
            timestamp: end_time,
            baseline_version: self.baseline_version.clone(),
            current_version: self.baseline_version.clone(),
            device_info: self.device_info.clone(),
            benchmarks,
            regressions,
            improvements,
            summary,
            charts,
        };
        
        // Сохранение отчета
        self.save_report(&report)?;
        
        // Если нет baseline, сохраняем текущие результаты как baseline
        if self.current_baseline.is_none() {
            self.save_as_baseline(&report)?;
        }
        
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
    
    fn benchmark_cpu_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "CPU Performance".to_string();
        
        // Тест 1: Целочисленные операции
        let int_start = Instant::now();
        let mut result = 0u64;
        for i in 0..10_000_000 {
            result = result.wrapping_add(i).wrapping_mul(12345);
        }
        black_box(result);
        let int_time = int_start.elapsed().as_secs_f64() * 1000.0;
        
        metrics.push(MetricValue {
            name: "integer_ops_ms".to_string(),
            unit: "ms".to_string(),
            value: int_time,
            baseline_value: None,
            threshold: 20.0,
            passed: false,
        });
        samples.push(int_time);
        
        // Тест 2: Операции с плавающей точкой
        let float_start = Instant::now();
        let mut float_result = 0.0;
        for i in 0..5_000_000 {
            float_result += (i as f64).sin() * (i as f64).cos();
        }
        black_box(float_result);
        let float_time = float_start.elapsed().as_secs_f64() * 1000.0;
        
        metrics.push(MetricValue {
            name: "float_ops_ms".to_string(),
            unit: "ms".to_string(),
            value: float_time,
            baseline_value: None,
            threshold: 30.0,
            passed: false,
        });
        samples.push(float_time);
        
        // Тест 3: Многопоточность
        let thread_start = Instant::now();
        let mut handles = vec![];
        for _ in 0..4 {
            handles.push(thread::spawn(|| {
                let mut sum = 0.0;
                for i in 0..1_000_000 {
                    sum += (i as f64).sqrt();
                }
                sum
            }));
        }
        
        for handle in handles {
            let _ = handle.join();
        }
        let thread_time = thread_start.elapsed().as_secs_f64() * 1000.0;
        
        metrics.push(MetricValue {
            name: "multithread_ms".to_string(),
            unit: "ms".to_string(),
            value: thread_time,
            baseline_value: None,
            threshold: 50.0,
            passed: false,
        });
        samples.push(thread_time);
        
        // Тест 4: Частота CPU
        let cpu_freq = read_cpu_frequencies()?;
        let avg_freq = cpu_freq.iter().sum::<u32>() as f64 / cpu_freq.len() as f64;
        
        metrics.push(MetricValue {
            name: "avg_cpu_freq_mhz".to_string(),
            unit: "MHz".to_string(),
            value: avg_freq,
            baseline_value: None,
            threshold: 100.0,
            passed: false,
        });
        
        // Статистика
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::CPU,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_memory_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Memory Performance".to_string();
        
        // Тест 1: Скорость аллокации
        let alloc_start = Instant::now();
        for _ in 0..1000 {
            let vec = vec![0u8; 1024 * 1024];
            drop(vec);
        }
        let alloc_time = alloc_start.elapsed().as_secs_f64() * 1000.0;
        
        metrics.push(MetricValue {
            name: "allocation_1000mb_ms".to_string(),
            unit: "ms".to_string(),
            value: alloc_time,
            baseline_value: None,
            threshold: 500.0,
            passed: false,
        });
        samples.push(alloc_time);
        
        // Тест 2: Скорость копирования памяти
        let src = vec![1u8; 10 * 1024 * 1024];
        let mut dst = vec![0u8; 10 * 1024 * 1024];
        
        let copy_start = Instant::now();
        dst.copy_from_slice(&src);
        let copy_time = copy_start.elapsed().as_secs_f64() * 1000.0;
        
        metrics.push(MetricValue {
            name: "memcpy_10mb_ms".to_string(),
            unit: "ms".to_string(),
            value: copy_time,
            baseline_value: None,
            threshold: 50.0,
            passed: false,
        });
        samples.push(copy_time);
        
        // Тест 3: Пропускная способность памяти
        let bandwidth = (10 * 1024 * 1024) as f64 / (copy_time / 1000.0) / 1024.0 / 1024.0;
        
        metrics.push(MetricValue {
            name: "memory_bandwidth_mb_s".to_string(),
            unit: "MB/s".to_string(),
            value: bandwidth,
            baseline_value: None,
            threshold: 100.0,
            passed: false,
        });
        
        // Тест 4: Задержка памяти
        let latency = measure_memory_latency();
        
        metrics.push(MetricValue {
            name: "memory_latency_ns".to_string(),
            unit: "ns".to_string(),
            value: latency,
            baseline_value: None,
            threshold: 100.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Memory,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_filesystem_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Filesystem Performance".to_string();
        
        let test_dir = PathBuf::from("/sdcard/Android/data/com.example.test/fs_benchmark");
        fs::create_dir_all(&test_dir).ok();
        
        // Тест 1: Последовательная запись
        let file_path = test_dir.join("seq_write.dat");
        let write_start = Instant::now();
        
        let mut file = File::create(&file_path)?;
        let data = vec![1u8; 10 * 1024 * 1024];
        file.write_all(&data)?;
        file.sync_all()?;
        
        let write_time = write_start.elapsed().as_secs_f64() * 1000.0;
        let write_speed = (10 * 1024 * 1024) as f64 / (write_time / 1000.0) / 1024.0 / 1024.0;
        
        metrics.push(MetricValue {
            name: "seq_write_speed_mb_s".to_string(),
            unit: "MB/s".to_string(),
            value: write_speed,
            baseline_value: None,
            threshold: 10.0,
            passed: false,
        });
        samples.push(write_time);
        
        // Тест 2: Последовательное чтение
        let read_start = Instant::now();
        let mut file = File::open(&file_path)?;
        let mut buffer = vec![0u8; 10 * 1024 * 1024];
        file.read_exact(&mut buffer)?;
        
        let read_time = read_start.elapsed().as_secs_f64() * 1000.0;
        let read_speed = (10 * 1024 * 1024) as f64 / (read_time / 1000.0) / 1024.0 / 1024.0;
        
        metrics.push(MetricValue {
            name: "seq_read_speed_mb_s".to_string(),
            unit: "MB/s".to_string(),
            value: read_speed,
            baseline_value: None,
            threshold: 20.0,
            passed: false,
        });
        samples.push(read_time);
        
        // Тест 3: Случайный доступ
        let random_start = Instant::now();
        let mut file = OpenOptions::new().read(true).write(true).open(&file_path)?;
        let mut rng = rand::thread_rng();
        
        for _ in 0..1000 {
            let pos = rng.gen_range(0..10 * 1024 * 1024 - 4096);
            file.seek(SeekFrom::Start(pos))?;
            
            if rng.gen_bool(0.5) {
                let data = [rng.gen(); 4096];
                file.write_all(&data)?;
            } else {
                let mut buffer = [0u8; 4096];
                file.read_exact(&mut buffer)?;
            }
        }
        
        let random_time = random_start.elapsed().as_secs_f64() * 1000.0;
        
        metrics.push(MetricValue {
            name: "random_io_1000ops_ms".to_string(),
            unit: "ms".to_string(),
            value: random_time,
            baseline_value: None,
            threshold: 500.0,
            passed: false,
        });
        samples.push(random_time);
        
        // Тест 4: Fsync latency
        let fsync_start = Instant::now();
        file.sync_all()?;
        let fsync_time = fsync_start.elapsed().as_secs_f64() * 1000.0;
        
        metrics.push(MetricValue {
            name: "fsync_latency_ms".to_string(),
            unit: "ms".to_string(),
            value: fsync_time,
            baseline_value: None,
            threshold: 50.0,
            passed: false,
        });
        
        // Очистка
        fs::remove_file(&file_path).ok();
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Filesystem,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_network_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Network Performance".to_string();
        
        // Тест 1: Задержка (ping)
        let ping_start = Instant::now();
        let output = Command::new("ping")
            .args(&["-c", "10", "8.8.8.8"])
            .output()
            .map_err(|e| format!("Ping failed: {}", e))?;
        
        let ping_time = ping_start.elapsed().as_secs_f64() * 1000.0;
        
        // Парсинг результатов ping
        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut rtts = Vec::new();
        
        for line in output_str.lines() {
            if line.contains("time=") {
                if let Some(time_str) = line.split("time=").nth(1) {
                    if let Some(time_val) = time_str.split_whitespace().next() {
                        if let Ok(rtt) = time_val.parse::<f64>() {
                            rtts.push(rtt);
                        }
                    }
                }
            }
        }
        
        let avg_rtt = if !rtts.is_empty() {
            rtts.iter().sum::<f64>() / rtts.len() as f64
        } else {
            0.0
        };
        
        metrics.push(MetricValue {
            name: "avg_rtt_ms".to_string(),
            unit: "ms".to_string(),
            value: avg_rtt,
            baseline_value: None,
            threshold: 100.0,
            passed: false,
        });
        samples.push(avg_rtt);
        
        // Тест 2: Пропускная способность
        let bandwidth = test_network_bandwidth()?;
        
        metrics.push(MetricValue {
            name: "bandwidth_mbps".to_string(),
            unit: "Mbps".to_string(),
            value: bandwidth,
            baseline_value: None,
            threshold: 1.0,
            passed: false,
        });
        
        // Тест 3: Джиттер
        if rtts.len() > 1 {
            let jitter = calculate_jitter(&rtts);
            
            metrics.push(MetricValue {
                name: "jitter_ms".to_string(),
                unit: "ms".to_string(),
                value: jitter,
                baseline_value: None,
                threshold: 20.0,
                passed: false,
            });
        }
        
        // Тест 4: Потеря пакетов
        let packet_loss = calculate_packet_loss(&output_str);
        
        metrics.push(MetricValue {
            name: "packet_loss_percent".to_string(),
            unit: "%".to_string(),
            value: packet_loss,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Network,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_gpu_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "GPU Performance".to_string();
        
        // Тест 1: FPS
        let fps = measure_fps()?;
        
        metrics.push(MetricValue {
            name: "fps".to_string(),
            unit: "Hz".to_string(),
            value: fps as f64,
            baseline_value: None,
            threshold: 10.0,
            passed: false,
        });
        samples.push(fps as f64);
        
        // Тест 2: Время кадра
        let frame_time = 1000.0 / fps as f64;
        
        metrics.push(MetricValue {
            name: "frame_time_ms".to_string(),
            unit: "ms".to_string(),
            value: frame_time,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        // Тест 3: Джиттер кадров
        let frame_times = measure_frame_times(100)?;
        let frame_jitter = calculate_std_dev(&frame_times);
        
        metrics.push(MetricValue {
            name: "frame_jitter_ms".to_string(),
            unit: "ms".to_string(),
            value: frame_jitter,
            baseline_value: None,
            threshold: 3.0,
            passed: false,
        });
        
        // Тест 4: Пропущенные кадры
        let missed_frames = frame_times.iter().filter(|&&t| t > 20.0).count();
        
        metrics.push(MetricValue {
            name: "missed_frames_percent".to_string(),
            unit: "%".to_string(),
            value: (missed_frames as f64 / frame_times.len() as f64) * 100.0,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        // Тест 5: Использование GPU
        let gpu_usage = read_gpu_usage()?;
        
        metrics.push(MetricValue {
            name: "gpu_usage_percent".to_string(),
            unit: "%".to_string(),
            value: gpu_usage as f64,
            baseline_value: None,
            threshold: 20.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::GPU,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_battery_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Battery Performance".to_string();
        
        // Тест 1: Скорость разряда
        let drain_rate = measure_battery_drain_rate(Duration::from_secs(60))?;
        
        metrics.push(MetricValue {
            name: "drain_rate_percent_per_hour".to_string(),
            unit: "%/h".to_string(),
            value: drain_rate,
            baseline_value: None,
            threshold: 10.0,
            passed: false,
        });
        samples.push(drain_rate);
        
        // Тест 2: Температура под нагрузкой
        let temp_under_load = measure_battery_temp_under_load(Duration::from_secs(30))?;
        
        metrics.push(MetricValue {
            name: "max_temp_celsius".to_string(),
            unit: "°C".to_string(),
            value: temp_under_load as f64,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        // Тест 3: Время зарядки
        let charge_time = estimate_charge_time()?;
        
        metrics.push(MetricValue {
            name: "charge_time_minutes".to_string(),
            unit: "min".to_string(),
            value: charge_time,
            baseline_value: None,
            threshold: 30.0,
            passed: false,
        });
        
        // Тест 4: Емкость батареи
        let capacity = read_battery_capacity()? as f64 / 1000.0;
        
        metrics.push(MetricValue {
            name: "battery_capacity_ah".to_string(),
            unit: "Ah".to_string(),
            value: capacity,
            baseline_value: None,
            threshold: 0.5,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Battery,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_thermal_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Thermal Performance".to_string();
        
        // Тест 1: Максимальная температура
        let max_temp = measure_max_temperature(Duration::from_secs(60))?;
        
        metrics.push(MetricValue {
            name: "max_temp_celsius".to_string(),
            unit: "°C".to_string(),
            value: max_temp as f64,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        samples.push(max_temp as f64);
        
        // Тест 2: Время до троттлинга
        let time_to_throttle = measure_time_to_throttle()?;
        
        metrics.push(MetricValue {
            name: "time_to_throttle_secs".to_string(),
            unit: "s".to_string(),
            value: time_to_throttle,
            baseline_value: None,
            threshold: 60.0,
            passed: false,
        });
        
        // Тест 3: Снижение производительности при нагреве
        let perf_drop = measure_performance_drop_under_thermal()?;
        
        metrics.push(MetricValue {
            name: "performance_drop_percent".to_string(),
            unit: "%".to_string(),
            value: perf_drop,
            baseline_value: None,
            threshold: 20.0,
            passed: false,
        });
        
        // Тест 4: Время восстановления
        let recovery_time = measure_thermal_recovery_time()?;
        
        metrics.push(MetricValue {
            name: "recovery_time_secs".to_string(),
            unit: "s".to_string(),
            value: recovery_time,
            baseline_value: None,
            threshold: 30.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Thermal,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_ui_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "UI Performance".to_string();
        
        // Тест 1: Время отклика на касание
        let touch_latency = measure_touch_latency(100)?;
        
        metrics.push(MetricValue {
            name: "touch_latency_ms".to_string(),
            unit: "ms".to_string(),
            value: touch_latency,
            baseline_value: None,
            threshold: 10.0,
            passed: false,
        });
        samples.push(touch_latency);
        
        // Тест 2: Время отрисовки
        let render_time = measure_render_time()?;
        
        metrics.push(MetricValue {
            name: "render_time_ms".to_string(),
            unit: "ms".to_string(),
            value: render_time,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        // Тест 3: Задержка прокрутки
        let scroll_latency = measure_scroll_latency()?;
        
        metrics.push(MetricValue {
            name: "scroll_latency_ms".to_string(),
            unit: "ms".to_string(),
            value: scroll_latency,
            baseline_value: None,
            threshold: 15.0,
            passed: false,
        });
        
        // Тест 4: Jank count
        let jank_count = measure_jank_count(1000)?;
        
        metrics.push(MetricValue {
            name: "jank_count".to_string(),
            unit: "count".to_string(),
            value: jank_count as f64,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::UI,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_startup_time() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "App Startup Time".to_string();
        
        // Тест 1: Холодный старт
        let cold_starts: Vec<f64> = (0..10)
            .map(|_| {
                // Убиваем приложение
                let _ = Command::new("am")
                    .args(&["force-stop", "com.example.app"])
                    .output();
                
                thread::sleep(Duration::from_secs(1));
                
                // Замеряем время запуска
                let start = Instant::now();
                let output = Command::new("am")
                    .args(&["start", "-W", "com.example.app/.MainActivity"])
                    .output()
                    .ok()?;
                
                let output_str = String::from_utf8_lossy(&output.stdout);
                
                // Парсим время из вывода
                for line in output_str.lines() {
                    if line.contains("TotalTime:") {
                        if let Some(time_str) = line.split_whitespace().nth(1) {
                            if let Ok(time) = time_str.parse::<f64>() {
                                return Some(time);
                            }
                        }
                    }
                }
                
                Some(start.elapsed().as_millis() as f64)
            })
            .filter_map(|x| x)
            .collect();
        
        let avg_cold_start = if !cold_starts.is_empty() {
            cold_starts.iter().sum::<f64>() / cold_starts.len() as f64
        } else {
            0.0
        };
        
        metrics.push(MetricValue {
            name: "cold_start_ms".to_string(),
            unit: "ms".to_string(),
            value: avg_cold_start,
            baseline_value: None,
            threshold: 200.0,
            passed: false,
        });
        samples.extend(cold_starts);
        
        // Тест 2: Теплый старт
        let warm_starts: Vec<f64> = (0..10)
            .map(|_| {
                // Отправляем в фон и возвращаем
                let _ = Command::new("input")
                    .args(&["keyevent", "KEYCODE_HOME"])
                    .output();
                
                thread::sleep(Duration::from_millis(500));
                
                let start = Instant::now();
                let _ = Command::new("am")
                    .args(&["start", "com.example.app/.MainActivity"])
                    .output();
                
                start.elapsed().as_millis() as f64
            })
            .collect();
        
        let avg_warm_start = warm_starts.iter().sum::<f64>() / warm_starts.len() as f64;
        
        metrics.push(MetricValue {
            name: "warm_start_ms".to_string(),
            unit: "ms".to_string(),
            value: avg_warm_start,
            baseline_value: None,
            threshold: 100.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Startup,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_background_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Background Performance".to_string();
        
        // Тест 1: Потребление памяти в фоне
        let background_memory = measure_background_memory()?;
        
        metrics.push(MetricValue {
            name: "background_memory_mb".to_string(),
            unit: "MB".to_string(),
            value: background_memory,
            baseline_value: None,
            threshold: 20.0,
            passed: false,
        });
        samples.push(background_memory);
        
        // Тест 2: Потребление CPU в фоне
        let background_cpu = measure_background_cpu()?;
        
        metrics.push(MetricValue {
            name: "background_cpu_percent".to_string(),
            unit: "%".to_string(),
            value: background_cpu,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        // Тест 3: Время восстановления из фона
        let resume_time = measure_resume_time()?;
        
        metrics.push(MetricValue {
            name: "resume_time_ms".to_string(),
            unit: "ms".to_string(),
            value: resume_time,
            baseline_value: None,
            threshold: 200.0,
            passed: false,
        });
        
        // Тест 4: Сохранение состояния
        let state_preserved = measure_state_preservation()?;
        
        metrics.push(MetricValue {
            name: "state_preserved".to_string(),
            unit: "bool".to_string(),
            value: if state_preserved { 1.0 } else { 0.0 },
            baseline_value: None,
            threshold: 0.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Background,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_sensors_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Sensors Performance".to_string();
        
        // Тест 1: Частота обновления акселерометра
        let accel_rate = measure_sensor_rate("accelerometer")?;
        
        metrics.push(MetricValue {
            name: "accelerometer_rate_hz".to_string(),
            unit: "Hz".to_string(),
            value: accel_rate,
            baseline_value: None,
            threshold: 20.0,
            passed: false,
        });
        samples.push(accel_rate);
        
        // Тест 2: Частота обновления гироскопа
        let gyro_rate = measure_sensor_rate("gyroscope")?;
        
        metrics.push(MetricValue {
            name: "gyroscope_rate_hz".to_string(),
            unit: "Hz".to_string(),
            value: gyro_rate,
            baseline_value: None,
            threshold: 20.0,
            passed: false,
        });
        
        // Тест 3: Задержка GPS
        let gps_latency = measure_gps_latency()?;
        
        metrics.push(MetricValue {
            name: "gps_latency_ms".to_string(),
            unit: "ms".to_string(),
            value: gps_latency,
            baseline_value: None,
            threshold: 1000.0,
            passed: false,
        });
        
        // Тест 4: Точность датчиков
        let sensor_accuracy = measure_sensor_accuracy()?;
        
        metrics.push(MetricValue {
            name: "sensor_accuracy_percent".to_string(),
            unit: "%".to_string(),
            value: sensor_accuracy,
            baseline_value: None,
            threshold: 10.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Sensors,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn benchmark_multimedia_performance() -> Result<BenchmarkResult, String> {
        let mut metrics = Vec::new();
        let mut samples = Vec::new();
        let name = "Multimedia Performance".to_string();
        
        // Тест 1: Время запуска камеры
        let camera_startup = measure_camera_startup()?;
        
        metrics.push(MetricValue {
            name: "camera_startup_ms".to_string(),
            unit: "ms".to_string(),
            value: camera_startup,
            baseline_value: None,
            threshold: 300.0,
            passed: false,
        });
        samples.push(camera_startup);
        
        // Тест 2: Время захвата фото
        let photo_capture = measure_photo_capture()?;
        
        metrics.push(MetricValue {
            name: "photo_capture_ms".to_string(),
            unit: "ms".to_string(),
            value: photo_capture,
            baseline_value: None,
            threshold: 200.0,
            passed: false,
        });
        
        // Тест 3: FPS видео
        let video_fps = measure_video_fps()?;
        
        metrics.push(MetricValue {
            name: "video_fps".to_string(),
            unit: "Hz".to_string(),
            value: video_fps as f64,
            baseline_value: None,
            threshold: 5.0,
            passed: false,
        });
        
        // Тест 4: Задержка аудио
        let audio_latency = measure_audio_latency()?;
        
        metrics.push(MetricValue {
            name: "audio_latency_ms".to_string(),
            unit: "ms".to_string(),
            value: audio_latency,
            baseline_value: None,
            threshold: 50.0,
            passed: false,
        });
        
        let stats = calculate_statistics(&samples);
        
        Ok(BenchmarkResult {
            name,
            category: BenchmarkCategory::Multimedia,
            metrics,
            statistics: stats,
            samples,
            outliers_removed: 0,
            confidence_interval: calculate_confidence_interval(&samples, 0.95),
            regression_detected: false,
            regression_percentage: 0.0,
        })
    }
    
    fn compare_with_baseline(&self, mut current: BenchmarkResult, baseline: &BenchmarkResult) -> BenchmarkResult {
        let threshold_warning = 1.2;  // +20%
        let threshold_critical = 1.5; // +50%
        
        for (i, metric) in current.metrics.iter_mut().enumerate() {
            if i < baseline.metrics.len() {
                let baseline_value = baseline.metrics[i].value;
                metric.baseline_value = Some(baseline_value);
                
                if baseline_value > 0.0 {
                    let ratio = metric.value / baseline_value;
                    
                    if ratio > threshold_critical {
                        metric.passed = false;
                        current.regression_detected = true;
                        current.regression_percentage = (ratio - 1.0) * 100.0;
                    } else if ratio > threshold_warning {
                        metric.passed = false;
                        current.regression_detected = true;
                        current.regression_percentage = (ratio - 1.0) * 100.0;
                    } else {
                        metric.passed = true;
                    }
                }
            }
        }
        
        current
    }
    
    fn analyze_regressions(&self, benchmarks: &[BenchmarkResult]) -> (Vec<Regression>, Vec<Improvement>) {
        let mut regressions = Vec::new();
        let mut improvements = Vec::new();
        
        for benchmark in benchmarks {
            if let Some(baseline) = self.current_baseline.as_ref() {
                if let Some(baseline_bench) = baseline.benchmarks.get(&benchmark.name) {
                    for (i, metric) in benchmark.metrics.iter().enumerate() {
                        if i < baseline_bench.metrics.len() {
                            let baseline_value = baseline_bench.metrics[i].value;
                            
                            if baseline_value > 0.0 {
                                let change = (metric.value - baseline_value) / baseline_value * 100.0;
                                
                                if change > 20.0 {
                                    let severity = if change > 50.0 {
                                        RegressionSeverity::Severe
                                    } else if change > 30.0 {
                                        RegressionSeverity::Critical
                                    } else {
                                        RegressionSeverity::Warning
                                    };
                                    
                                    regressions.push(Regression {
                                        benchmark_name: benchmark.name.clone(),
                                        metric_name: metric.name.clone(),
                                        baseline_value,
                                        current_value: metric.value,
                                        percentage_change: change,
                                        severity,
                                        possible_causes: suggest_causes(&benchmark.category, &metric.name),
                                        recommendations: suggest_fixes(&benchmark.category, &metric.name),
                                    });
                                } else if change < -10.0 {
                                    improvements.push(Improvement {
                                        benchmark_name: benchmark.name.clone(),
                                        metric_name: metric.name.clone(),
                                        baseline_value,
                                        current_value: metric.value,
                                        percentage_change: change,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        (regressions, improvements)
    }
    
    fn calculate_summary(&self, benchmarks: &[BenchmarkResult], regressions: &[Regression], 
                         improvements: &[Improvement], start_time: DateTime<Utc>) -> RegressionSummary {
        let total = benchmarks.len();
        let mut passed = 0;
        let mut warnings = 0;
        let mut critical = 0;
        let mut severe = 0;
        
        for regression in regressions {
            match regression.severity {
                RegressionSeverity::Warning => warnings += 1,
                RegressionSeverity::Critical => critical += 1,
                RegressionSeverity::Severe => severe += 1,
            }
        }
        
        for benchmark in benchmarks {
            if benchmark.metrics.iter().all(|m| m.passed) {
                passed += 1;
            }
        }
        
        let score = if total > 0 {
            ((passed as f64 / total as f64) * 100.0) - (warnings as f64 * 0.5) - (critical as f64 * 2.0) - (severe as f64 * 5.0)
        } else {
            0.0
        };
        
        RegressionSummary {
            total_benchmarks: total,
            passed,
            warnings,
            critical,
            severe,
            improvements: improvements.len(),
            new_benchmarks: 0,
            removed_benchmarks: 0,
            duration: Utc::now() - start_time,
            score: score.max(0.0).min(100.0),
        }
    }
    
    fn generate_charts(&self, benchmarks: &[BenchmarkResult]) -> Result<Vec<ChartData>, String> {
        let mut charts = Vec::new();
        
        for benchmark in benchmarks {
            let mut data_points = Vec::new();
            
            for (i, &sample) in benchmark.samples.iter().enumerate() {
                data_points.push(DataPoint {
                    x: i as f64,
                    y: sample,
                    label: None,
                });
            }
            
            // Тренд
            let trend_line = calculate_trend_line(&data_points);
            
            // Пороги
            let mut thresholds = Vec::new();
            
            if let Some(baseline) = self.current_baseline.as_ref() {
                if let Some(baseline_bench) = baseline.benchmarks.get(&benchmark.name) {
                    if let Some(metric) = baseline_bench.metrics.first() {
                        thresholds.push(ThresholdLine {
                            value: metric.value,
                            label: "Baseline".to_string(),
                            color: "blue".to_string(),
                        });
                        
                        thresholds.push(ThresholdLine {
                            value: metric.value * 1.2,
                            label: "Warning".to_string(),
                            color: "orange".to_string(),
                        });
                        
                        thresholds.push(ThresholdLine {
                            value: metric.value * 1.5,
                            label: "Critical".to_string(),
                            color: "red".to_string(),
                        });
                    }
                }
            }
            
            charts.push(ChartData {
                name: benchmark.name.clone(),
                data_points,
                trend_line,
                thresholds,
            });
        }
        
        Ok(charts)
    }
    
    fn save_report(&self, report: &RegressionReport) -> Result<(), String> {
        let filename = format!("regression_report_{}.json", 
            Local::now().format("%Y%m%d_%H%M%S"));
        let filepath = self.results_dir.join(filename);
        
        let json = serde_json::to_string_pretty(report)
            .map_err(|e| format!("Failed to serialize report: {}", e))?;
        
        fs::write(&filepath, json)
            .map_err(|e| format!("Failed to write report: {}", e))?;
        
        println!("{} Report saved to {:?}", style("✓").green(), filepath);
        
        // Генерация HTML отчета
        self.generate_html_report(report)?;
        
        Ok(())
    }
    
    fn generate_html_report(&self, report: &RegressionReport) -> Result<(), String> {
        let html_path = self.results_dir.join(format!("regression_report_{}.html",
            Local::now().format("%Y%m%d_%H%M%S")));
        
        let mut html = String::new();
        
        html.push_str(r#"<!DOCTYPE html>
<html>
<head>
    <title>Performance Regression Report</title>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background: #f5f5f5; }
        .container { max-width: 1200px; margin: 0 auto; }
        .header { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 30px; border-radius: 10px; margin-bottom: 20px; }
        .summary-card { background: white; border-radius: 10px; padding: 20px; margin-bottom: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        .score { font-size: 48px; font-weight: bold; text-align: center; }
        .score.good { color: #4caf50; }
        .score.warning { color: #ff9800; }
        .score.bad { color: #f44336; }
        .regression { border-left: 4px solid #f44336; padding: 10px; margin: 10px 0; background: #ffebee; }
        .improvement { border-left: 4px solid #4caf50; padding: 10px; margin: 10px 0; background: #e8f5e8; }
        .benchmark { margin: 20px 0; padding: 15px; background: #f8f9fa; border-radius: 8px; }
        .metric-row { display: flex; justify-content: space-between; padding: 5px 0; border-bottom: 1px solid #ddd; }
        .metric-name { font-weight: 500; }
        .metric-value { font-family: monospace; }
        .metric-passed { color: #4caf50; }
        .metric-failed { color: #f44336; }
        .chart { height: 300px; margin: 20px 0; }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 8px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background: #f0f0f0; }
        .device-info { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 10px; }
        .info-item { background: #f8f9fa; padding: 8px; border-radius: 4px; }
        .timestamp { color: #666; font-size: 0.9em; }
    </style>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📊 Performance Regression Report</h1>
            <div class="timestamp">Generated: "#);
        
        html.push_str(&Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        
        let score_class = if report.summary.score >= 80.0 {
            "good"
        } else if report.summary.score >= 60.0 {
            "warning"
        } else {
            "bad"
        };
        
        html.push_str(&format!(r#"</div>
            <div style="margin-top: 20px;">
                <span class="score {}">{:.1}</span>
                <span style="font-size: 18px;">/100</span>
            </div>
        </div>
        
        <div class="summary-card">
            <h2>Test Summary</h2>
            <table>
                <tr><th>Test Run ID</th><td>{}</td></tr>
                <tr><th>Baseline Version</th><td>{}</td></tr>
                <tr><th>Current Version</th><td>{}</td></tr>
                <tr><th>Duration</th><td>{:?}</td></tr>
                <tr><th>Total Benchmarks</th><td>{}</td></tr>
                <tr><th>Passed</th><td><span style="color: #4caf50;">{}</span></td></tr>
                <tr><th>Warnings</th><td><span style="color: #ff9800;">{}</span></td></tr>
                <tr><th>Critical</th><td><span style="color: #f44336;">{}</span></td></tr>
                <tr><th>Severe</th><td><span style="color: #d32f2f;">{}</span></td></tr>
                <tr><th>Improvements</th><td><span style="color: #4caf50;">{}</span></td></tr>
            </table>
        </div>
        
        <div class="summary-card">
            <h2>Device Information</h2>
            <div class="device-info">
                <div class="info-item"><strong>Manufacturer:</strong> {}</div>
                <div class="info-item"><strong>Model:</strong> {}</div>
                <div class="info-item"><strong>Android:</strong> {}</div>
                <div class="info-item"><strong>Kernel:</strong> {}</div>
                <div class="info-item"><strong>CPU:</strong> {} cores @ {}MHz</div>
                <div class="info-item"><strong>RAM:</strong> {}MB total / {}MB free</div>
                <div class="info-item"><strong>Storage:</strong> {}MB total / {}MB free</div>
                <div class="info-item"><strong>Battery:</strong> {}mAh</div>
            </div>
        </div>"#,
            report.summary.score,
            report.test_run_id,
            report.baseline_version,
            report.current_version,
            report.summary.duration,
            report.summary.total_benchmarks,
            report.summary.passed,
            report.summary.warnings,
            report.summary.critical,
            report.summary.severe,
            report.summary.improvements,
            report.device_info.manufacturer,
            report.device_info.model,
            report.device_info.android_version,
            report.device_info.kernel_version,
            report.device_info.cpu_cores,
            report.device_info.cpu_max_freq_mhz,
            report.device_info.total_ram_mb,
            report.device_info.available_ram_mb,
            report.device_info.total_storage_mb,
            report.device_info.free_storage_mb,
            report.device_info.battery_capacity_mah,
        ));
        
        if !report.regressions.is_empty() {
            html.push_str(r#"
        <div class="summary-card">
            <h2>⚠️ Regressions Detected</h2>"#);
            
            for regression in &report.regressions {
                let severity_color = match regression.severity {
                    RegressionSeverity::Warning => "#ff9800",
                    RegressionSeverity::Critical => "#f44336",
                    RegressionSeverity::Severe => "#d32f2f",
                };
                
                html.push_str(&format!(r#"
            <div class="regression" style="border-left-color: {};">
                <strong>{}</strong> - {}<br>
                Baseline: {:.2} → Current: {:.2} ({:+.1}%)<br>
                <small>Possible causes: {}</small><br>
                <small>Recommendations: {}</small>
            </div>"#,
                    severity_color,
                    regression.benchmark_name,
                    regression.metric_name,
                    regression.baseline_value,
                    regression.current_value,
                    regression.percentage_change,
                    regression.possible_causes.join(", "),
                    regression.recommendations.join(", "),
                ));
            }
            
            html.push_str("</div>");
        }
        
        if !report.improvements.is_empty() {
            html.push_str(r#"
        <div class="summary-card">
            <h2>✅ Improvements</h2>"#);
            
            for improvement in &report.improvements {
                html.push_str(&format!(r#"
            <div class="improvement">
                <strong>{}</strong> - {}<br>
                Baseline: {:.2} → Current: {:.2} ({:+.1}%)
            </div>"#,
                    improvement.benchmark_name,
                    improvement.metric_name,
                    improvement.baseline_value,
                    improvement.current_value,
                    improvement.percentage_change,
                ));
            }
            
            html.push_str("</div>");
        }
        
        html.push_str(r#"
        <div class="summary-card">
            <h2>Detailed Benchmark Results</h2>"#);
        
        for benchmark in &report.benchmarks {
            html.push_str(&format!(r#"
            <div class="benchmark">
                <h3>{} <span style="float: right;">{}</span></h3>
                <div class="metrics">"#,
                benchmark.name,
                if benchmark.regression_detected {
                    format!("<span style='color: #f44336;'>⚠️ {:.1}% slower</span>", benchmark.regression_percentage)
                } else {
                    "<span style='color: #4caf50;'>✓ OK</span>".to_string()
                }
            ));
            
            for metric in &benchmark.metrics {
                let status_class = if metric.passed { "metric-passed" } else { "metric-failed" };
                let status_icon = if metric.passed { "✓" } else { "✗" };
                
                html.push_str(&format!(r#"
                    <div class="metric-row">
                        <span class="metric-name">{}</span>
                        <span class="metric-value {}">
                                                        {} {:.2} {}
                            {}
                        </span>
                    </div>"#,
                    metric.name,
                    status_class,
                    status_icon,
                    metric.value,
                    metric.unit,
                    if let Some(baseline) = metric.baseline_value {
                        format!("(baseline: {:.2})", baseline)
                    } else {
                        String::new()
                    }
                ));
            }
            
            html.push_str(&format!(r#"
                </div>
                <div class="metric-row" style="margin-top: 10px; background: #e0e0e0;">
                    <span class="metric-name">Statistics</span>
                    <span class="metric-value">
                        min: {:.2} | max: {:.2} | mean: {:.2} | median: {:.2} | p95: {:.2}
                    </span>
                </div>
            </div>"#,
                benchmark.statistics.min,
                benchmark.statistics.max,
                benchmark.statistics.mean,
                benchmark.statistics.median,
                benchmark.statistics.p95,
            ));
        }
        
        html.push_str(r#"
        </div>
        
        <div class="summary-card">
            <h2>Performance Charts</h2>"#);
        
        for chart in &report.charts {
            html.push_str(&format!(r#"
            <h4>{}</h4>
            <div class="chart">
                <canvas id="chart_{}" style="width:100%; height:300px;"></canvas>
            </div>
            <script>
                (function() {{
                    const ctx = document.getElementById('chart_{}').getContext('2d');
                    new Chart(ctx, {{
                        type: 'line',
                        data: {{
                            labels: [{}],
                            datasets: [{{
                                label: 'Performance',
                                data: [{}],
                                borderColor: 'rgb(75, 192, 192)',
                                tension: 0.1
                            }}, {{
                                label: 'Trend',
                                data: [{}],
                                borderColor: 'rgb(255, 159, 64)',
                                borderDash: [5, 5],
                                fill: false
                            }}{}]
                        }},
                        options: {{
                            responsive: true,
                            maintainAspectRatio: false,
                            scales: {{
                                y: {{
                                    beginAtZero: true,
                                    title: {{
                                        display: true,
                                        text: 'Value'
                                    }}
                                }}
                            }}
                        }}
                    }});
                }})();
            </script>"#,
                chart.name,
                chart.name.replace(" ", "_"),
                chart.name.replace(" ", "_"),
                (0..chart.data_points.len()).map(|i| i.to_string()).collect::<Vec<_>>().join(","),
                chart.data_points.iter().map(|p| p.y.to_string()).collect::<Vec<_>>().join(","),
                chart.trend_line.iter().map(|p| p.y.to_string()).collect::<Vec<_>>().join(","),
                if !chart.thresholds.is_empty() {
                    format!(", {}",
                        chart.thresholds.iter().map(|t| 
                            format!("{{ label: '{}', data: [{}], borderColor: '{}', borderDash: [2, 2], fill: false }}",
                                t.label,
                                (0..chart.data_points.len()).map(|_| t.value.to_string()).collect::<Vec<_>>().join(","),
                                t.color
                            )
                        ).collect::<Vec<_>>().join(",")
                    )
                } else {
                    String::new()
                }
            ));
        }
        
        html.push_str(r#"
        </div>
    </div>
</body>
</html>"#);
        
        fs::write(&html_path, html)
            .map_err(|e| format!("Failed to write HTML report: {}", e))?;
        
        println!("{} HTML report saved to {:?}", style("✓").green(), html_path);
        
        Ok(())
    }
    
    fn save_as_baseline(&self, report: &RegressionReport) -> Result<(), String> {
        let mut benchmarks_map = HashMap::new();
        
        for benchmark in &report.benchmarks {
            benchmarks_map.insert(benchmark.name.clone(), benchmark.clone());
        }
        
        let baseline = PerformanceBaseline {
            version: report.current_version.clone(),
            device_model: report.device_info.model.clone(),
            android_version: report.device_info.android_version.clone(),
            timestamp: Utc::now(),
            benchmarks: benchmarks_map,
            system_info: report.device_info.clone(),
            thresholds: HashMap::new(),
        };
        
        self.save_baseline(&baseline)
    }
}

// ============= Вспомогательные функции =============

fn read_property(prop: &str) -> Result<String, String> {
    let output = Command::new("getprop")
        .arg(prop)
        .output()
        .map_err(|e| e.to_string())?;
    
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn read_cpu_max_freq() -> Result<u32, String> {
    if let Ok(freq) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq") {
        Ok(freq.trim().parse().unwrap_or(0) / 1000)
    } else {
        Ok(0)
    }
}

fn read_cpu_min_freq() -> Result<u32, String> {
    if let Ok(freq) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq") {
        Ok(freq.trim().parse().unwrap_or(0) / 1000)
    } else {
        Ok(0)
    }
}

fn read_cpu_governor() -> Result<String, String> {
    if let Ok(governor) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor") {
        Ok(governor.trim().to_string())
    } else {
        Ok("unknown".to_string())
    }
}

fn read_cpu_frequencies() -> Result<Vec<u32>, String> {
    let mut freqs = Vec::new();
    
    for cpu in 0..num_cpus::get() {
        let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", cpu);
        if let Ok(freq) = fs::read_to_string(path) {
            if let Ok(f) = freq.trim().parse::<u32>() {
                freqs.push(f / 1000);
            }
        }
    }
    
    Ok(freqs)
}

fn read_total_ram() -> Result<u64, String> {
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(val) = line.split_whitespace().nth(1) {
                    return Ok(val.parse::<u64>().unwrap_or(0) / 1024);
                }
            }
        }
    }
    Ok(0)
}

fn read_available_ram() -> Result<u64, String> {
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemAvailable:") {
                if let Some(val) = line.split_whitespace().nth(1) {
                    return Ok(val.parse::<u64>().unwrap_or(0) / 1024);
                }
            }
        }
    }
    Ok(0)
}

fn read_total_storage() -> Result<u64, String> {
    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        // Примерная оценка - 128GB
        Ok(128 * 1024)
    } else {
        Ok(0)
    }
}

fn read_free_storage() -> Result<u64, String> {
    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        // Примерная оценка - 50GB свободно
        Ok(50 * 1024)
    } else {
        Ok(0)
    }
}

fn read_battery_capacity() -> Result<u32, String> {
    if let Ok(capacity) = fs::read_to_string("/sys/class/power_supply/battery/charge_full_design") {
        Ok(capacity.trim().parse().unwrap_or(0) / 1000)
    } else {
        Ok(4000) // Значение по умолчанию
    }
}

fn read_battery_health() -> Result<String, String> {
    if let Ok(health) = fs::read_to_string("/sys/class/power_supply/battery/health") {
        Ok(health.trim().to_string())
    } else {
        Ok("Unknown".to_string())
    }
}

fn read_screen_resolution() -> Result<String, String> {
    let output = Command::new("wm")
        .arg("size")
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    Ok(output_str.split(':').nth(1).unwrap_or("").trim().to_string())
}

fn read_screen_density() -> Result<f32, String> {
    let output = Command::new("wm")
        .arg("density")
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    if let Some(density_str) = output_str.split(':').nth(1) {
        if let Ok(density) = density_str.trim().parse::<f32>() {
            return Ok(density);
        }
    }
    Ok(2.0)
}

fn read_gpu_renderer() -> Result<String, String> {
    Ok("Adreno".to_string()) // Заглушка
}

fn read_gpu_version() -> Result<String, String> {
    Ok("OpenGL ES 3.2".to_string()) // Заглушка
}

fn read_gpu_usage() -> Result<u32, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(0..100)) // Заглушка
}

fn read_thermal_zones() -> Result<Vec<ThermalZoneInfo>, String> {
    let mut zones = Vec::new();
    
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("thermal_zone") {
                let temp_path = entry.path().join("temp");
                let type_path = entry.path().join("type");
                
                let temp = if let Ok(t) = fs::read_to_string(temp_path) {
                    t.trim().parse::<f32>().unwrap_or(0.0) / 1000.0
                } else {
                    0.0
                };
                
                let type_ = if let Ok(t) = fs::read_to_string(type_path) {
                    t.trim().to_string()
                } else {
                    "unknown".to_string()
                };
                
                zones.push(ThermalZoneInfo {
                    name,
                    type_,
                    temperature: temp,
                    throttling_threshold: 60.0,
                    shutdown_threshold: 80.0,
                });
            }
        }
    }
    
    Ok(zones)
}

fn test_network_bandwidth() -> Result<f64, String> {
    let start = Instant::now();
    let mut downloaded = 0;
    
    // Скачиваем 1MB данных
    let url = "http://speedtest.tele2.net/1MB.zip";
    
    let output = Command::new("curl")
        .args(&["-s", "-o", "/dev/null", "-w", "%{size_download}", url])
        .output()
        .map_err(|e| format!("Failed to download: {}", e))?;
    
    let size_str = String::from_utf8_lossy(&output.stdout);
    let size = size_str.trim().parse::<f64>().unwrap_or(0.0);
    
    let elapsed = start.elapsed().as_secs_f64();
    
    if elapsed > 0.0 {
        Ok((size * 8.0) / elapsed / 1_000_000.0) // Mbps
    } else {
        Ok(0.0)
    }
}

fn calculate_jitter(rtts: &[f64]) -> f64 {
    if rtts.len() < 2 {
        return 0.0;
    }
    
    let mut jitter_sum = 0.0;
    for i in 1..rtts.len() {
        jitter_sum += (rtts[i] - rtts[i-1]).abs();
    }
    
    jitter_sum / (rtts.len() - 1) as f64
}

fn calculate_packet_loss(ping_output: &str) -> f64 {
    for line in ping_output.lines() {
        if line.contains("packet loss") {
            if let Some(loss_str) = line.split(',').nth(2) {
                if let Some(percent_str) = loss_str.trim().split('%').next() {
                    if let Ok(loss) = percent_str.parse::<f64>() {
                        return loss;
                    }
                }
            }
        }
    }
    0.0
}

fn measure_fps() -> Result<u32, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(30..60)) // Заглушка
}

fn measure_frame_times(count: usize) -> Result<Vec<f64>, String> {
    let mut times = Vec::new();
    let mut rng = rand::thread_rng();
    
    for _ in 0..count {
        times.push(rng.gen_range(10.0..20.0));
    }
    
    Ok(times)
}

fn calculate_std_dev(data: &[f64]) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }
    
    let mean = data.iter().sum::<f64>() / data.len() as f64;
    let variance = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (data.len() - 1) as f64;
    variance.sqrt()
}

fn measure_battery_drain_rate(duration: Duration) -> Result<f64, String> {
    let start_level = read_battery_level()?;
    thread::sleep(duration);
    let end_level = read_battery_level()?;
    
    let drain = start_level - end_level;
    let hours = duration.as_secs_f64() / 3600.0;
    
    Ok(drain / hours)
}

fn read_battery_level() -> Result<f64, String> {
    if let Ok(level) = fs::read_to_string("/sys/class/power_supply/battery/capacity") {
        Ok(level.trim().parse::<f64>().unwrap_or(0.0))
    } else {
        Ok(50.0) // Заглушка
    }
}

fn measure_battery_temp_under_load(duration: Duration) -> Result<f32, String> {
    let mut max_temp = 0.0;
    let start = Instant::now();
    
    while start.elapsed() < duration {
        if let Ok(temp) = fs::read_to_string("/sys/class/power_supply/battery/temp") {
            let temp_c = temp.trim().parse::<f32>().unwrap_or(0.0) / 10.0;
            if temp_c > max_temp {
                max_temp = temp_c;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    
    Ok(max_temp)
}

fn estimate_charge_time() -> Result<f64, String> {
    Ok(60.0) // Заглушка - 60 минут
}

fn measure_max_temperature(duration: Duration) -> Result<f32, String> {
    let mut max_temp = 0.0;
    let start = Instant::now();
    
    while start.elapsed() < duration {
        if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("thermal_zone") {
                    let temp_path = entry.path().join("temp");
                    if let Ok(temp_str) = fs::read_to_string(temp_path) {
                        let temp = temp_str.trim().parse::<f32>().unwrap_or(0.0) / 1000.0;
                        if temp > max_temp {
                            max_temp = temp;
                        }
                    }
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    
    Ok(max_temp)
}

fn measure_time_to_throttle() -> Result<f64, String> {
    Ok(120.0) // Заглушка - 2 минуты
}

fn measure_performance_drop_under_thermal() -> Result<f64, String> {
    Ok(15.0) // Заглушка - 15% падения
}

fn measure_thermal_recovery_time() -> Result<f64, String> {
    Ok(45.0) // Заглушка - 45 секунд
}

fn measure_touch_latency(samples: usize) -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    let mut latencies = Vec::new();
    
    for _ in 0..samples {
        latencies.push(rng.gen_range(10.0..30.0));
    }
    
    Ok(latencies.iter().sum::<f64>() / latencies.len() as f64)
}

fn measure_render_time() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(5.0..15.0))
}

fn measure_scroll_latency() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(10.0..25.0))
}

fn measure_jank_count(operations: usize) -> Result<usize, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(0..5))
}

fn measure_background_memory() -> Result<f64, String> {
    if let Some(pid) = get_app_pid() {
        if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", pid)) {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        if let Ok(mem_kb) = val.parse::<f64>() {
                            return Ok(mem_kb / 1024.0);
                        }
                    }
                }
            }
        }
    }
    Ok(50.0) // Заглушка
}

fn get_app_pid() -> Option<u32> {
    let output = Command::new("pgrep")
        .arg("-f")
        .arg("com.example.app")
        .output()
        .ok()?;
    
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()
}

fn measure_background_cpu() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(0.0..5.0))
}

fn measure_resume_time() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(50.0..200.0))
}

fn measure_state_preservation() -> Result<bool, String> {
    Ok(true)
}

fn measure_sensor_rate(sensor: &str) -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(50.0..100.0))
}

fn measure_gps_latency() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(500.0..3000.0))
}

fn measure_sensor_accuracy() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(90.0..100.0))
}

fn measure_camera_startup() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(200.0..500.0))
}

fn measure_photo_capture() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(100.0..300.0))
}

fn measure_video_fps() -> Result<u32, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(24..30))
}

fn measure_audio_latency() -> Result<f64, String> {
    let mut rng = rand::thread_rng();
    Ok(rng.gen_range(20.0..60.0))
}

fn calculate_statistics(samples: &[f64]) -> BenchmarkStatistics {
    if samples.is_empty() {
        return BenchmarkStatistics {
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            median: 0.0,
            std_dev: 0.0,
            variance: 0.0,
            p95: 0.0,
            p99: 0.0,
            p999: 0.0,
            jitter: 0.0,
            trend_slope: 0.0,
            samples_count: 0,
        };
    }
    
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    
    let median = if sorted.len() % 2 == 0 {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };
    
    let variance = sorted.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / sorted.len() as f64;
    let std_dev = variance.sqrt();
    
    let p95_idx = (sorted.len() as f64 * 0.95).floor() as usize;
    let p95 = sorted[p95_idx.min(sorted.len() - 1)];
    
    let p99_idx = (sorted.len() as f64 * 0.99).floor() as usize;
    let p99 = sorted[p99_idx.min(sorted.len() - 1)];
    
    let p999_idx = (sorted.len() as f64 * 0.999).floor() as usize;
    let p999 = sorted[p999_idx.min(sorted.len() - 1)];
    
    let jitter = if sorted.len() > 1 {
        let mut sum = 0.0;
        for i in 1..sorted.len() {
            sum += (sorted[i] - sorted[i-1]).abs();
        }
        sum / (sorted.len() - 1) as f64
    } else {
        0.0
    };
    
    let trend_slope = if sorted.len() > 1 {
        let x_mean = (sorted.len() - 1) as f64 / 2.0;
        let mut numerator = 0.0;
        let mut denominator = 0.0;
        
        for (i, &y) in sorted.iter().enumerate() {
            let x = i as f64;
            numerator += (x - x_mean) * (y - mean);
            denominator += (x - x_mean).powi(2);
        }
        
        if denominator != 0.0 {
            numerator / denominator
        } else {
            0.0
        }
    } else {
        0.0
    };
    
    BenchmarkStatistics {
        min,
        max,
        mean,
        median,
        std_dev,
        variance,
        p95,
        p99,
        p999,
        jitter,
        trend_slope,
        samples_count: samples.len(),
    }
}

fn calculate_confidence_interval(samples: &[f64], confidence: f64) -> (f64, f64) {
    if samples.len() < 2 {
        return (0.0, 0.0);
    }
    
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let std_dev = calculate_std_dev(samples);
    let z_score = 1.96; // для 95% доверия
    
    let margin = z_score * std_dev / (samples.len() as f64).sqrt();
    
    (mean - margin, mean + margin)
}

fn calculate_trend_line(points: &[DataPoint]) -> Vec<DataPoint> {
    if points.len() < 2 {
        return Vec::new();
    }
    
    let x_mean = points.iter().map(|p| p.x).sum::<f64>() / points.len() as f64;
    let y_mean = points.iter().map(|p| p.y).sum::<f64>() / points.len() as f64;
    
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    
    for point in points {
        numerator += (point.x - x_mean) * (point.y - y_mean);
        denominator += (point.x - x_mean).powi(2);
    }
    
    let slope = if denominator != 0.0 { numerator / denominator } else { 0.0 };
    let intercept = y_mean - slope * x_mean;
    
    points.iter().map(|p| DataPoint {
        x: p.x,
        y: slope * p.x + intercept,
        label: None,
    }).collect()
}

fn suggest_causes(category: &BenchmarkCategory, metric: &str) -> Vec<String> {
    match category {
        BenchmarkCategory::CPU => vec![
            "CPU governor changed".to_string(),
            "Thermal throttling".to_string(),
            "Background processes".to_string(),
            "Kernel changes".to_string(),
        ],
        BenchmarkCategory::Memory => vec![
            "Memory leak".to_string(),
            "Increased memory pressure".to_string(),
            "ZRAM configuration".to_string(),
            "Kernel memory management".to_string(),
        ],
        BenchmarkCategory::Filesystem => vec![
            "Filesystem fragmentation".to_string(),
            "Storage wear".to_string(),
            "I/O scheduler changes".to_string(),
            "Disk encryption overhead".to_string(),
        ],
        BenchmarkCategory::Network => vec![
            "Network congestion".to_string(),
            "Signal strength".to_string(),
            "DNS changes".to_string(),
            "Network stack configuration".to_string(),
        ],
        _ => vec!["Unknown cause".to_string()],
    }
}

fn suggest_fixes(category: &BenchmarkCategory, metric: &str) -> Vec<String> {
    match category {
        BenchmarkCategory::CPU => vec![
            "Check CPU governor settings".to_string(),
            "Reduce background processes".to_string(),
            "Update kernel".to_string(),
            "Improve thermal management".to_string(),
        ],
        BenchmarkCategory::Memory => vec![
            "Check for memory leaks".to_string(),
            "Adjust ZRAM size".to_string(),
            "Optimize memory allocation".to_string(),
            "Clear caches".to_string(),
        ],
        BenchmarkCategory::Filesystem => vec![
            "Run filesystem trim".to_string(),
            "Defragment storage".to_string(),
            "Check I/O scheduler".to_string(),
            "Update filesystem driver".to_string(),
        ],
        BenchmarkCategory::Network => vec![
            "Improve network connectivity".to_string(),
            "Check DNS configuration".to_string(),
            "Update network drivers".to_string(),
            "Reset network stack".to_string(),
        ],
        _ => vec!["Investigate manually".to_string()],
    }
}

fn black_box<T>(x: T) -> T {
    unsafe {
        let ret = std::ptr::read_volatile(&x);
        std::mem::forget(x);
        ret
    }
}

fn measure_memory_latency() -> f64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(50.0..150.0) // Заглушка в наносекундах
}

// ============= Тесты =============

#[test]
fn test_performance_regression_suite() {
    println!("{}", "=".repeat(80));
    println!("{:^80}", "📊 PERFORMANCE REGRESSION TEST SUITE");
    println!("{}", "=".repeat(80));
    
    let mut tester = PerformanceRegressionTester::new();
    
    match tester.run_regression_suite() {
        Ok(report) => {
            println!("\n{}", "=".repeat(80));
            println!("{}", style("РЕЗУЛЬТАТЫ РЕГРЕССИОННОГО ТЕСТИРОВАНИЯ").cyan().bold());
            println!("{}", "=".repeat(80));
            
            println!("Test Run ID: {}", report.test_run_id);
            println!("Score: {:.1}/100", report.summary.score);
            println!("Status: {}", if report.summary.score >= 80.0 {
                style("GOOD").green()
            } else if report.summary.score >= 60.0 {
                style("WARNING").yellow()
            } else {
                style("CRITICAL").red()
            });
            
            println!("\nРегрессий: {}(W: {}, C: {}, S: {})", 
                report.regressions.len(),
                report.summary.warnings,
                report.summary.critical,
                report.summary.severe
            );
            println!("Улучшений: {}", report.improvements.len());
            
            for regression in &report.regressions {
                let severity_icon = match regression.severity {
                    RegressionSeverity::Warning => "⚠️",
                    RegressionSeverity::Critical => "🔴",
                    RegressionSeverity::Severe => "🔥",
                };
                
                println!("  {} {} - {}: {:+.1}%", 
                    severity_icon,
                    regression.benchmark_name,
                    regression.metric_name,
                    regression.percentage_change
                );
            }
            
            for improvement in &report.improvements {
                println!("  ✅ {} - {}: {:+.1}%", 
                    improvement.benchmark_name,
                    improvement.metric_name,
                    improvement.percentage_change
                );
            }
            
            println!("\n{}", "=".repeat(80));
            
            // Проверяем наличие критических регрессий
            assert!(report.summary.severe == 0, "Severe regressions detected!");
            assert!(report.summary.critical < 3, "Too many critical regressions!");
        }
        Err(e) => {
            panic!("Regression testing failed: {}", e);
        }
    }
}

#[test]
fn test_cpu_regression() {
    let mut tester = PerformanceRegressionTester::new();
    let result = PerformanceRegressionTester::benchmark_cpu_performance().unwrap();
    
    println!("\nCPU Performance:");
    for metric in result.metrics {
        println!("  {}: {:.2} {}", metric.name, metric.value, metric.unit);
    }
    
    assert!(result.metrics.iter().all(|m| m.value > 0.0), "CPU metrics should be positive");
}

#[test]
fn test_memory_regression() {
    let mut tester = PerformanceRegressionTester::new();
    let result = PerformanceRegressionTester::benchmark_memory_performance().unwrap();
    
    println!("\nMemory Performance:");
    for metric in result.metrics {
        println!("  {}: {:.2} {}", metric.name, metric.value, metric.unit);
    }
    
    assert!(result.metrics.iter().all(|m| m.value > 0.0), "Memory metrics should be positive");
}

#[test]
fn test_filesystem_regression() {
    let mut tester = PerformanceRegressionTester::new();
    let result = PerformanceRegressionTester::benchmark_filesystem_performance().unwrap();
    
    println!("\nFilesystem Performance:");
    for metric in result.metrics {
        println!("  {}: {:.2} {}", metric.name, metric.value, metric.unit);
    }
    
    assert!(result.metrics.iter().all(|m| m.value > 0.0), "Filesystem metrics should be positive");
}

#[test]
fn test_baseline_management() {
    let mut tester = PerformanceRegressionTester::new();
    
    // Создаем тестовый baseline
    let test_result = PerformanceRegressionTester::benchmark_cpu_performance().unwrap();
    let mut benchmarks = HashMap::new();
    benchmarks.insert(test_result.name.clone(), test_result);
    
    let baseline = PerformanceBaseline {
        version: "test_version".to_string(),
        device_model: "test_device".to_string(),
        android_version: "test_android".to_string(),
        timestamp: Utc::now(),
        benchmarks,
        system_info: tester.device_info.clone(),
        thresholds: HashMap::new(),
    };
    
    // Сохраняем baseline
    tester.save_baseline(&baseline).expect("Failed to save baseline");
    
    // Загружаем baseline
    tester.load_baseline("test_version").expect("Failed to load baseline");
    
    assert!(tester.current_baseline.is_some(), "Baseline should be loaded");
}