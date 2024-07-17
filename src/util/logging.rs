use crate::config::ev;
use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use std::fs::OpenOptions;
/*
pub fn log_level(level_str: &str) -> Result<LevelFilter> {
    match level_str {
        "off" => Ok(LevelFilter::Off),
        "trace" => Ok(LevelFilter::Trace),
        "debug" => Ok(LevelFilter::Debug),
        "info" => Ok(LevelFilter::Info),
        "warn" => Ok(LevelFilter::Warn),
        "error" => Ok(LevelFilter::Error),
        _ => Err(anyhow!(
            "Couldn't determine file log level from '{}'",
            level_str
        )),
    }
}

pub fn file_log_level() -> Result<LevelFilter> {
    let level = ev("FILE_LOG_LEVEL").unwrap_or_else(|_| "off".to_string());
    log_level(&level)
}

pub fn console_log_level() -> Result<LevelFilter> {
    let level = ev("CONSOLE_LOG_LEVEL").unwrap_or_else(|_| "off".to_string());
    log_level(&level)
}

pub fn setup_logging() -> Result<()> {
    let file_level = file_log_level()?;
    let console_level = console_log_level()?;

    let file_config = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(ev("LOG_FILE")?)?;

    let file_logger = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(file_level)
        .chain(file_config);

    let stdout_logger = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}] {}",
                Local::now().format("[%H:%M:%S]"),
                record.level(),
                message
            ))
        })
        .level(console_level)
        .chain(std::io::stdout());

    let combined_logger = Dispatch::new().chain(stdout_logger).chain(file_logger);

    combined_logger.apply()?;

    Ok(())
}
*/
