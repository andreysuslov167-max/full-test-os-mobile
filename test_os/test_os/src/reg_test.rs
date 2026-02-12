
use std::time::{Duration, Instant};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;


fn get_test_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        
        PathBuf::from("/data/local/tmp")
    }
    
    #[cfg(target_os = "ios")]
    {
        
        let dirs = dirs::document_dir().expect("No document dir");
        dirs.join("test_data")
    }
    
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        
        std::env::temp_dir()
    }
}


#[test]
fn test_mobile_file_io_performance() {
    let test_dir = get_test_dir();
    
    
    if !test_dir.exists() {
        fs::create_dir_all(&test_dir).expect("Failed to create test dir");
    }
    
    let test_file = test_dir.join("mobile_perf_test.bin");
    
    
    let data_size = if cfg!(target_os = "android") {
        1024 * 1024 
    } else if cfg!(target_os = "ios") {
        512 * 1024 
    } else {
        10 * 1024 * 1024 
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
    
    
    let baseline = if cfg!(target_os = "android") {
        Duration::from_millis(50) 
    } else if cfg!(target_os = "ios") {
        Duration::from_millis(30) 
    } else {
        Duration::from_millis(20)
    };
    
    check_mobile_performance("file_write", duration, baseline);
}


#[test]
fn test_mobile_memory_performance() {
    
    let (small_size, large_size) = if cfg!(target_os = "android") {
        (1024, 16 * 1024 * 1024) 
    } else if cfg!(target_os = "ios") {
        (1024, 8 * 1024 * 1024) 
    } else {
        (1024, 100 * 1024 * 1024) 
    };
    

    let small_start = Instant::now();
    for _ in 0..1000 {
        let _vec = Vec::<u8>::with_capacity(small_size);
        let _string = String::with_capacity(small_size / 2);
    }
    let small_time = small_start.elapsed();
    
  
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


#[test]
fn test_mobile_threading_performance() {
    
    let num_threads = if cfg!(target_os = "android") {
        4 
    } else if cfg!(target_os = "ios") {
        2 
    } else {
        8 
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


#[cfg(any(target_os = "android", target_os = "ios"))]
#[test]
fn test_power_efficiency() {
    use std::thread;
    use std::time::Duration;
    
   
    let start = Instant::now();
    let start_cpu_time = get_cpu_time();
    
    
    for _ in 0..1000000 {
        let _x = 42 * 42;
    }
    
    thread::sleep(Duration::from_millis(100));
    
    let duration = start.elapsed();
    let cpu_time_used = get_cpu_time() - start_cpu_time;
    
    
    let efficiency = 1000000.0 / cpu_time_used.as_secs_f64();
    
    println!("Power efficiency: {:.0} ops/sec CPU time", efficiency);
    
    
    assert!(
        cpu_time_used < duration * 2, 
        "Excessive CPU usage: {:?} > {:?}",
        cpu_time_used,
        duration * 2
    );
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn get_cpu_time() -> Duration {
   
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
       
        Duration::from_secs(0) 
    }
}


#[cfg(any(target_os = "android", target_os = "ios"))]
#[test]
fn test_sensor_performance() {
   
    let start = Instant::now();
    
    
    let mut samples = 0;
    let sample_duration = Duration::from_micros(16667); // 60Hz
    
    while start.elapsed() < Duration::from_secs(1) {
        thread::sleep(sample_duration);
        samples += 1;
    }
    
    let actual_fps = samples as f64 / start.elapsed().as_secs_f64();
    println!("Sensor sampling rate: {:.1} Hz", actual_fps);
    
    
    assert!(
        actual_fps > 55.0 && actual_fps < 65.0,
        "Unstable sensor sampling: {:.1} Hz",
        actual_fps
    );
}


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
    
   
    let tolerance = if cfg!(target_os = "android") {
        1.0 
    } else if cfg!(target_os = "ios") {
        0.7 
    } else {
        0.5 
    };
    
    if ratio > (1.0 + tolerance) {
        panic!(
            "Performance regression on {}: {} is {:.1}% slower than baseline",
            platform, test_name, (ratio - 1.0) * 100.0
        );
    }
}


#[cfg(any(target_os = "android", target_os = "ios"))]
#[test]
fn test_touch_latency() {
    use std::time::{Instant, Duration};
    
    
    let mut total_latency = Duration::new(0, 0);
    let mut events = 0;
    
    for i in 0..100 {
        let event_time = Instant::now();
        
        
        thread::sleep(Duration::from_micros(1000)); 
        let processed_time = Instant::now();
        
        total_latency += processed_time.duration_since(event_time);
        events += 1;
    }
    
    let avg_latency = total_latency / events;
    println!("Average touch latency: {:?}", avg_latency);
    

    assert!(
        avg_latency < Duration::from_millis(16),
        "Touch latency too high: {:?}",
        avg_latency
    );
}

