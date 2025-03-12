use std::sync::LazyLock;

use sysinfo::System;
use thiserror::Error;
use tokio::sync::Mutex;

static SYSTEM_INFO: LazyLock<Mutex<System>> = LazyLock::new(|| {
    let mut sys = System::new_all();
    sys.refresh_all();
    Mutex::new(sys)
});

#[derive(Debug, Error)]
pub enum SystemResourceMonitorError {
    #[error("Failed to get current pid: {0}")]
    GetCurrentPidError(&'static str),
}

async fn get_cpu_usage_percentage() -> Result<f32, SystemResourceMonitorError> {
    let mut sys = SYSTEM_INFO.lock().await;
    sys.refresh_cpu_usage();
    let cpu_usage = {
        let mut cpu_usage = 0.0;
        for cpu in sys.cpus() {
            cpu_usage += cpu.cpu_usage();
        }
        cpu_usage / sys.cpus().len() as f32
    };

    Ok(cpu_usage)
}

async fn get_memory_usage_percentage() -> Result<f32, SystemResourceMonitorError> {
    let mut sys = SYSTEM_INFO.lock().await;
    sys.refresh_memory();
    let memory_usage = sys.used_memory() as f64 / sys.total_memory() as f64;
    Ok(memory_usage as f32)
}

/// Get the current process used cpu and memory, CPU usage (in %), Memory usage (in bytes)
async fn get_current_process_used_cpu_and_memory() -> Result<(f32, u64), SystemResourceMonitorError>
{
    let mut sys = SYSTEM_INFO.lock().await;
    sys.refresh_all();

    let current_pid = match sysinfo::get_current_pid() {
        Ok(pid) => pid,
        Err(e) => return Err(SystemResourceMonitorError::GetCurrentPidError(e)),
    };

    let current_process = match sys.process(current_pid) {
        Some(process) => process,
        None => {
            return Err(SystemResourceMonitorError::GetCurrentPidError(
                "Process not found",
            ));
        }
    };

    Ok((current_process.cpu_usage(), current_process.memory()))
}
