use anyhow::anyhow;
use anyhow::Result;
use fern::Dispatch;

use std::fs::OpenOptions;

use crate::common::ev;

fn log_level(level_str: &str) -> Result<log::LevelFilter> {
    match level_str {
        "off" => Ok(log::LevelFilter::Off),
        "trace" => Ok(log::LevelFilter::Trace),
        "debug" => Ok(log::LevelFilter::Debug),
        "info" => Ok(log::LevelFilter::Info),
        "warn" => Ok(log::LevelFilter::Warn),
        "error" => Ok(log::LevelFilter::Error),
        _ => Err(anyhow!(
            "file_log_level(): couldn't determine file log level.".to_owned()
        )),
    }
}

pub fn file_log_level() -> Result<log::LevelFilter> {
    let level = match ev("FILE_LOG_LEVEL") {
        Ok(lvl) => lvl,
        Err(_) => "off".to_owned(),
    };

    log_level(&level)
}

pub fn console_log_level() -> Result<log::LevelFilter> {
    let level = match ev("CONSOLE_LOG_LEVEL") {
        Ok(lvl) => lvl,
        Err(_) => "off".to_owned(),
    };

    log_level(&level)
}

pub fn setup_logging() -> Result<()> {
    // Configure logging to file
    let file_config = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(ev("LOG_FILE")?)?;

    let file_logger = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(file_log_level()?) // File gets Debug and higher
        .chain(file_config);

    // Configure logging to stdout
    let stdout_logger = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}] {}",
                chrono::Local::now().format("[%H:%M:%S]"),
                record.level(),
                message
            ))
        })
        .level(console_log_level()?) // Stdout gets Info and higher
        .chain(std::io::stdout());

    let app_module_filter = Dispatch::new()
        .filter(|metadata| metadata.target().starts_with("spark_matcher"))
        .chain(stdout_logger)
        .chain(file_logger);

    app_module_filter.apply()?;

    Ok(())
}
