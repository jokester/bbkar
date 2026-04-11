use std::fmt;

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

struct BbkarEventFormat;

impl<S, N> FormatEvent<S, N> for BbkarEventFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        let target = metadata.target();

        match *metadata.level() {
            Level::INFO => {
                if !target.starts_with("bbkar") {
                    write!(writer, "[{}] ", target)?;
                }
                ctx.field_format().format_fields(writer.by_ref(), event)?;
                writeln!(writer)
            }
            Level::WARN => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[33mWARN\x1b[0m ")?;
                } else {
                    write!(writer, "WARN ")?;
                }
                ctx.field_format().format_fields(writer.by_ref(), event)?;
                writeln!(writer)
            }
            Level::ERROR => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[31mERROR\x1b[0m ")?;
                } else {
                    write!(writer, "ERROR ")?;
                }
                ctx.field_format().format_fields(writer.by_ref(), event)?;
                writeln!(writer)
            }
            Level::DEBUG => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[34mDEBUG\x1b[0m ")?;
                } else {
                    write!(writer, "DEBUG ")?;
                }
                write!(writer, "[{}] ", target)?;
                ctx.field_format().format_fields(writer.by_ref(), event)?;
                writeln!(writer)
            }
            Level::TRACE => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[90mTRACE\x1b[0m ")?;
                } else {
                    write!(writer, "TRACE ")?;
                }
                write!(writer, "[{}] ", target)?;
                ctx.field_format().format_fields(writer.by_ref(), event)?;
                writeln!(writer)
            }
        }
    }
}

fn level_filter_for_verbosity(verbose: u8) -> LevelFilter {
    match verbose {
        0 => LevelFilter::INFO,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    }
}

pub fn init_tracing(verbose: u8) {
    let level = level_filter_for_verbosity(verbose);

    tracing_subscriber::fmt()
        .with_max_level(level)
        .without_time()
        .with_writer(std::io::stderr)
        .with_ansi(atty::is(atty::Stream::Stderr))
        .event_format(BbkarEventFormat)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing::{debug, error, info, trace, warn};
    use tracing_subscriber::fmt::MakeWriter;

    #[test]
    fn test_level_filter_for_verbosity() {
        assert_eq!(level_filter_for_verbosity(0), LevelFilter::INFO);
        assert_eq!(level_filter_for_verbosity(1), LevelFilter::DEBUG);
        assert_eq!(level_filter_for_verbosity(2), LevelFilter::TRACE);
        assert_eq!(level_filter_for_verbosity(9), LevelFilter::TRACE);
    }

    #[derive(Clone)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for SharedWriter {
        type Writer = SharedWriterGuard;

        fn make_writer(&'a self) -> Self::Writer {
            SharedWriterGuard(self.0.clone())
        }
    }

    struct SharedWriterGuard(Arc<Mutex<Vec<u8>>>);

    impl io::Write for SharedWriterGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn capture_logs(ansi: bool, emit: impl FnOnce()) -> String {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(LevelFilter::TRACE)
            .without_time()
            .with_ansi(ansi)
            .with_writer(SharedWriter(buffer.clone()))
            .event_format(BbkarEventFormat)
            .finish();

        tracing::subscriber::with_default(subscriber, emit);

        String::from_utf8(buffer.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn test_info_events_hide_bbkar_target_and_show_foreign_target() {
        let output = capture_logs(false, || {
            info!(target: "bbkar::cli", message = "from bbkar");
            info!(target: "external::tool", message = "from external");
        });

        assert!(output.contains("from bbkar"));
        assert!(!output.contains("[bbkar::cli]"));
        assert!(output.contains("[external::tool]"));
        assert!(output.contains("from external"));
    }

    #[test]
    fn test_warn_and_error_events_include_plain_prefixes_without_ansi() {
        let output = capture_logs(false, || {
            warn!(warning = "disk nearly full");
            error!(failure = "write failed");
        });

        assert!(output.contains("WARN"));
        assert!(output.contains("disk nearly full"));
        assert!(output.contains("ERROR"));
        assert!(output.contains("write failed"));
    }

    #[test]
    fn test_debug_and_trace_events_include_target_prefixes() {
        let output = capture_logs(false, || {
            debug!(target: "bbkar::planner", step = "plan");
            trace!(target: "bbkar::planner", step = "trace");
        });

        assert!(output.contains("DEBUG [bbkar::planner]"));
        assert!(output.contains("TRACE [bbkar::planner]"));
    }
}
