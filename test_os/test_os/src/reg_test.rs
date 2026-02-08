// tests/mobile_performance/mod.rs
use std::time::{Duration, Instant};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

// 1. Кроссплатформенные пути
fn get_test_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // Android: используем внешнее хранилище или кэш
        PathBuf::from("/data/local/tmp")
    }
    
    #[cfg(target_os = "ios")]
    {
        // iOS: используем Documents директорию приложения
        let dirs = dirs::document_dir().expect("No document dir");
        dirs.join("test_data")
    }
    
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        // Десктоп: временная директория
        std::env::temp_dir()
    }
}

// 2. Адаптированный тест файловой системы
#[test]
fn test_mobile_file_io_performance() {
    let test_dir = get_test_dir();
    
    // Создаем тестовую директорию если не существует
    if !test_dir.exists() {
        fs::create_dir_all(&test_dir).expect("Failed to create test dir");
    }
    
    let test_file = test_dir.join("mobile_perf_test.bin");
    
    // Размер данных адаптирован под мобильные устройства
    let data_size = if cfg!(target_os = "android") {
        1024 * 1024 // 1MB для Android
    } else if cfg!(target_os = "ios") {
        512 * 1024 // 512KB для iOS
    } else {
        10 * 1024 * 1024 // 10MB для десктопов
    };
    
    let data = vec![42u8; 4096];
    let iterations = data_size / 4096;
    
    let start = Instant::now();
    let mut file = fs::File::create(&test_file).expect("Failed to create file");
    
    for _ in 0..iterations {
        use std::io::Write;
        file.write_all(&data).expect("Write failed");
    }
    
    file.sync_all().expect("Sync failed");
    let duration = start.elapsed();
    
    // Разные baseline для разных платформ
    let baseline = if cfg!(target_os = "android") {
        Duration::from_millis(50) // Android обычно медленнее
    } else if cfg!(target_os = "ios") {
        Duration::from_millis(30) // iOS быстрее
    } else {
        Duration::from_millis(20) // Десктоп самый быстрый
    };
    
    check_mobile_performance("file_write", duration, baseline);
}

// 3. Тест памяти с учетом ограничений
#[test]
fn test_mobile_memory_performance() {
    // Разные лимиты для разных платформ
    let (small_size, large_size) = if cfg!(target_os = "android") {
        (1024, 16 * 1024 * 1024) // 1KB и 16MB
    } else if cfg!(target_os = "ios") {
        (1024, 8 * 1024 * 1024) // 1KB и 8MB
    } else {
        (1024, 100 * 1024 * 1024) // 1KB и 100MB
    };
    
    // Тест мелких аллокаций
    let small_start = Instant::now();
    for _ in 0..1000 {
        let _vec = Vec::<u8>::with_capacity(small_size);
        let _string = String::with_capacity(small_size / 2);
    }
    let small_time = small_start.elapsed();
    
    // Тест больших аллокаций (меньше итераций)
    let large_start = Instant::now();
    for i in 0..10 {
        let size = large_size / (i + 1);
        let _large_vec = vec![0u8; size];
    }
    let large_time = large_start.elapsed();
    
    check_mobile_performance(
        "small_allocs_1000", 
        small_time, 
        Duration::from_micros(if cfg!(mobile) { 2000 } else { 1000 })
    );
    
    check_mobile_performance(
        "large_allocs_10",
        large_time,
        Duration::from_millis(if cfg!(mobile) { 100 } else { 50 })
    );
}

// 4. Тест многопоточности (количество потоков ограничено)
#[test]
fn test_mobile_threading_performance() {
    // Меньше потоков на мобильных
    let num_threads = if cfg!(target_os = "android") {
        4 // Android обычно 4-8 ядер
    } else if cfg!(target_os = "ios") {
        2 // Старые iPhone могут иметь 2 ядра
    } else {
        8 // Десктопы могут иметь много ядер
    };
    
    let barrier = Arc::new(Barrier::new(num_threads));
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for _ in 0..num_threads {
        let barrier = Arc::clone(&barrier);
        let counter = Arc::clone(&counter);
        
        let handle = thread::spawn(move || {
            barrier.wait();
            
            for i in 0..10000 {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // Меньше yield на мобильных
                if i % 100 == 0 {
                    thread::yield_now();
                }
            }
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    
    check_mobile_performance(
        &format!("threading_{}_threads", num_threads),
        duration,
        Duration::from_millis(match num_threads {
            2 => 10,
            4 => 15,
            8 => 20,
            _ => 25,
        })
    );
}

// 5. Тест батареи и энергоэффективности (специфично для мобильных)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[test]
fn test_power_efficiency() {
    use std::thread;
    use std::time::Duration;
    
    // Измеряем потребление CPU
    let start = Instant::now();
    let start_cpu_time = get_cpu_time();
    
    // Имитируем полезную нагрузку
    for _ in 0..1000000 {
        let _x = 42 * 42;
    }
    
    thread::sleep(Duration::from_millis(100));
    
    let duration = start.elapsed();
    let cpu_time_used = get_cpu_time() - start_cpu_time;
    
    // Энергоэффективность = полезная работа / время CPU
    let efficiency = 1000000.0 / cpu_time_used.as_secs_f64();
    
    println!("Power efficiency: {:.0} ops/sec CPU time", efficiency);
    
    // Проверяем что CPU не используется постоянно
    assert!(
        cpu_time_used < duration * 2, // Не более 2x реального времени
        "Excessive CPU usage: {:?} > {:?}",
        cpu_time_used,
        duration * 2
    );
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn get_cpu_time() -> Duration {
    // Получаем время CPU процесса (платформозависимо)
    #[cfg(target_os = "android")]
    {
        use libc::{times, clock_gettime, CLOCK_PROCESS_CPUTIME_ID};
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        unsafe {
            clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &mut ts);
        }
        Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
    }
    
    #[cfg(target_os = "ios")]
    {
        // iOS альтернатива
        Duration::from_secs(0) // Заглушка
    }
}

// 6. Тест сенсора/гироскопа (только мобильные)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[test]
fn test_sensor_performance() {
    // Измеряем задержку получения данных с сенсора
    let start = Instant::now();
    
    // Имитируем работу с сенсором
    let mut samples = 0;
    let sample_duration = Duration::from_micros(16667); // 60Hz
    
    while start.elapsed() < Duration::from_secs(1) {
        thread::sleep(sample_duration);
        samples += 1;
    }
    
    let actual_fps = samples as f64 / start.elapsed().as_secs_f64();
    println!("Sensor sampling rate: {:.1} Hz", actual_fps);
    
    // Проверяем стабильность частоты дискретизации
    assert!(
        actual_fps > 55.0 && actual_fps < 65.0, // Ожидаем ~60Hz
        "Unstable sensor sampling: {:.1} Hz",
        actual_fps
    );
}

// 7. Проверка производительности с учетом платформы
fn check_mobile_performance(test_name: &str, current: Duration, baseline: Duration) {
    let ratio = current.as_secs_f64() / baseline.as_secs_f64();
    let platform = if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(target_os = "ios") {
        "iOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Unknown"
    };
    
    println!(
        "[{}] {}: {:?} (baseline: {:?}, ratio: {:.2}x)",
        platform, test_name, current, baseline, ratio
    );
    
    // Разные допуски для разных платформ
    let tolerance = if cfg!(target_os = "android") {
        1.0 // Android более вариативен
    } else if cfg!(target_os = "ios") {
        0.7 // iOS более стабильна
    } else {
        0.5 // Десктопы самые стабильные
    };
    
    if ratio > (1.0 + tolerance) {
        panic!(
            "Performance regression on {}: {} is {:.1}% slower than baseline",
            platform, test_name, (ratio - 1.0) * 100.0
        );
    }
}

// 8. Тест тач-интерфейса (специфично для мобильных)
#[cfg(any(target_os = "android", target_os = "ios"))]
#[test]
fn test_touch_latency() {
    use std::time::{Instant, Duration};
    
    // Имитируем обработку тач-событий
    let mut total_latency = Duration::new(0, 0);
    let mut events = 0;
    
    for i in 0..100 {
        let event_time = Instant::now();
        
        // Имитируем обработку события
        thread::sleep(Duration::from_micros(1000)); // 1ms обработка
        let processed_time = Instant::now();
        
        total_latency += processed_time.duration_since(event_time);
        events += 1;
    }
    
    let avg_latency = total_latency / events;
    println!("Average touch latency: {:?}", avg_latency);
    
    // На мобильных ожидаем латенси < 16ms (60 FPS)
    assert!(
        avg_latency < Duration::from_millis(16),
        "Touch latency too high: {:?}",
        avg_latency
    );
}

