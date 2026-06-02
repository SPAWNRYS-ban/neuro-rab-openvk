use anyhow::Result;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Combined writer that writes to both stdout and file
#[derive(Clone)]
struct DualWriter {
    file: Arc<Mutex<fs::File>>,
}

impl Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Write to stderr (console)
        let _ = io::stderr().write_all(buf);
        
        // Write to file
        let mut file = self.file.lock().unwrap();
        file.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        let mut file = self.file.lock().unwrap();
        file.flush()
    }
}

pub fn init_logger_dual(
    log_file_path: &str,
    log_level: &str,
    use_console: bool,
) -> Result<()> {
    // Create logs directory if it doesn't exist
    let log_path = Path::new(log_file_path);
    if let Some(parent) = log_path.parent() {
        if !parent.as_os_str().is_empty() && parent.to_str() != Some(".") {
            fs::create_dir_all(parent)?;
        }
    }

    // Bridge the `log` crate (used by log::info! in main.rs) into `tracing`.
    // Without this, ANY log::info!/error! call is silently dropped because our
    // subscriber only captures `tracing` events. `set_global_default` does NOT
    // install this bridge automatically (only `.init()` would), so we do it
    // explicitly here. Ignore the error if it's already set.
    let _ = tracing_log::LogTracer::init();

    let level_str = log_level.to_lowercase();

    let env_filter = match level_str.as_str() {
        "debug" => tracing_subscriber::EnvFilter::new("debug"),
        "warn" => tracing_subscriber::EnvFilter::new("warn"),
        "error" => tracing_subscriber::EnvFilter::new("error"),
        "trace" => tracing_subscriber::EnvFilter::new("trace"),
        _ => tracing_subscriber::EnvFilter::new("info"),
    };

    if use_console {
        // Log to BOTH file and console (dual logging)
        let file = fs::File::create(log_file_path)?;
        let dual_writer = DualWriter {
            file: Arc::new(Mutex::new(file)),
        };
        
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(move || dual_writer.clone())
            .with_ansi(true)
            .with_target(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .finish();

        tracing::subscriber::set_global_default(subscriber)?;
        eprintln!("🤖 НейроРаб: Логирование в консоль и файл включено (DUAL MODE)");
    } else {
        // Log only to file
        let file = fs::File::create(log_file_path)?;
        
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(file)
            .with_target(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_ansi(false)
            .finish();

        tracing::subscriber::set_global_default(subscriber)?;
    }

    log::info!("Logger initialized with level: {}", log_level);

    Ok(())
}
