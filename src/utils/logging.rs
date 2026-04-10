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

    #[test]
    fn test_level_filter_for_verbosity() {
        assert_eq!(level_filter_for_verbosity(0), LevelFilter::INFO);
        assert_eq!(level_filter_for_verbosity(1), LevelFilter::DEBUG);
        assert_eq!(level_filter_for_verbosity(2), LevelFilter::TRACE);
        assert_eq!(level_filter_for_verbosity(9), LevelFilter::TRACE);
    }
}
