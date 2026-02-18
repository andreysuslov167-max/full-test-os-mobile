#![cfg(any(target_os = "android " , target_os = "ios"))]

use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write , Read , Seek , SeekFrom};
use std::sync::{Arc , Mutex , Barrier};
use std::thread
use std::collections::HashMap;


#[cfg(target_os = "android")]
mod android{
    pub use jni::objects::{JClass , JString , JObject};
    pub use jni::JNIEnv;
    pub use jni::sys::{jint, jlong , jboolean};
    
    pub fn get_android_context() -> String {

         String::from("android.app.Application")
    }  
    pub fn get_storage_state() -> Result<String , String>{
        Ok("mounted".to_string())
        
    }
}
fn get_mobile_app_dir()-> ParhBuf {
    #[cfg(target_os = "android")]
    {
        ParhBuf::from("/storage/emulated/0/Android/data")
            .join("com.example.app")
            .join("files")
            
    }

}
#[test]
fn test_mobile_app_lifecycle_e2e(){
    println!("====TEST-E2E====");
    let app_dir = get_mobile_app_dir();
    fs::create_dir_all(&app_dir).expect("failed to create app directory");

    let config_path = app_dir.join("config.json");
    let config_data = r#"{
    "app version": "1.0.0",
    "user_id": "user228",
    "settings":{
        "notficantions": true,
        "theme": "dark",
        "language": "en"

    }
    }"#
    fs::write(&config_path, config_data).expect("Failed to write config");
    assert!(config_path.exists(), "Config file should exist");




}