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