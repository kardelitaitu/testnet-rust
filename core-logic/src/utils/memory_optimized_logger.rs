//! Memory-optimized logger with direct disk I/O and proper resource management

use anyhow::{Context, Result};
use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{prelude::*, registry::LookupSpan, Layer};

/// Configuration for memory-optimized logging
#[derive(Debug, Clone)]
pub struct MemoryOptimizedLoggerConfig {
    /// Maximum log file size in bytes before rotation
    pub max_file_size: u64,
    /// Maximum number of log files to keep
    pub max_files: usize,
    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,
    /// Buffer size for file writing
    pub buffer_size: usize,
}

impl Default for MemoryOptimizedLoggerConfig {
    fn default() -> Self {
        Self {
            max_file_size: 10 * 1024 * 1024, // 10MB
            max_files: 5,
            flush_interval_ms: 1000, // 1 second
            buffer_size: 8 * 1024,   // 8KB buffer
        }
    }
}

/// Memory-optimized file appender with rotation and buffering
pub struct MemoryOptimizedFileAppender {
    writer: Arc<Mutex<BufWriter<File>>>,
    current_file: String,
    config: MemoryOptimizedLoggerConfig,
    last_flush: Instant,
    bytes_written: u64,
}

impl MemoryOptimizedFileAppender {
    pub fn new(log_dir: &str, config: MemoryOptimizedLoggerConfig) -> Result<Self> {
        // Create log directory
        std::fs::create_dir_all(log_dir).context("Failed to create log directory")?;

        let file_path = Self::generate_file_path(log_dir);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .context("Failed to open log file")?;

        Ok(Self {
            writer: Arc::new(Mutex::new(BufWriter::with_capacity(config.buffer_size, file))),
            current_file: file_path,
            config,
            last_flush: Instant::now(),
            bytes_written: 0,
        })
    }

    fn generate_file_path(log_dir: &str) -> String {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        format!("{}/app_{}.log", log_dir, timestamp)
    }

    pub fn write(&mut self, message: &str) -> Result<()> {
        // Check if we need to rotate
        if self.bytes_written >= self.config.max_file_size {
            self.rotate()?;
        }

        let mut writer = self.writer.lock().unwrap();
        let line = format!("{} {}\n", Local::now().format("%Y-%m-%d %H:%M:%S"), message);
        let bytes = line.as_bytes();

        writer.write_all(bytes)?;
        self.bytes_written += bytes.len() as u64;

        // Auto-flush based on interval
        if self.last_flush.elapsed() > Duration::from_millis(self.config.flush_interval_ms) {
            writer.flush()?;
            self.last_flush = Instant::now();
        }

        Ok(())
    }

    fn rotate(&mut self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.flush()?;

        // Clean up old files
        self.cleanup_old_files()?;

        // Create new file
        let new_file_path = Self::generate_file_path(Path::new(&self.current_file).parent().unwrap().to_str().unwrap());

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_file_path)
            .context("Failed to create new log file")?;

        *writer = BufWriter::with_capacity(self.config.buffer_size, file);
        self.current_file = new_file_path;
        self.bytes_written = 0;
        self.last_flush = Instant::now();

        Ok(())
    }

    fn cleanup_old_files(&self) -> Result<()> {
        let log_dir = Path::new(&self.current_file).parent().unwrap();

        let mut log_files: Vec<_> = std::fs::read_dir(log_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file() && entry.path().extension().is_some_and(|ext| ext == "log"))
            .collect();

        // Sort by modified time (oldest first)
        log_files.sort_by_key(|entry| entry.metadata().ok().and_then(|m| m.modified().ok()));

        // Remove oldest files if we exceed the limit
        while log_files.len() > self.config.max_files {
            if let Some(oldest) = log_files.first() {
                std::fs::remove_file(oldest.path())?;
                log_files.remove(0);
            }
        }

        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.flush()?;
        Ok(())
    }
}

/// Memory-optimized logging layer
pub struct MemoryOptimizedLayer {
    file_appender: Arc<Mutex<MemoryOptimizedFileAppender>>,
}

impl MemoryOptimizedLayer {
    pub fn new(log_dir: &str, config: MemoryOptimizedLoggerConfig) -> Result<Self> {
        let file_appender = MemoryOptimizedFileAppender::new(log_dir, config)?;
        Ok(Self {
            file_appender: Arc::new(Mutex::new(file_appender)),
        })
    }
}

impl<S> Layer<S> for MemoryOptimizedLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // Only log INFO and above to file to save space/memory
        if event.metadata().level() <= &Level::INFO {
            let mut message = String::new();
            let mut visitor = MessageVisitor { message: &mut message };
            event.record(&mut visitor);

            if let Ok(mut appender) = self.file_appender.lock() {
                // Use FileFormatter style or similar
                let _ = appender.write(&message);
            }
        }
    }
}

struct MessageVisitor<'a> {
    message: &'a mut String,
}

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            *self.message = value.to_string();
        }
    }
}

/// Setup memory-optimized logger
pub fn setup_memory_optimized_logger() -> Result<()> {
    let config = MemoryOptimizedLoggerConfig::default();
    let layer = MemoryOptimizedLayer::new("logs", config)?;

    // Use filters similar to setup_logger
    let file_filter = tracing_subscriber::filter::Targets::new()
        .with_target("task_result", tracing::Level::INFO)
        .with_default(tracing::Level::WARN);

    let console_filter = tracing_subscriber::filter::Targets::new()
        .with_target("task_result", tracing::Level::INFO)
        .with_default(tracing::Level::ERROR);

    let subscriber = tracing_subscriber::registry()
        .with(layer.with_filter(file_filter))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_ansi(true)
                .event_format(crate::utils::logger::TerminalFormatter)
                .with_filter(console_filter),
        );

    tracing::subscriber::set_global_default(subscriber).context("Failed to set global subscriber")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_config_default_values() {
        let config = MemoryOptimizedLoggerConfig::default();
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
        assert_eq!(config.max_files, 5);
        assert_eq!(config.flush_interval_ms, 1000);
        assert_eq!(config.buffer_size, 8 * 1024);
    }

    #[test]
    fn test_config_custom_values() {
        let config = MemoryOptimizedLoggerConfig {
            max_file_size: 5_000_000,
            max_files: 3,
            flush_interval_ms: 500,
            buffer_size: 16_384,
        };
        assert_eq!(config.max_file_size, 5_000_000);
        assert_eq!(config.max_files, 3);
        assert_eq!(config.flush_interval_ms, 500);
        assert_eq!(config.buffer_size, 16_384);
    }

    #[test]
    fn test_generate_file_path_format() {
        let path = MemoryOptimizedFileAppender::generate_file_path("/tmp/test-logs");
        assert!(path.starts_with("/tmp/test-logs/app_"));
        assert!(path.ends_with(".log"));
        // Extract the timestamp portion: app_YYYYMMDD_HHMMSS.log
        let filename = Path::new(&path).file_name().unwrap().to_str().unwrap();
        assert_eq!(&filename[..4], "app_");
        assert_eq!(&filename[filename.len() - 4..], ".log");
        // Timestamp should be 15 chars: YYYYMMDD_HHMMSS
        let ts = &filename[4..filename.len() - 4];
        assert_eq!(ts.len(), 15);
        assert_eq!(&ts[8..9], "_");
        // All other chars should be digits
        for (i, c) in ts.char_indices() {
            if i != 8 {
                assert!(c.is_ascii_digit(), "expected digit at pos {} in '{}'", i, ts);
            }
        }
    }

    #[test]
    fn test_appender_new_in_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemoryOptimizedLoggerConfig::default();
        let appender = MemoryOptimizedFileAppender::new(dir.path().to_str().unwrap(), config);
        assert!(appender.is_ok());
        let appender = appender.unwrap();
        assert!(appender.current_file.contains(dir.path().to_str().unwrap()));
        assert_eq!(appender.bytes_written, 0);
    }

    #[test]
    fn test_appender_new_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("deep/nested/logs");
        let config = MemoryOptimizedLoggerConfig::default();
        let appender = MemoryOptimizedFileAppender::new(nested.to_str().unwrap(), config);
        assert!(appender.is_ok(), "should auto-create nested directories");
        assert!(nested.exists(), "directory should exist");
    }

    #[test]
    fn test_appender_write_and_verify_content() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemoryOptimizedLoggerConfig::default();
        let mut appender = MemoryOptimizedFileAppender::new(dir.path().to_str().unwrap(), config.clone()).unwrap();

        // Write some content
        let result = appender.write("test message");
        assert!(result.is_ok());
        assert!(appender.bytes_written > 0);

        // Write more content
        appender.write("second line").unwrap();
        assert!(appender.bytes_written > b"test message".len() as u64);

        // Flush and verify file exists
        appender.flush().unwrap();
        let file_path = &appender.current_file;
        assert!(Path::new(file_path).exists(), "log file should exist");
        let content = std::fs::read_to_string(file_path).unwrap();
        assert!(content.contains("test message"), "file should contain written message");
        assert!(content.contains("second line"), "file should contain second message");
        // Each line should have timestamp prefix
        for line in content.lines() {
            assert!(line.len() > 20, "each line should have timestamp + message: '{}'", line);
        }
    }

    #[test]
    fn test_appender_rotate_triggers_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemoryOptimizedLoggerConfig {
            max_file_size: 100,
            max_files: 2,
            flush_interval_ms: 1,
            buffer_size: 1024,
        };
        let mut appender = MemoryOptimizedFileAppender::new(dir.path().to_str().unwrap(), config).unwrap();

        for i in 0..10 {
            appender
                .write(&format!("line {} with some padding data here", i))
                .unwrap();
        }

        appender.flush().unwrap();
        assert!(Path::new(&appender.current_file).exists());

        let log_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
            .collect();

        assert!(log_files.len() <= 2, "at most 2 log files, got {}", log_files.len());
    }
}
