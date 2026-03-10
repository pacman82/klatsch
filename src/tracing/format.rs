use std::fmt;

use nu_ansi_term::{Color, Style};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    fmt::{FmtContext, FormatFields, FormattedFields, format::Writer, time::FormatTime},
    registry::LookupSpan,
};

use super::operator_target;

/// Formats log events for operators. Replaces the default formatter to control how targets are
/// displayed — internal crate names like `tower_http::trace` are remapped to operator-friendly
/// names via [`operator_target`].
pub struct OperatorFormat;

impl<S, N> tracing_subscriber::fmt::FormatEvent<S, N> for OperatorFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> tracing_subscriber::fmt::FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let ansi = writer.has_ansi_escapes();

        // Timestamp
        let dimmed = if ansi {
            Style::new().dimmed()
        } else {
            Style::new()
        };
        if ansi {
            write!(writer, "{}", dimmed.prefix())?;
        }
        if tracing_subscriber::fmt::time::SystemTime
            .format_time(&mut writer)
            .is_err()
        {
            writer.write_str("<unknown time>")?;
        }
        if ansi {
            write!(writer, "{} ", dimmed.suffix())?;
        } else {
            writer.write_str(" ")?;
        }

        // Level
        let level_str = match *meta.level() {
            Level::TRACE => "TRACE",
            Level::DEBUG => "DEBUG",
            Level::INFO => " INFO",
            Level::WARN => " WARN",
            Level::ERROR => "ERROR",
        };
        if ansi {
            let color = match *meta.level() {
                Level::TRACE => Color::Purple,
                Level::DEBUG => Color::Blue,
                Level::INFO => Color::Green,
                Level::WARN => Color::Yellow,
                Level::ERROR => Color::Red,
            };
            write!(writer, "{} ", color.paint(level_str))?;
        } else {
            write!(writer, "{level_str} ")?;
        }

        // Span context
        if let Some(scope) = ctx.event_scope() {
            let bold = if ansi {
                Style::new().bold()
            } else {
                Style::new()
            };
            let mut seen = false;
            for span in scope.from_root() {
                write!(writer, "{}", bold.paint(span.metadata().name()))?;
                seen = true;
                let ext = span.extensions();
                if let Some(fields) = &ext.get::<FormattedFields<N>>() {
                    if !fields.is_empty() {
                        write!(writer, "{}{}{}", bold.paint("{"), fields, bold.paint("}"))?;
                    }
                }
                write!(writer, "{}", dimmed.paint(":"))?;
            }
            if seen {
                writer.write_str(" ")?;
            }
        }

        // Target
        let target = operator_target(meta.target());
        write!(writer, "{}{} ", dimmed.paint(target), dimmed.paint(":"))?;

        // Fields and message
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}
