#![allow(dead_code)]

use anyhow::{Context, Result};
use chrono::Local;
use nu_ansi_term::{Color, Style};
use std::fmt;
use std::fs::File;
use std::io::BufWriter;
use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields},
    prelude::*,
    registry::LookupSpan,
    Layer,
};

pub fn setup_logger() -> Option<WorkerGuard> {
    // Create logs directory
    std::fs::create_dir_all("logs").ok();

    // HOURLY rotation with aggressive cleanup for 10-20MB total limit
    // Each log file ~5-10MB max, keep only 2 files (current + previous hour)
    let file_appender = tracing_appender::rolling::hourly("logs", "app");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // File layer: INFO for task_result, WARN for others
    let file_filter = tracing_subscriber::filter::Targets::new()
        .with_target("task_result", tracing::Level::INFO)
        .with_default(tracing::Level::WARN);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .event_format(FileFormatter)
        .with_filter(file_filter);

    // Console layer: INFO for task_result, ERROR for others
    let console_filter = tracing_subscriber::filter::Targets::new()
        .with_target("task_result", tracing::Level::INFO)
        .with_default(tracing::Level::ERROR);

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .event_format(TerminalFormatter)
        .with_filter(console_filter);

    // Combine both layers
    tracing_subscriber::registry()
        .with(file_layer)
        .with(console_layer)
        .init();

    // Return guard - MUST be kept alive by caller
    Some(guard)
}

pub fn setup_logger_with_file(log_path: &str) -> Result<WorkerGuard> {
    let file = File::create(log_path).context("Failed to create log file")?;
    let (non_blocking, guard) = tracing_appender::non_blocking(BufWriter::new(file));

    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .event_format(FileFormatter),
    );

    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set global subscriber")?;

    Ok(guard)
}

// --- Formatters ---

struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

pub struct TerminalFormatter;

impl<S, N> FormatEvent<S, N> for TerminalFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        // Extract message
        let mut msg_visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut msg_visitor);
        let msg = msg_visitor.message;

        // Colorization for SUCCESS and FAILED
        let colored_msg = if msg.contains("SUCCESS") || msg.contains("Success") {
            let green_text = Style::new().fg(Color::LightGreen).bold();
            msg.replace("SUCCESS", &format!("{}", green_text.paint("SUCCESS")))
                .replace("Success", &format!("{}", green_text.paint("Success")))
        } else if msg.contains("FAILED") || msg.contains("Failed") {
            let red_text = Style::new().fg(Color::LightRed).bold();
            msg.replace("FAILED", &format!("{}", red_text.paint("FAILED")))
                .replace("Failed", &format!("{}", red_text.paint("Failed")))
        } else {
            msg
        };

        write!(writer, "{}", colored_msg)?;
        writeln!(writer)
    }
}

pub struct FileFormatter;

impl<S, N> FormatEvent<S, N> for FileFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let level = event.metadata().level();

        write!(writer, "{} [{}] ", timestamp, level)?;

        let mut msg_visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut msg_visitor);
        writeln!(writer, "{}", msg_visitor.message)
    }
}
