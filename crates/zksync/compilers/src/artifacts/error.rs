//! zksolc error from std json output
use foundry_compilers_artifacts_solc::error::{Severity, SourceLocation};

use core::iter::Peekable;
use foundry_compilers_artifacts_solc::serde_helpers;
use serde::{Deserialize, Serialize};
use std::{fmt, ops::Range};
use yansi::{Color, Style};

/// The `solc --standard-json` output error.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// The component type.
    pub component: String,
    /// The error code.
    #[serde(default, with = "serde_helpers::display_from_str_opt")]
    pub error_code: Option<u64>,
    /// The formatted error message.
    pub formatted_message: Option<String>,
    /// The non-formatted error message.
    pub message: String,
    /// The error severity.
    pub severity: Severity,
    /// The error location data.
    pub source_location: Option<SourceLocation>,
    /// The error type.
    pub r#type: String,
}

impl Error {
    /// Returns `true` if the error is an error.
    pub const fn is_error(&self) -> bool {
        self.severity.is_error()
    }

    /// Returns `true` if the error is a warning.
    pub const fn is_warning(&self) -> bool {
        self.severity.is_warning()
    }

    /// Returns `true` if the error is an info.
    pub const fn is_info(&self) -> bool {
        self.severity.is_info()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fmtd_msg = self.formatted_message.as_deref().unwrap_or("");

        // Format the severity level
        styled(f, self.severity.color().bold(), |f| self.fmt_severity(f))?;

        let mut lines = fmtd_msg.lines().peekable();

        // Skip the first line if it contains the same message as severity,
        // unless it includes a source location (denoted by 3+ colons) something like:
        // path/to/file:line:column: ErrorType: message
        if let Some(l) = lines.peek() {
            if l.contains(self.severity.to_string().as_str()) &&
                l.bytes().filter(|b| *b == b':').count() < 3
            {
                lines.next();
            }
        }

        // Format the main source location
        fmt_source_location(f, &mut lines)?;

        // Process additional message lines
        while let Some(line) = lines.next() {
            // Use carriage return instead of newline to refresh the same line
            f.write_str("\r")?;

            match line.split_once(':') {
                Some((note, msg)) => {
                    styled(f, Self::secondary_style(), |f| f.write_str(note))?;
                    fmt_msg(f, msg)?;
                }
                None => f.write_str(line)?,
            }

            fmt_source_location(f, &mut lines)?;
        }

        Ok(())
    }
}

impl Error {
    /// The style of the diagnostic severity.
    pub fn error_style(&self) -> Style {
        self.severity.color().bold()
    }

    /// The style of the diagnostic message.
    pub fn message_style() -> Style {
        Color::White.bold()
    }

    /// The style of the secondary source location.
    pub fn secondary_style() -> Style {
        Color::Cyan.bold()
    }

    /// The style of the source location highlight.
    pub fn highlight_style() -> Style {
        Style::new().fg(Color::Yellow)
    }

    /// The style of the diagnostics.
    pub fn diag_style() -> Style {
        Color::Yellow.bold()
    }

    /// The style of the source location frame.
    pub fn frame_style() -> Style {
        Style::new().fg(Color::Blue)
    }

    /// Formats the diagnostic severity:
    ///
    /// ```text
    /// Error (XXXX)
    /// ```
    fn fmt_severity(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.severity.as_str())?;
        if let Some(code) = self.error_code {
            write!(f, " ({code})")?;
        }
        Ok(())
    }
}

/// Formats the diagnostic message.
fn fmt_msg(f: &mut fmt::Formatter<'_>, msg: &str) -> fmt::Result {
    styled(f, Error::message_style(), |f| {
        f.write_str(": ")?;
        f.write_str(msg.trim_start())
    })
}

fn fmt_source_location(
    f: &mut fmt::Formatter<'_>,
    lines: &mut Peekable<std::str::Lines<'_>>,
) -> fmt::Result {
    // --> source
    if let Some(line) = lines.next() {
        f.write_str("\n")?;

        let arrow = "-->";
        if let Some((left, loc)) = line.split_once(arrow) {
            f.write_str(left)?;
            styled(f, Error::frame_style(), |f| f.write_str(arrow))?;
            f.write_str(loc)?;
        } else {
            f.write_str(line)?;
        }
    }

    // get the next 3 lines
    let Some(line1) = lines.next() else {
        return Ok(());
    };
    let Some(line2) = lines.next() else {
        f.write_str("\n")?;
        f.write_str(line1)?;
        return Ok(());
    };
    let Some(line3) = lines.next() else {
        f.write_str("\n")?;
        f.write_str(line1)?;
        f.write_str("\n")?;
        f.write_str(line2)?;
        return Ok(());
    };

    // line 1, just a frame
    fmt_framed_location(f, line1, None)?;

    // line 2, frame and code; highlight the text based on line 3's carets
    let hl_start = line3.find('^');
    let highlight = hl_start.map(|start| {
        let end = if line3.contains("^ (") {
            // highlight the entire line because of "spans across multiple lines" diagnostic
            line2.len()
        } else if let Some(carets) = line3[start..].find(|c: char| c != '^') {
            // highlight the text that the carets point to
            start + carets
        } else {
            // the carets span the entire third line
            line3.len()
        }
        // bound in case carets span longer than the code they point to
        .min(line2.len());
        (start.min(end)..end, Error::highlight_style())
    });
    fmt_framed_location(f, line2, highlight)?;

    // line 3, frame and maybe highlight, this time till the end unconditionally
    let highlight = hl_start.map(|i| (i..line3.len(), Error::diag_style()));
    fmt_framed_location(f, line3, highlight)
}

/// Colors a single Solidity framed source location line. Part of [`fmt_source_location`].
fn fmt_framed_location(
    f: &mut fmt::Formatter<'_>,
    line: &str,
    highlight: Option<(Range<usize>, Style)>,
) -> fmt::Result {
    f.write_str("\n")?;

    if let Some((space_or_line_number, rest)) = line.split_once('|') {
        // if the potential frame is not just whitespace or numbers, don't color it
        if !space_or_line_number.chars().all(|c| c.is_whitespace() || c.is_numeric()) {
            return f.write_str(line);
        }

        styled(f, Error::frame_style(), |f| {
            f.write_str(space_or_line_number)?;
            f.write_str("|")
        })?;

        if let Some((range, style)) = highlight {
            let Range { start, end } = range;
            // Skip highlighting if the range is not valid unicode.
            if !line.is_char_boundary(start) || !line.is_char_boundary(end) {
                f.write_str(rest)
            } else {
                let rest_start = line.len() - rest.len();
                f.write_str(&line[rest_start..start])?;
                styled(f, style, |f| f.write_str(&line[range]))?;
                f.write_str(&line[end..])
            }
        } else {
            f.write_str(rest)
        }
    } else {
        f.write_str(line)
    }
}

/// Calls `fun` in between [`Style::fmt_prefix`] and [`Style::fmt_suffix`].
fn styled<F>(f: &mut fmt::Formatter<'_>, style: Style, fun: F) -> fmt::Result
where
    F: FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    let enabled = yansi::is_enabled();
    if enabled {
        style.fmt_prefix(f)?;
    }
    fun(f)?;
    if enabled {
        style.fmt_suffix(f)?;
    }
    Ok(())
}
