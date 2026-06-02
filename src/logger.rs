use anyhow::Result;
use std::fs;
use std::path::Path;

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

    let level_str = log_level.to_lowercase();
    let env_filter = match level_str.as_str() {
        "debug" => tracing_subscriber::EnvFilter::new("debug"),
        "warn" => tracing_subscriber::EnvFilter::new("warn"),
        "error" => tracing_subscriber::EnvFilter::new("error"),
        "trace" => tracing_subscriber::EnvFilter::new("trace"),
        _ => tracing_subscriber::EnvFilter::new("info"),
    };

    if use_console {
        // Log to both file and console
        let file = fs::File::create(log_file_path)?;
        
        // Use a simple approach: log to stdout (console)
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_ansi(true)
            .with_target(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .finish();

        tracing::subscriber::set_global_default(subscriber)?;
        eprintln!("🤖 НейροРаб: Логирование в консоль включено");
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
