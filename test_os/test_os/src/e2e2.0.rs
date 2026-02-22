// tests/mobile_e2e_adb/mod.rs
#![cfg(target_os = "android")]

use std::time::{Duration, Instant, SystemTime};
use std::thread;
use std::sync::{Arc, Mutex, RwLock, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::fs::{self, File, OpenOptions};
use std::io::{Write, Read, Seek, SeekFrom, BufReader, BufWriter, LineWriter};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, VecDeque, BTreeMap};
use std::process::{Command, Stdio, Child};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use regex::Regex;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Local};

// ============= ADB Manager =============

struct ADBManager {
    device_serial: Option<String>,
    adb_path: String,
    connected: bool,
    logcat_process: Option<Child>,
    log_file: PathBuf,
    screenshot_dir: PathBuf,
    video_dir: PathBuf,
}

impl ADBManager {
    fn new() -> Self {
        let adb_path = Self::find_adb();
        let device_serial = Self::get_device_serial(&adb_path);
        let connected = device_serial.is_some();
        
        let log_dir = PathBuf::from("/sdcard/Android/data/com.example.test/logs");
        let screenshot_dir = PathBuf::from("/sdcard/Android/data/com.example.test/screenshots");
        let video_dir = PathBuf::from("/sdcard/Android/data/com.example.test/videos");
        
        if connected {
            let serial = device_serial.as_ref().unwrap();
            Self::ensure_directory(&adb_path, serial, &log_dir);
            Self::ensure_directory(&adb_path, serial, &screenshot_dir);
            Self::ensure_directory(&adb_path, serial, &video_dir);
        }
        
        Self {
            device_serial,
            adb_path,
            connected,
            logcat_process: None,
            log_file: log_dir.join(format!("logcat_{}.log", Local::now().format("%Y%m%d_%H%M%S"))),
            screenshot_dir,
            video_dir,
        }
    }
    
    fn find_adb() -> String {
        let possible_paths = vec![
            "adb".to_string(),
            "/usr/bin/adb".to_string(),
            "/usr/local/bin/adb".to_string(),
            "C:\\Program Files\\Android\\platform-tools\\adb.exe".to_string(),
            "C:\\Android\\platform-tools\\adb.exe".to_string(),
            std::env::var("ANDROID_HOME").unwrap_or_default() + "/platform-tools/adb",
        ];
        
        for path in possible_paths {
            if Command::new(&path)
                .arg("version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false) 
            {
                println!("ADB found at: {}", path);
                return path;
            }
        }
        
        panic!("ADB not found! Please install Android platform tools");
    }
    
    fn get_device_serial(adb_path: &str) -> Option<String> {
        let output = Command::new(adb_path)
            .arg("devices")
            .output()
            .expect("Failed to run adb devices");
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        
        for line in output_str.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == "device" {
                let serial = parts[0].to_string();
                println!("Connected to device: {}", serial);
                return Some(serial);
            }
        }
        
        eprintln!("No Android device connected!");
        None
    }
    
    fn ensure_directory(adb_path: &str, serial: &str, path: &Path) {
        Command::new(adb_path)
            .arg("-s")
            .arg(serial)
            .arg("shell")
            .arg("mkdir")
            .arg("-p")
            .arg(path.to_str().unwrap())
            .output()
            .ok();
    }
    
    fn exec(&self, args: &[&str]) -> Result<String, String> {
        let mut cmd = Command::new(&self.adb_path);
        
        if let Some(serial) = &self.device_serial {
            cmd.arg("-s").arg(serial);
        }
        
        cmd.args(args);
        
        println!("ADB: {}", format_args!("{:?}", args).replace("\"", ""));
        
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute ADB command: {}", e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ADB command failed: {}", stderr));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    fn shell(&self, command: &str) -> Result<String, String> {
        self.exec(&["shell", command])
    }
    
    fn pull(&self, remote: &str, local: &str) -> Result<String, String> {
        self.exec(&["pull", remote, local])
    }
    
    fn push(&self, local: &str, remote: &str) -> Result<String, String> {
        self.exec(&["push", local, remote])
    }
    
    fn start_logcat(&mut self) -> Result<(), String> {
        let mut cmd = Command::new(&self.adb_path);
        
        if let Some(serial) = &self.device_serial {
            cmd.arg("-s").arg(serial);
        }
        
        cmd.args(&["logcat", "-v", "threadtime", "-f", "/sdcard/logcat.txt"]);
        
        let process = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start logcat: {}", e))?;
        
        self.logcat_process = Some(process);
        
        thread::sleep(Duration::from_millis(500));
        
        Ok(())
    }
    
    fn stop_logcat(&mut self) -> Result<String, String> {
        if let Some(mut process) = self.logcat_process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        
        self.shell("killall -9 logcat").ok();
        
        let remote_log = "/sdcard/logcat.txt";
        let local_log = self.log_file.to_str().unwrap();
        
        self.pull(remote_log, local_log).ok();
        self.shell(&format!("rm {}", remote_log)).ok();
        
        Ok(local_log.to_string())
    }
    
    fn take_screenshot(&self, name: &str) -> Result<PathBuf, String> {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let remote_file = format!("/sdcard/screenshot_{}_{}.png", name, timestamp);
        let local_file = format!("screenshots/{}_{}.png", name, timestamp);
        
        self.shell(&format!("screencap -p {}", remote_file))?;
        
        fs::create_dir_all("screenshots").ok();
        self.pull(&remote_file, &local_file)?;
        self.shell(&format!("rm {}", remote_file))?;
        
        Ok(PathBuf::from(local_file))
    }
    
    fn start_screenrecord(&self, duration: Duration) -> Result<String, String> {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let remote_file = format!("/sdcard/recording_{}.mp4", timestamp);
        
        let seconds = duration.as_secs();
        self.shell(&format!("screenrecord --time-limit {} {}", seconds, remote_file))?;
        
        Ok(remote_file)
    }
    
    fn stop_screenrecord(&self, remote_file: &str) -> Result<PathBuf, String> {
        self.shell("killall -SIGINT screenrecord").ok();
        thread::sleep(Duration::from_secs(2));
        
        let local_file = format!("videos/{}", Path::new(remote_file).file_name().unwrap().to_str().unwrap());
        
        fs::create_dir_all("videos").ok();
        self.pull(remote_file, &local_file)?;
        self.shell(&format!("rm {}", remote_file))?;
        
        Ok(PathBuf::from(local_file))
    }
    
    fn install_app(&self, apk_path: &str) -> Result<(), String> {
        self.exec(&["install", "-r", apk_path])?;
        Ok(())
    }
    
    fn uninstall_app(&self, package: &str) -> Result<(), String> {
        self.exec(&["uninstall", package])?;
        Ok(())
    }
    
    fn launch_app(&self, package: &str, activity: &str) -> Result<(), String> {
        self.shell(&format!("am start -n {}/{}", package, activity))?;
        Ok(())
    }
    
    fn force_stop_app(&self, package: &str) -> Result<(), String> {
        self.shell(&format!("am force-stop {}", package))?;
        Ok(())
    }
    
    fn clear_app_data(&self, package: &str) -> Result<(), String> {
        self.shell(&format!("pm clear {}", package))?;
        Ok(())
    }
    
    fn get_app_pid(&self, package: &str) -> Option<u32> {
        let output = self.shell(&format!("pidof {}", package)).ok()?;
        output.trim().parse().ok()
    }
    
    fn get_cpu_usage(&self, pid: u32) -> Result<f32, String> {
        let stat = self.shell(&format!("cat /proc/{}/stat", pid))?;
        let parts: Vec<&str> = stat.split_whitespace().collect();
        
        if parts.len() >= 14 {
            let utime: u64 = parts[13].parse().unwrap_or(0);
            let stime: u64 = parts[14].parse().unwrap_or(0);
            let cutime: u64 = parts[15].parse().unwrap_or(0);
            let cstime: u64 = parts[16].parse().unwrap_or(0);
            
            let total_time = utime + stime + cutime + cstime;
            
            let uptime = self.shell("cat /proc/uptime")?;
            let seconds: f64 = uptime.split_whitespace().next().unwrap_or("0").parse().unwrap_or(0.0);
            
            let usage = (total_time as f64 / seconds) * 100.0;
            
            Ok(usage as f32)
        } else {
            Ok(0.0)
        }
    }
    
    fn get_memory_usage(&self, pid: u32) -> Result<u64, String> {
        let status = self.shell(&format!("cat /proc/{}/status", pid))?;
        
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return Ok(parts[1].parse::<u64>().unwrap_or(0) * 1024);
                }
            }
        }
        
        Ok(0)
    }
    
    fn get_battery_info(&self) -> Result<BatteryInfo, String> {
        let info = self.shell("dumpsys battery")?;
        
        let mut battery = BatteryInfo::default();
        
        for line in info.lines() {
            if line.contains("level:") {
                battery.level = line.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
            } else if line.contains("temperature:") {
                let temp_c = line.split(':').nth(1).unwrap_or("0").trim().parse::<u32>().unwrap_or(0);
                battery.temperature = temp_c as f32 / 10.0;
            } else if line.contains("voltage:") {
                battery.voltage = line.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
            } else if line.contains("status:") {
                let status = line.split(':').nth(1).unwrap_or("").trim();
                battery.status = match status {
                    "1" => "Unknown".to_string(),
                    "2" => "Charging".to_string(),
                    "3" => "Discharging".to_string(),
                    "4" => "Not charging".to_string(),
                    "5" => "Full".to_string(),
                    _ => status.to_string(),
                };
            } else if line.contains("health:") {
                let health = line.split(':').nth(1).unwrap_or("").trim();
                battery.health = match health {
                    "1" => "Unknown".to_string(),
                    "2" => "Good".to_string(),
                    "3" => "Overheat".to_string(),
                    "4" => "Dead".to_string(),
                    "5" => "Over voltage".to_string(),
                    "6" => "Unspecified failure".to_string(),
                    "7" => "Cold".to_string(),
                    _ => health.to_string(),
                };
            }
        }
        
        Ok(battery)
    }
    
    fn set_battery_level(&self, level: u32) -> Result<(), String> {
        self.shell("dumpsys battery set status 1")?;
        self.shell(&format!("dumpsys battery set level {}", level))?;
        Ok(())
    }
    
    fn reset_battery(&self) -> Result<(), String> {
        self.shell("dumpsys battery reset")?;
        Ok(())
    }
    
    fn get_network_info(&self) -> Result<NetworkInfo, String> {
        let mut network = NetworkInfo::default();
        
        if let Ok(wifi) = self.shell("dumpsys wifi") {
            for line in wifi.lines() {
                if line.contains("mNetworkInfo") && line.contains("CONNECTED") {
                    network.connected = true;
                    network.type_ = "WiFi".to_string();
                }
            }
        }
        
        if let Ok(signal) = self.shell("dumpsys telephony") {
            for line in signal.lines() {
                if line.contains("mSignalStrength") {
                    if let Some(parts) = line.split_whitespace().last() {
                        network.signal_strength = parts.parse().unwrap_or(0);
                    }
                } else if line.contains("mNetworkType") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        network.type_ = parts.last().unwrap_or(&"Unknown").to_string();
                    }
                }
            }
        }
        
        Ok(network)
    }
    
    fn switch_wifi(&self, enable: bool) -> Result<(), String> {
        if enable {
            self.shell("svc wifi enable")?;
        } else {
            self.shell("svc wifi disable")?;
        }
        Ok(())
    }
    
    fn switch_mobile_data(&self, enable: bool) -> Result<(), String> {
        if enable {
            self.shell("svc data enable")?;
        } else {
            self.shell("svc data disable")?;
        }
        Ok(())
    }
    
    fn set_airplane_mode(&self, enable: bool) -> Result<(), String> {
        let value = if enable { "1" } else { "0" };
        self.shell(&format!("settings put global airplane_mode_on {}", value))?;
        self.shell(&format!("am broadcast -a android.intent.action.AIRPLANE_MODE --ez state {}", value))?;
        Ok(())
    }
    
    fn get_thermal_throttling(&self) -> Result<bool, String> {
        let thermal = self.shell("dumpsys thermalservice")?;
        
        for line in thermal.lines() {
            if line.contains("Thermal status:") && line.contains("CRITICAL") {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    fn get_cpu_frequencies(&self) -> Result<Vec<u32>, String> {
        let mut frequencies = Vec::new();
        
        for cpu in 0..num_cpus::get() {
            let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", cpu);
            if let Ok(freq) = self.shell(&format!("cat {}", path)) {
                if let Ok(freq_num) = freq.trim().parse::<u32>() {
                    frequencies.push(freq_num / 1000);
                }
            }
        }
        
        Ok(frequencies)
    }
    
    fn dump_window_hierarchy(&self) -> Result<String, String> {
        self.shell("uiautomator dump /dev/stdout")
    }
    
    fn find_element_by_text(&self, text: &str) -> Result<bool, String> {
        let dump = self.dump_window_hierarchy()?;
        Ok(dump.contains(text))
    }
    
    fn tap(&self, x: u32, y: u32) -> Result<(), String> {
        self.shell(&format!("input tap {} {}", x, y))?;
        Ok(())
    }
    
    fn swipe(&self, x1: u32, y1: u32, x2: u32, y2: u32, duration_ms: u32) -> Result<(), String> {
        self.shell(&format!("input swipe {} {} {} {} {}", x1, y1, x2, y2, duration_ms))?;
        Ok(())
    }
    
    fn type_text(&self, text: &str) -> Result<(), String> {
        self.shell(&format!("input text '{}'", text.replace("'", "\\'")))?;
        Ok(())
    }
    
    fn press_key(&self, key: &str) -> Result<(), String> {
        self.shell(&format!("input keyevent {}", key))?;
        Ok(())
    }
    
    fn get_logs(&self, tag: &str, lines: usize) -> Result<Vec<String>, String> {
        let output = self.shell(&format!("logcat -d -t {} | grep {}", lines, tag))?;
        Ok(output.lines().map(String::from).collect())
    }
    
    fn clear_logs(&self) -> Result<(), String> {
        self.shell("logcat -c")?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct BatteryInfo {
    level: u32,
    temperature: f32,
    voltage: u32,
    status: String,
    health: String,
}

#[derive(Debug, Default)]
struct NetworkInfo {
    connected: bool,
    type_: String,
    signal_strength: i32,
}

// ============= ADB Debugger =============

struct ADBDebugger {
    adb: Arc<ADBManager>,
    breakpoints: HashMap<String, Breakpoint>,
    watchpoints: HashMap<String, Watchpoint>,
    log_streams: Vec<LogStream>,
    debug_mode: Arc<AtomicBool>,
    session_id: String,
}

struct Breakpoint {
    location: String,
    condition: Option<String>,
    hit_count: AtomicU64,
    enabled: bool,
}

struct Watchpoint {
    expression: String,
    last_value: String,
    enabled: bool,
}

struct LogStream {
    tag: String,
    level: LogLevel,
    buffer: Arc<Mutex<VecDeque<String>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl ADBDebugger {
    fn new(adb: Arc<ADBManager>) -> Self {
        Self {
            adb,
            breakpoints: HashMap::new(),
            watchpoints: HashMap::new(),
            log_streams: Vec::new(),
            debug_mode: Arc::new(AtomicBool::new(false)),
            session_id: Uuid::new_v4().to_string(),
        }
    }
    
    fn set_breakpoint(&mut self, name: &str, location: &str, condition: Option<&str>) {
        self.breakpoints.insert(name.to_string(), Breakpoint {
            location: location.to_string(),
            condition: condition.map(String::from),
            hit_count: AtomicU64::new(0),
            enabled: true,
        });
        
        println!("[DEBUG] Breakpoint set: {} @ {}", name, location);
    }
    
    fn set_watchpoint(&mut self, name: &str, expression: &str) {
        self.watchpoints.insert(name.to_string(), Watchpoint {
            expression: expression.to_string(),
            last_value: String::new(),
            enabled: true,
        });
        
        println!("[DEBUG] Watchpoint set: {} = {}", name, expression);
    }
    
    fn watch_log(&mut self, tag: &str, level: LogLevel) {
        let stream = LogStream {
            tag: tag.to_string(),
            level,
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
        };
        
        self.log_streams.push(stream);
    }
    
    fn start_debug_session(&self) -> Result<DebugSession, String> {
        println!("\n[DEBUG] Starting debug session: {}", self.session_id);
        
        self.adb.clear_logs()?;
        self.adb.shell("setprop debug.monitor.enable 1")?;
        
        Ok(DebugSession {
            adb: self.adb.clone(),
            breakpoints: self.breakpoints.clone(),
            watchpoints: self.watchpoints.clone(),
            start_time: Instant::now(),
            session_id: self.session_id.clone(),
        })
    }
    
    fn interactive_debug(&self) -> Result<(), String> {
        println!("\n[DEBUG] Entering interactive debug mode (type 'help' for commands)");
        
        let mut session = self.start_debug_session()?;
        
        loop {
            print!("(adb-debug) ");
            std::io::stdout().flush().unwrap();
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();
            
            if input.is_empty() {
                continue;
            }
            
            let parts: Vec<&str> = input.split_whitespace().collect();
            
            match parts[0] {
                "help" => {
                    println!("Commands:");
                    println!("  break <location> [condition] - Set breakpoint");
                    println!("  watch <expression>           - Set watchpoint");
                    println!("  continue                      - Continue execution");
                    println!("  step                          - Step to next line");
                    println!("  next                          - Step over");
                    println!("  finish                        - Step out");
                    println!("  print <expr>                   - Print expression");
                    println!("  backtrace                      - Print stack trace");
                    println!("  variables                      - List local variables");
                    println!("  logcat [tag]                   - Show logcat");
                    println!("  screenshot                      - Take screenshot");
                    println!("  info breakpoints                - List breakpoints");
                    println!("  delete <name>                   - Delete breakpoint");
                    println!("  quit                            - Exit debug mode");
                }
                
                "break" | "b" => {
                    if parts.len() >= 2 {
                        let location = parts[1];
                        let condition = if parts.len() >= 3 { Some(parts[2]) } else { None };
                        
                        session.add_breakpoint(location, condition)?;
                        println!("Breakpoint set at {}", location);
                    } else {
                        println!("Usage: break <location> [condition]");
                    }
                }
                
                "watch" | "w" => {
                    if parts.len() >= 2 {
                        session.add_watchpoint(parts[1])?;
                    } else {
                        println!("Usage: watch <expression>");
                    }
                }
                
                "continue" | "c" => {
                    println!("Continuing execution...");
                    session.continue_execution()?;
                }
                
                "step" | "s" => {
                    session.step()?;
                }
                
                "next" | "n" => {
                    session.next()?;
                }
                
                "finish" => {
                    session.finish()?;
                }
                
                "print" | "p" => {
                    if parts.len() >= 2 {
                        let value = session.evaluate(parts[1])?;
                        println!("{} = {}", parts[1], value);
                    } else {
                        println!("Usage: print <expression>");
                    }
                }
                
                "backtrace" | "bt" => {
                    let trace = session.backtrace()?;
                    println!("Stack trace:");
                    for frame in trace {
                        println!("  {}", frame);
                    }
                }
                
                "variables" | "vars" | "locals" => {
                    let vars = session.get_variables()?;
                    println!("Local variables:");
                    for (name, value) in vars {
                        println!("  {} = {}", name, value);
                    }
                }
                
                "logcat" => {
                    let tag = if parts.len() >= 2 { Some(parts[1]) } else { None };
                    let logs = session.get_logs(tag, 50)?;
                    for log in logs {
                        println!("{}", log);
                    }
                }
                
                "screenshot" => {
                    let path = session.take_screenshot()?;
                    println!("Screenshot saved to: {}", path.display());
                }
                
                "info" => {
                    if parts.len() >= 2 && parts[1] == "breakpoints" {
                        let bps = session.list_breakpoints()?;
                        println!("Breakpoints:");
                        for bp in bps {
                            println!("  {} @ {} (hits: {})", bp.name, bp.location, bp.hits);
                        }
                    }
                }
                
                "delete" => {
                    if parts.len() >= 2 {
                        session.delete_breakpoint(parts[1])?;
                        println!("Breakpoint {} deleted", parts[1]);
                    }
                }
                
                "quit" | "exit" => {
                    println!("Exiting debug mode");
                    break;
                }
                
                _ => {
                    println!("Unknown command: {}", parts[0]);
                }
            }
        }
        
        Ok(())
    }
}

struct DebugSession {
    adb: Arc<ADBManager>,
    breakpoints: HashMap<String, Breakpoint>,
    watchpoints: HashMap<String, Watchpoint>,
    start_time: Instant,
    session_id: String,
}

impl DebugSession {
    fn add_breakpoint(&mut self, location: &str, condition: Option<&str>) -> Result<(), String> {
        let name = format!("bp_{}", self.breakpoints.len() + 1);
        let condition_str = condition.map(|s| s.to_string());
        
        self.breakpoints.insert(name.clone(), Breakpoint {
            location: location.to_string(),
            condition: condition_str,
            hit_count: AtomicU64::new(0),
            enabled: true,
        });
        
        Ok(())
    }
    
    fn add_watchpoint(&mut self, expression: &str) -> Result<(), String> {
        let name = format!("wp_{}", self.watchpoints.len() + 1);
        
        self.watchpoints.insert(name.clone(), Watchpoint {
            expression: expression.to_string(),
            last_value: String::new(),
            enabled: true,
        });
        
        Ok(())
    }
    
    fn continue_execution(&self) -> Result<(), String> {
        self.adb.shell("am broadcast -a com.example.DEBUG_CONTINUE")?;
        Ok(())
    }
    
    fn step(&self) -> Result<(), String> {
        self.adb.shell("am broadcast -a com.example.DEBUG_STEP")?;
        Ok(())
    }
    
    fn next(&self) -> Result<(), String> {
        self.adb.shell("am broadcast -a com.example.DEBUG_NEXT")?;
        Ok(())
    }
    
    fn finish(&self) -> Result<(), String> {
        self.adb.shell("am broadcast -a com.example.DEBUG_FINISH")?;
        Ok(())
    }
    
    fn evaluate(&self, expression: &str) -> Result<String, String> {
        let output = self.adb.shell(&format!("am broadcast -a com.example.DEBUG_EVALUATE --es expr '{}'", expression))?;
        Ok(output)
    }
    
    fn backtrace(&self) -> Result<Vec<String>, String> {
        let output = self.adb.shell("am broadcast -a com.example.DEBUG_BACKTRACE")?;
        Ok(output.lines().map(String::from).collect())
    }
    
    fn get_variables(&self) -> Result<Vec<(String, String)>, String> {
        let output = self.adb.shell("am broadcast -a com.example.DEBUG_VARIABLES")?;
        let mut vars = Vec::new();
        
        for line in output.lines() {
            if let Some((name, value)) = line.split_once('=') {
                vars.push((name.trim().to_string(), value.trim().to_string()));
            }
        }
        
        Ok(vars)
    }
    
    fn get_logs(&self, tag: Option<&str>, lines: usize) -> Result<Vec<String>, String> {
        let tag_filter = tag.unwrap_or("*");
        let output = self.adb.shell(&format!("logcat -d -t {} | grep {}", lines, tag_filter))?;
        Ok(output.lines().map(String::from).collect())
    }
    
    fn take_screenshot(&self) -> Result<PathBuf, String> {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let filename = format!("debug_{}.png", timestamp);
        
        self.adb.shell(&format!("screencap -p /sdcard/{}", filename))?;
        
        fs::create_dir_all("debug_screenshots").ok();
        let local_path = format!("debug_screenshots/{}", filename);
        self.adb.pull(&format!("/sdcard/{}", filename), &local_path)?;
        self.adb.shell(&format!("rm /sdcard/{}", filename))?;
        
        Ok(PathBuf::from(local_path))
    }
    
    fn list_breakpoints(&self) -> Result<Vec<BreakpointInfo>, String> {
        let mut info = Vec::new();
        
        for (name, bp) in &self.breakpoints {
            info.push(BreakpointInfo {
                name: name.clone(),
                location: bp.location.clone(),
                hits: bp.hit_count.load(Ordering::Relaxed),
                enabled: bp.enabled,
            });
        }
        
        Ok(info)
    }
    
    fn delete_breakpoint(&mut self, name: &str) -> Result<(), String> {
        self.breakpoints.remove(name);
        Ok(())
    }
}

struct BreakpointInfo {
    name: String,
    location: String,
    hits: u64,
    enabled: bool,
}

// ============= Enhanced E2E Test with ADB Debug =============

struct MobileE2ETestWithADB {
    adb: Arc<ADBManager>,
    debugger: ADBDebugger,
    package_name: String,
    main_activity: String,
    test_data_dir: PathBuf,
    metrics_history: Arc<RwLock<VecDeque<TestMetrics>>>,
    test_report: Arc<Mutex<Option<TestReport>>>,
}

#[derive(Debug, Clone, Serialize)]
struct TestMetrics {
    timestamp: DateTime<Utc>,
    cpu_usage: f32,
    memory_kb: u64,
    battery_level: u32,
    battery_temp: f32,
    fps: f32,
    frame_time_ms: f32,
    network_rx_kb: u64,
    network_tx_kb: u64,
    thermal_throttling: bool,
    pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct TestStepResult {
    name: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    duration: Duration,
    status: StepStatus,
    error: Option<String>,
    screenshot: Option<PathBuf>,
    metrics: Vec<TestMetrics>,
    logs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
enum StepStatus {
    Passed,
    Failed,
    Skipped,
    DebugBreak,
}

#[derive(Debug, Clone, Serialize)]
struct TestReport {
    test_name: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    duration: Duration,
    status: TestStatus,
    steps: Vec<TestStepResult>,
    device_info: DeviceInfo,
    log_file: Option<PathBuf>,
    video_file: Option<PathBuf>,
    summary: TestSummary,
}

#[derive(Debug, Clone, Serialize)]
struct TestSummary {
    total_steps: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
    debug_breaks: usize,
    avg_cpu: f32,
    max_cpu: f32,
    avg_memory_mb: f64,
    max_memory_mb: f64,
    battery_drain_percent: f32,
    max_temperature: f32,
}

#[derive(Debug, Clone, Serialize)]
enum TestStatus {
    Passed,
    Failed,
    Debugged,
}

#[derive(Debug, Clone, Serialize)]
struct DeviceInfo {
    manufacturer: String,
    model: String,
    android_version: String,
    sdk_version: u32,
    screen_resolution: String,
    battery_capacity: u32,
    total_ram_mb: u64,
    available_ram_mb: u64,
    internal_storage_mb: u64,
    free_storage_mb: u64,
}

impl MobileE2ETestWithADB {
    fn new(package_name: &str, main_activity: &str) -> Self {
        let adb = Arc::new(ADBManager::new());
        let debugger = ADBDebugger::new(adb.clone());
        
        let test_data_dir = PathBuf::from("/sdcard/Android/data")
            .join(package_name)
            .join("test_data");
        
        if adb.connected {
            adb.shell(&format!("mkdir -p {}", test_data_dir.display())).ok();
        }
        
        Self {
            adb,
            debugger,
            package_name: package_name.to_string(),
            main_activity: main_activity.to_string(),
            test_data_dir,
            metrics_history: Arc::new(RwLock::new(VecDeque::with_capacity(10000))),
            test_report: Arc::new(Mutex::new(None)),
        }
    }
    
    fn run_test_with_debug(&mut self, test_name: &str, debug_mode: bool) -> Result<TestReport, String> {
        println!("\n{}", "=".repeat(80));
        println!("Running test: {}", test_name);
        if debug_mode {
            println!("DEBUG MODE ENABLED - Interactive debugging available");
        }
        println!("{}", "=".repeat(80));
        
        let start_time = Utc::now();
        let mut steps = Vec::new();
        let mut video_file = None;
        
        // Start logcat recording
        self.adb.start_logcat()?;
        
        // Start screen recording if in debug mode
        if debug_mode {
            let remote_video = self.adb.start_screenrecord(Duration::from_secs(300))?;
            video_file = Some(remote_video);
        }
        
        // Clear app data and restart
        self.adb.clear_app_data(&self.package_name)?;
        self.adb.force_stop_app(&self.package_name)?;
        thread::sleep(Duration::from_secs(1));
        
        // Launch app
        self.adb.launch_app(&self.package_name, &self.main_activity)?;
        thread::sleep(Duration::from_secs(2));
        
        let pid = self.adb.get_app_pid(&self.package_name);
        println!("App PID: {:?}", pid);
        
        // Start metrics collection
        let stop_metrics = Arc::new(AtomicBool::new(false));
        let metrics_handle = self.start_metrics_collection(pid, stop_metrics.clone());
        
        // Define test steps
        let test_steps = self.define_test_steps();
        
        // Run steps
        for step in test_steps {
            if stop_metrics.load(Ordering::Relaxed) {
                break;
            }
            
            let step_result = self.execute_step_with_debug(step, debug_mode, pid)?;
            steps.push(step_result);
            
            if step_result.status == StepStatus::Failed && !debug_mode {
                println!("Test failed, stopping execution");
                break;
            }
        }
        
        // Stop metrics collection
        stop_metrics.store(true, Ordering::Relaxed);
        metrics_handle.join().unwrap();
        
        // Stop recording
        if debug_mode && video_file.is_some() {
            if let Some(remote) = video_file {
                video_file = Some(self.adb.stop_screenrecord(&remote)?);
            }
        }
        
        // Stop logcat and get logs
        let log_file = self.adb.stop_logcat().ok();
        
        // Get final metrics
        let metrics_history = self.metrics_history.read().unwrap();
        let metrics_vec: Vec<_> = metrics_history.iter().cloned().collect();
        
        // Calculate summary
        let summary = self.calculate_summary(&steps, &metrics_vec);
        
        // Get device info
        let device_info = self.get_device_info()?;
        
        let end_time = Utc::now();
        
        let status = if steps.iter().any(|s| s.status == StepStatus::Failed) {
            TestStatus::Failed
        } else if debug_mode {
            TestStatus::Debugged
        } else {
            TestStatus::Passed
        };
        
        let report = TestReport {
            test_name: test_name.to_string(),
            start_time,
            end_time,
            duration: end_time - start_time,
            status,
            steps,
            device_info,
            log_file,
            video_file,
            summary,
        };
        
        // Save report
        self.save_report(&report)?;
        
        Ok(report)
    }
    
    fn define_test_steps(&self) -> Vec<TestStep> {
        vec![
            TestStep {
                name: "Cold Start".to_string(),
                action: StepAction::Wait(Duration::from_secs(2)),
                timeout: Duration::from_secs(5),
                expected: "App should be responsive".to_string(),
            },
            TestStep {
                name: "UI Navigation".to_string(),
                action: StepAction::Tap(500, 1000),
                timeout: Duration::from_secs(3),
                expected: "UI should respond".to_string(),
            },
            TestStep {
                name: "Scroll".to_string(),
                action: StepAction::Swipe(500, 1500, 500, 500, 500),
                timeout: Duration::from_secs(2),
                expected: "Scroll should complete".to_string(),
            },
            TestStep {
                name: "Network Operation".to_string(),
                action: StepAction::NetworkChange(NetworkType::WiFi),
                timeout: Duration::from_secs(5),
                expected: "Network should switch".to_string(),
            },
            TestStep {
                name: "Background".to_string(),
                action: StepAction::Background(Duration::from_secs(5)),
                timeout: Duration::from_secs(10),
                expected: "App should survive background".to_string(),
            },
            TestStep {
                name: "Return to Foreground".to_string(),
                action: StepAction::Foreground,
                timeout: Duration::from_secs(5),
                expected: "App should resume".to_string(),
            },
        ]
    }
    
    fn execute_step_with_debug(&mut self, step: TestStep, debug_mode: bool, pid: Option<u32>) -> Result<TestStepResult, String> {
        println!("\n[STEP] {}", step.name);
        
        let step_start = Utc::now();
        let mut step_metrics = Vec::new();
        let mut error = None;
        let mut screenshot = None;
        let mut status = StepStatus::Passed;
        
        // Take pre-step screenshot
        if debug_mode {
            screenshot = self.adb.take_screenshot(&step.name.replace(" ", "_")).ok();
        }
        
        // Execute step with monitoring
        let step_duration = Instant::now();
        let step_result = self.execute_action(&step.action, step.timeout);
        
        // Check result
        match step_result {
            Ok(_) => {
                println!("  ✓ Step completed in {:?}", step_duration.elapsed());
            }
            Err(e) => {
                if debug_mode {
                    println!("  ⚠ Step failed: {}", e);
                    println!("  Entering debug mode for this step...");
                    
                    // Enter interactive debug
                    self.debugger.interactive_debug()?;
                    
                    // Retry step after debug
                    if let Ok(_) = self.execute_action(&step.action, step.timeout) {
                        status = StepStatus::Debugged;
                        println!("  ✓ Step completed after debug");
                    } else {
                        error = Some(e);
                        status = StepStatus::Failed;
                    }
                } else {
                    error = Some(e);
                    status = StepStatus::Failed;
                }
            }
        }
        
        // Take post-step screenshot on failure
        if status == StepStatus::Failed {
            screenshot = self.adb.take_screenshot(&format!("{}_failed", step.name.replace(" ", "_"))).ok();
        }
        
        // Collect metrics during step
        if let Some(pid) = pid {
            for _ in 0..5 {
                if let Ok(cpu) = self.adb.get_cpu_usage(pid) {
                    if let Ok(mem) = self.adb.get_memory_usage(pid) {
                        step_metrics.push(TestMetrics {
                            timestamp: Utc::now(),
                            cpu_usage: cpu,
                            memory_kb: mem,
                            battery_level: 0,
                            battery_temp: 0.0,
                            fps: 0.0,
                            frame_time_ms: 0.0,
                            network_rx_kb: 0,
                            network_tx_kb: 0,
                            thermal_throttling: false,
                            pid: Some(pid),
                        });
                    }
                }
                thread::sleep(Duration::from_millis(200));
            }
        }
        
        // Get step-specific logs
        let logs = self.adb.get_logs(&self.package_name, 100).unwrap_or_default();
        
        let step_end = Utc::now();
        
        Ok(TestStepResult {
            name: step.name,
            start_time: step_start,
            end_time: step_end,
            duration: step_end - step_start,
            status,
            error,
            screenshot,
            metrics: step_metrics,
            logs,
        })
    }
    
    fn execute_action(&self, action: &StepAction, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        
        loop {
            let result = match action {
                StepAction::Wait(duration) => {
                    thread::sleep(*duration);
                    Ok(())
                }
                StepAction::Tap(x, y) => {
                    self.adb.tap(*x, *y)
                }
                StepAction::Swipe(x1, y1, x2, y2, ms) => {
                    self.adb.swipe(*x1, *y1, *x2, *y2, *ms)
                }
                StepAction::TypeText(text) => {
                    self.adb.type_text(text)
                }
                StepAction::PressKey(key) => {
                    self.adb.press_key(key)
                }
                StepAction::NetworkChange(network) => {
                    match network {
                        NetworkType::WiFi => {
                            self.adb.switch_wifi(true)?;
                            self.adb.switch_mobile_data(false)?;
                        }
                        NetworkType::Mobile4G => {
                            self.adb.switch_wifi(false)?;
                            self.adb.switch_mobile_data(true)?;
                            self.adb.shell("setprop gsm.network.type lte")?;
                        }
                        NetworkType::Mobile3G => {
                            self.adb.switch_wifi(false)?;
                            self.adb.switch_mobile_data(true)?;
                            self.adb.shell("setprop gsm.network.type umts")?;
                        }
                        NetworkType::AirplaneMode => {
                            self.adb.set_airplane_mode(true)?;
                        }
                        NetworkType::NoNetwork => {
                            self.adb.set_airplane_mode(true)?;
                        }
                    }
                    Ok(())
                }
                StepAction::BatteryLevel(level) => {
                    self.adb.set_battery_level(*level)
                }
                StepAction::Background(duration) => {
                    self.adb.press_key("KEYCODE_HOME")?;
                    thread::sleep(*duration);
                    Ok(())
                }
                StepAction::Foreground => {
                    self.adb.launch_app(&self.package_name, &self.main_activity)
                }
                StepAction::KillApp => {
                    self.adb.force_stop_app(&self.package_name)
                }
                StepAction::WaitForElement(text) => {
                    if self.adb.find_element_by_text(text)? {
                        return Ok(());
                    }
                    Err("Element not found".to_string())
                }
            };
            
            if result.is_ok() {
                return result;
            }
            
            if start.elapsed() > timeout {
                return Err(format!("Timeout after {:?}", timeout));
            }
            
            thread::sleep(Duration::from_millis(100));
        }
    }
    
    fn start_metrics_collection(&self, pid: Option<u32>, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
        let adb = self.adb.clone();
        let metrics_history = self.metrics_history.clone();
        
        thread::spawn(move || {
            let interval = Duration::from_millis(500);
            
            while !stop.load(Ordering::Relaxed) {
                let mut metrics = TestMetrics {
                    timestamp: Utc::now(),
                    cpu_usage: 0.0,
                    memory_kb: 0,
                    battery_level: 0,
                    battery_temp: 0.0,
                    fps: 0.0,
                    frame_time_ms: 0.0,
                    network_rx_kb: 0,
                    network_tx_kb: 0,
                    thermal_throttling: false,
                    pid,
                };
                
                // Get CPU and memory for app
                if let Some(pid) = pid {
                    if let Ok(cpu) = adb.get_cpu_usage(pid) {
                        metrics.cpu_usage = cpu;
                    }
                    if let Ok(mem) = adb.get_memory_usage(pid) {
                        metrics.memory_kb = mem;
                    }
                }
                
                // Get battery info
                if let Ok(battery) = adb.get_battery_info() {
                    metrics.battery_level = battery.level;
                    metrics.battery_temp = battery.temperature;
                }
                
                // Get thermal info
                if let Ok(throttling) = adb.get_thermal_throttling() {
                    metrics.thermal_throttling = throttling;
                }
                
                // Store metrics
                metrics_history.write().unwrap().push_back(metrics);
                
                thread::sleep(interval);
            }
        })
    }
    
    fn get_device_info(&self) -> Result<DeviceInfo, String> {
        let props = self.adb.shell("getprop")?;
        
        let mut manufacturer = "Unknown".to_string();
        let mut model = "Unknown".to_string();
        let mut version = "Unknown".to_string();
        let mut sdk = 0;
        
        for line in props.lines() {
            if line.contains("ro.product.manufacturer") {
                manufacturer = line.split(':').nth(1).unwrap_or("").trim().replace('[', "").replace(']', "");
            } else if line.contains("ro.product.model") {
                model = line.split(':').nth(1).unwrap_or("").trim().replace('[', "").replace(']', "");
            } else if line.contains("ro.build.version.release") {
                version = line.split(':').nth(1).unwrap_or("").trim().replace('[', "").replace(']', "");
            } else if line.contains("ro.build.version.sdk") {
                if let Ok(val) = line.split(':').nth(1).unwrap_or("").trim().replace('[', "").replace(']', "").parse() {
                    sdk = val;
                }
            }
        }
        
        let resolution = self.adb.shell("wm size")?;
        let resolution = resolution.split(':').nth(1).unwrap_or("").trim().to_string();
        
        let battery = self.adb.get_battery_info()?;
        
        let meminfo = self.adb.shell("cat /proc/meminfo")?;
        let mut total_ram = 0;
        let mut available_ram = 0;
        
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                total_ram = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            } else if line.starts_with("MemAvailable:") {
                available_ram = line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
            }
        }
        
        let storage = self.adb.shell("df /data")?;
        let mut total_storage = 0;
        let mut free_storage = 0;
        
        for line in storage.lines() {
            if line.contains("/data") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    total_storage = parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
                    free_storage = parts.get(3).unwrap_or(&"0").parse().unwrap_or(0);
                }
            }
        }
        
        Ok(DeviceInfo {
            manufacturer,
            model,
            android_version: version,
            sdk_version: sdk,
            screen_resolution: resolution,
            battery_capacity: 4000, // Not easily accessible via ADB
            total_ram_mb: total_ram / 1024,
            available_ram_mb: available_ram / 1024,
            internal_storage_mb: total_storage / 1024,
            free_storage_mb: free_storage / 1024,
        })
    }
    
    fn calculate_summary(&self, steps: &[TestStepResult], metrics: &[TestMetrics]) -> TestSummary {
        let total_steps = steps.len();
        let passed = steps.iter().filter(|s| s.status == StepStatus::Passed).count();
        let failed = steps.iter().filter(|s| s.status == StepStatus::Failed).count();
        let skipped = steps.iter().filter(|s| s.status == StepStatus::Skipped).count();
        let debug_breaks = steps.iter().filter(|s| s.status == StepStatus::DebugBreak).count();
        
        let avg_cpu = if !metrics.is_empty() {
            metrics.iter().map(|m| m.cpu_usage).sum::<f32>() / metrics.len() as f32
        } else {
            0.0
        };
        
        let max_cpu = metrics.iter().map(|m| m.cpu_usage).fold(0.0, f32::max);
        
        let avg_memory_mb = if !metrics.is_empty() {
            metrics.iter().map(|m| m.memory_kb).sum::<u64>() as f64 / metrics.len() as f64 / 1024.0
        } else {
            0.0
        };
        
        let max_memory_mb = metrics.iter().map(|m| m.memory_kb).max().unwrap_or(0) as f64 / 1024.0;
        
        let battery_drain = if metrics.len() >= 2 {
            metrics.first().unwrap().battery_level as f32 - metrics.last().unwrap().battery_level as f32
        } else {
            0.0
        };
        
        let max_temperature = metrics.iter().map(|m| m.battery_temp).fold(0.0, f32::max);
        
        TestSummary {
            total_steps,
            passed,
            failed,
            skipped,
            debug_breaks,
            avg_cpu,
            max_cpu,
            avg_memory_mb,
            max_memory_mb,
            battery_drain_percent: battery_drain,
            max_temperature,
        }
    }
    
    fn save_report(&self, report: &TestReport) -> Result<(), String> {
        let report_json = serde_json::to_string_pretty(report).unwrap();
        
        let local_report_dir = PathBuf::from("test_reports");
        fs::create_dir_all(&local_report_dir).ok();
        
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let report_file = local_report_dir.join(format!("test_report_{}.json", timestamp));
        
        fs::write(&report_file, report_json).map_err(|e| e.to_string())?;
        
        // Generate HTML report
        self.generate_html_report(report, &local_report_dir.join(format!("test_report_{}.html", timestamp)))?;
        
        println!("\nReports saved to: {:?}", report_file);
        
        // Push to device if connected
        if self.adb.connected {
            let device_report_dir = format!("/sdcard/Android/data/{}/reports", self.package_name);
            self.adb.shell(&format!("mkdir -p {}", device_report_dir)).ok();
            self.adb.push(report_file.to_str().unwrap(), &format!("{}/report_{}.json", device_report_dir, timestamp)).ok();
        }
        
        Ok(())
    }
    
    fn generate_html_report(&self, report: &TestReport, path: &Path) -> Result<(), String> {
        let mut html = String::new();
        
        html.push_str(r#"<!DOCTYPE html>
<html>
<head>
    <title>Mobile E2E Test Report</title>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            margin: 0;
            padding: 20px;
            background: #f5f5f5;
            color: #333;
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
        }
        .header {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 30px;
            border-radius: 10px;
            margin-bottom: 20px;
            box-shadow: 0 4px 6px rgba(0,0,0,0.1);
        }
        .header h1 {
            margin: 0;
            font-size: 28px;
        }
        .header .timestamp {
            margin-top: 10px;
            opacity: 0.9;
        }
        .status-badge {
            display: inline-block;
            padding: 5px 15px;
            border-radius: 20px;
            font-weight: bold;
            text-transform: uppercase;
            font-size: 14px;
        }
        .status-passed {
            background: #4caf50;
            color: white;
        }
        .status-failed {
            background: #f44336;
            color: white;
        }
        .status-debugged {
            background: #ff9800;
            color: white;
        }
        .summary-card {
            background: white;
            border-radius: 10px;
            padding: 20px;
            margin-bottom: 20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }
        .summary-stats {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin-top: 20px;
        }
        .stat-item {
            text-align: center;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 8px;
        }
        .stat-value {
            font-size: 32px;
            font-weight: bold;
            color: #667eea;
        }
        .stat-label {
            font-size: 14px;
            color: #666;
            margin-top: 5px;
        }
        .step {
            background: white;
            border-radius: 8px;
            margin-bottom: 10px;
            padding: 15px;
            border-left: 4px solid #ddd;
        }
        .step.passed {
            border-left-color: #4caf50;
        }
        .step.failed {
            border-left-color: #f44336;
        }
        .step.debugged {
            border-left-color: #ff9800;
        }
        .step-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        .step-name {
            font-weight: bold;
            font-size: 16px;
        }
        .step-status {
            padding: 3px 10px;
            border-radius: 15px;
            font-size: 12px;
            font-weight: bold;
        }
        .step-time {
            font-size: 12px;
            color: #999;
            margin-top: 5px;
        }
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
            gap: 10px;
            margin-top: 15px;
            padding: 10px;
            background: #f8f9fa;
            border-radius: 6px;
        }
        .metric {
            text-align: center;
        }
        .metric-label {
            font-size: 12px;
            color: #666;
        }
        .metric-value {
            font-size: 18px;
            font-weight: bold;
            color: #333;
        }
        .error-details {
            margin-top: 10px;
            padding: 10px;
            background: #fff3f3;
            border-radius: 4px;
            color: #d32f2f;
            font-family: monospace;
            font-size: 12px;
        }
        .device-info {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
            gap: 10px;
            margin-top: 15px;
        }
        .info-item {
            background: #f8f9fa;
            padding: 8px 12px;
            border-radius: 6px;
        }
        .info-label {
            font-size: 11px;
            color: #666;
            text-transform: uppercase;
        }
        .info-value {
            font-size: 14px;
            font-weight: 500;
            margin-top: 2px;
        }
        .chart-container {
            height: 300px;
            margin: 20px 0;
        }
        .button {
            display: inline-block;
            padding: 8px 16px;
            background: #667eea;
            color: white;
            text-decoration: none;
            border-radius: 4px;
            font-size: 14px;
            margin-right: 10px;
        }
        .button:hover {
            background: #5a6fd8;
        }
        .logs {
            margin-top: 15px;
            padding: 10px;
            background: #1e1e1e;
            color: #d4d4d4;
            font-family: 'Monaco', 'Menlo', monospace;
            font-size: 12px;
            border-radius: 6px;
            max-height: 200px;
            overflow-y: auto;
        }
        .log-line {
            white-space: pre-wrap;
            border-bottom: 1px solid #333;
            padding: 2px 0;
        }
        @media (max-width: 768px) {
            .summary-stats {
                grid-template-columns: 1fr;
            }
            .metrics-grid {
                grid-template-columns: 1fr 1fr;
            }
        }
    </style>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📱 Mobile E2E Test Report</h1>
            <div class="timestamp">Generated: "#);
        
        html.push_str(&Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        
        html.push_str(&format!(r#"</div>
            <div style="margin-top: 20px;">
                <span class="status-badge status-{}">{}</span>
            </div>
        </div>
        
        <div class="summary-card">
            <h2>Test Summary</h2>
            <div class="summary-stats">
                <div class="stat-item">
                    <div class="stat-value">{}</div>
                    <div class="stat-label">Total Steps</div>
                </div>
                <div class="stat-item">
                    <div class="stat-value" style="color: #4caf50;">{}</div>
                    <div class="stat-label">Passed</div>
                </div>
                <div class="stat-item">
                    <div class="stat-value" style="color: #f44336;">{}</div>
                    <div class="stat-label">Failed</div>
                </div>
                <div class="stat-item">
                    <div class="stat-value" style="color: #ff9800;">{}</div>
                    <div class="stat-label">Debugged</div>
                </div>
            </div>
        </div>
        
        <div class="summary-card">
            <h2>Performance Metrics</h2>
            <div class="metrics-grid">
                <div class="metric">
                    <div class="metric-label">Avg CPU</div>
                    <div class="metric-value">{:.1}%</div>
                </div>
                <div class="metric">
                    <div class="metric-label">Max CPU</div>
                    <div class="metric-value">{:.1}%</div>
                </div>
                <div class="metric">
                    <div class="metric-label">Avg Memory</div>
                    <div class="metric-value">{:.1f} MB</div>
                </div>
                <div class="metric">
                    <div class="metric-label">Max Memory</div>
                    <div class="metric-value">{:.1f} MB</div>
                </div>
                <div class="metric">
                    <div class="metric-label">Battery Drain</div>
                    <div class="metric-value">{:.1f}%</div>
                </div>
                <div class="metric">
                    <div class="metric-label">Max Temp</div>
                    <div class="metric-value">{:.1f}°C</div>
                </div>
            </div>
            <div class="chart-container">
                <canvas id="metricsChart"></canvas>
            </div>
        </div>
        
        <div class="summary-card">
            <h2>Device Information</h2>
            <div class="device-info">
                <div class="info-item">
                    <div class="info-label">Manufacturer</div>
                    <div class="info-value">{}</div>
                </div>
                <div class="info-item">
                    <div class="info-label">Model</div>
                    <div class="info-value">{}</div>
                </div>
                <div class="info-item">
                    <div class="info-label">Android Version</div>
                    <div class="info-value">{} (API {})</div>
                </div>
                <div class="info-item">
                    <div class="info-label">Screen</div>
                    <div class="info-value">{}</div>
                </div>
                <div class="info-item">
                    <div class="info-label">RAM</div>
                    <div class="info-value">{} MB total / {} MB free</div>
                </div>
                <div class="info-item">
                    <div class="info-label">Storage</div>
                    <div class="info-value">{} MB total / {} MB free</div>
                </div>
                <div class="info-item">
                    <div class="info-label">Battery</div>
                    <div class="info-value">{} mAh</div>
                </div>
            </div>
        </div>
        
        <div class="summary-card">
            <h2>Test Steps</h2>
            <div class="steps">"#,
            match report.status {
                TestStatus::Passed => "passed",
                TestStatus::Failed => "failed",
                TestStatus::Debugged => "debugged",
            },
            match report.status {
                TestStatus::Passed => "✅ PASSED",
                TestStatus::Failed => "❌ FAILED",
                TestStatus::Debugged => "🔧 DEBUGGED",
            },
            report.summary.total_steps,
            report.summary.passed,
            report.summary.failed,
            report.summary.debug_breaks,
            report.summary.avg_cpu,
            report.summary.max_cpu,
            report.summary.avg_memory_mb,
            report.summary.max_memory_mb,
            report.summary.battery_drain_percent,
            report.summary.max_temperature,
            report.device_info.manufacturer,
            report.device_info.model,
            report.device_info.android_version,
            report.device_info.sdk_version,
            report.device_info.screen_resolution,
            report.device_info.total_ram_mb,
            report.device_info.available_ram_mb,
            report.device_info.internal_storage_mb,
            report.device_info.free_storage_mb,
            report.device_info.battery_capacity,
        ));
        
        for step in &report.steps {
            let status_class = match step.status {
                StepStatus::Passed => "passed",
                StepStatus::Failed => "failed",
                StepStatus::Debugged => "debugged",
                StepStatus::Skipped => "skipped",
            };
            
            html.push_str(&format!(r#"
                <div class="step {}">
                    <div class="step-header">
                        <span class="step-name">{}</span>
                        <span class="step-status">{}</span>
                    </div>
                    <div class="step-time">
                        {} → {} ({:.1}s)
                    </div>"#,
                status_class,
                step.name,
                match step.status {
                    StepStatus::Passed => "✅ Passed",
                    StepStatus::Failed => "❌ Failed",
                    StepStatus::Debugged => "🔧 Debugged",
                    StepStatus::Skipped => "⏭️ Skipped",
                },
                step.start_time.format("%H:%M:%S"),
                step.end_time.format("%H:%M:%S"),
                step.duration.as_secs_f64(),
            ));
            
            if let Some(error) = &step.error {
                html.push_str(&format!(r#"
                    <div class="error-details">
                        <strong>Error:</strong> {}
                    </div>"#, error));
            }
            
            if let Some(screenshot) = &step.screenshot {
                html.push_str(&format!(r#"
                    <div style="margin-top: 10px;">
                        <a href="{}" class="button" target="_blank">📸 View Screenshot</a>
                    </div>"#, screenshot.display()));
            }
            
            if !step.metrics.is_empty() {
                html.push_str(r#"<div class="metrics-grid">"#);
                
                let avg_cpu = step.metrics.iter().map(|m| m.cpu_usage).sum::<f32>() / step.metrics.len() as f32;
                let max_cpu = step.metrics.iter().map(|m| m.cpu_usage).fold(0.0, f32::max);
                let avg_mem = step.metrics.iter().map(|m| m.memory_kb).sum::<u64>() / step.metrics.len() as u64;
                let max_mem = step.metrics.iter().map(|m| m.memory_kb).max().unwrap_or(0);
                
                html.push_str(&format!(r#"
                    <div class="metric">
                        <div class="metric-label">Avg CPU</div>
                        <div class="metric-value">{:.1}%</div>
                    </div>
                    <div class="metric">
                        <div class="metric-label">Max CPU</div>
                        <div class="metric-value">{:.1}%</div>
                    </div>
                    <div class="metric">
                        <div class="metric-label">Avg Memory</div>
                        <div class="metric-value">{:.1f} MB</div>
                    </div>
                    <div class="metric">
                        <div class="metric-label">Max Memory</div>
                        <div class="metric-value">{:.1f} MB</div>
                    </div>
                "#, avg_cpu, max_cpu, avg_mem as f64 / 1024.0, max_mem as f64 / 1024.0));
                
                html.push_str("</div>");
            }
            
            if !step.logs.is_empty() {
                html.push_str(r#"<div class="logs">"#);
                for log in step.logs.iter().take(20) {
                    html.push_str(&format!(r#"<div class="log-line">{}</div>"#, 
                        log.replace("<", "&lt;").replace(">", "&gt;")));
                }
                if step.logs.len() > 20 {
                    html.push_str(&format!(r#"<div class="log-line">... and {} more lines</div>"#, step.logs.len() - 20));
                }
                html.push_str("</div>");
            }
            
            html.push_str("</div>");
        }
        
        if let Some(log_file) = &report.log_file {
            html.push_str(&format!(r#"
        <div class="summary-card">
            <h2>Logs & Recordings</h2>
            <a href="{}" class="button" target="_blank">📋 View Full Logcat</a>"#, log_file.display()));
            
            if let Some(video_file) = &report.video_file {
                html.push_str(&format!(r#"
            <a href="{}" class="button" target="_blank">🎥 View Screen Recording</a>"#, video_file.display()));
            }
            
            html.push_str("</div>");
        }
        
        html.push_str(r#"
    </div>
    
    <script>
        const ctx = document.getElementById('metricsChart').getContext('2d');
        const chart = new Chart(ctx, {
            type: 'line',
            data: {
                labels: ["#);
        
        if let Some(first_step) = report.steps.first() {
            let timestamps: Vec<String> = first_step.metrics.iter()
                .enumerate()
                .map(|(i, _)| format!("{}s", i))
                .collect();
            html.push_str(&timestamps.join(","));
        }
        
        html.push_str(r#"],
                datasets: [
                    {
                        label: 'CPU Usage %',
                        data: ["#);
        
        if let Some(first_step) = report.steps.first() {
            let cpu_data: Vec<String> = first_step.metrics.iter()
                .map(|m| m.cpu_usage.to_string())
                .collect();
            html.push_str(&cpu_data.join(","));
        }
        
        html.push_str(r#"],
                        borderColor: 'rgb(255, 99, 132)',
                        tension: 0.1
                    },
                    {
                        label: 'Memory (MB)',
                        data: ["#);
        
        if let Some(first_step) = report.steps.first() {
            let mem_data: Vec<String> = first_step.metrics.iter()
                .map(|m| (m.memory_kb as f64 / 1024.0).to_string())
                .collect();
            html.push_str(&mem_data.join(","));
        }
        
        html.push_str(r#"],
                        borderColor: 'rgb(54, 162, 235)',
                        tension: 0.1
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                scales: {
                    y: {
                        beginAtZero: true
                    }
                }
            }
        });
    </script>
</body>
</html>"#);
        
        fs::write(path, html).map_err(|e| e.to_string())?;
        
        Ok(())
    }
}

enum StepAction {
    Wait(Duration),
    Tap(u32, u32),
    Swipe(u32, u32, u32, u32, u32),
    TypeText(String),
    PressKey(String),
    NetworkChange(NetworkType),
    BatteryLevel(u32),
    Background(Duration),
    Foreground,
    KillApp,
    WaitForElement(String),
}

enum NetworkType {
    WiFi,
    Mobile4G,
    Mobile3G,
    AirplaneMode,
    NoNetwork,
}

struct TestStep {
    name: String,
    action: StepAction,
    timeout: Duration,
    expected: String,
}

// ============= Main Test =============

#[test]
fn test_mobile_app_with_adb_debug() {
    println!("{}", "=".repeat(80));
    println!("{:^80}", "MOBILE E2E TEST WITH ADB DEBUGGING");
    println!("{}", "=".repeat(80));
    
    let package_name = "com.example.app";
    let main_activity = "com.example.app.MainActivity";
    
    let mut tester = MobileE2ETestWithADB::new(package_name, main_activity);
    
    if !tester.adb.connected {
        eprintln!("No Android device connected! Tests will be skipped.");
        return;
    }
    
    println!("\nConnected to device. Running tests...\n");
    
    // Run test in normal mode
    let report = tester.run_test_with_debug("Main Test Suite", false).expect("Test failed");
    
    // Print summary
    println!("\n{}", "=".repeat(80));
    println!("Test Summary:");
    println!("  Total Steps: {}", report.summary.total_steps);
    println!("  Passed: {}", report.summary.passed);
    println!("  Failed: {}", report.summary.failed);
    println!("  Debug Breaks: {}", report.summary.debug_breaks);
    println!("  Duration: {:?}", report.duration);
    println!("\nPerformance:");
    println!("  Avg CPU: {:.1}%", report.summary.avg_cpu);
    println!("  Max CPU: {:.1}%", report.summary.max_cpu);
    println!("  Avg Memory: {:.1f} MB", report.summary.avg_memory_mb);
    println!("  Battery Drain: {:.1f}%", report.summary.battery_drain_percent);
    println!("  Max Temperature: {:.1f}°C", report.summary.max_temperature);
    println!("{}", "=".repeat(80));
    
    assert!(report.summary.failed == 0, "Test failed with {} failures", report.summary.failed);
}

#[test]
fn test_mobile_app_interactive_debug() {
    println!("{}", "=".repeat(80));
    println!("{:^80}", "INTERACTIVE DEBUG MODE");
    println!("{}", "=".repeat(80));
    println!("This test will pause at each step for debugging.");
    println!("Type 'help' for available commands.");
    
    let package_name = "com.example.app";
    let main_activity = "com.example.app.MainActivity";
    
    let mut tester = MobileE2ETestWithADB::new(package_name, main_activity);
    
    if !tester.adb.connected {
        eprintln!("No Android device connected! Cannot debug.");
        return;
    }
    
    // Run test in debug mode
    let report = tester.run_test_with_debug("Debug Session", true).expect("Debug session failed");
    
    println!("\n{}", "=".repeat(80));
    println!("Debug session completed");
    println!("Report saved to test_reports/");
    println!("{}", "=".repeat(80));
}