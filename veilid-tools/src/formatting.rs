use super::*;

use indent::{indent_all_by, indent_by};

//////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Indentation

/// Spaces prepended by [`indent_string`] and [`indent_all_string`]
pub const DEFAULT_INDENT_SPACES: usize = 4;

/// Indent all but the first line of `s`
#[must_use]
pub fn indent_string<S: AsRef<str>>(s: S) -> String {
    indent_by(DEFAULT_INDENT_SPACES, s.as_ref().to_owned())
}

/// Indent every line of `s`
#[must_use]
pub fn indent_all_string<S: AsRef<str>>(s: S) -> String {
    indent_all_by(DEFAULT_INDENT_SPACES, s.as_ref().to_owned())
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Newline stripping

/// Strip a trailing `\n` or `\r\n` from a string-like value.
pub trait StripTrailingNewline {
    /// Return the string with a single trailing `\n` or `\r\n` removed.
    fn strip_trailing_newline(&self) -> &str;
}

impl<T: AsRef<str>> StripTrailingNewline for T {
    fn strip_trailing_newline(&self) -> &str {
        self.as_ref()
            .strip_suffix("\r\n")
            .or_else(|| self.as_ref().strip_suffix('\n'))
            .unwrap_or(self.as_ref())
    }
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Human display formatting

/// Format a byte count as a human-readable string with binary (1024-based) GB/MB/KB/B units.
#[must_use]
pub fn human_byte_count<T: Into<u64>>(count: T) -> String {
    let count = count.into();
    if count >= 1024u64 * 1024u64 * 1024u64 {
        format!("{:.3}GB", (count / (1024u64 * 1024u64)) as f64 / 1024.0)
    } else if count >= 1024u64 * 1024u64 {
        format!("{:.3}MB", (count / 1024u64) as f64 / 1024.0)
    } else if count >= 1024u64 {
        format!("{:.3}KB", count as f64 / 1024.0)
    } else {
        format!("{}B", count)
    }
}

/// Format a byte slice for display: ASCII-printable data is shown as text, otherwise as shell-quoted
/// text or hex (whichever is shorter). Truncated to `truncate_len` bytes with a trailing `...` when set.
#[must_use]
pub fn human_byte_data<T: AsRef<[u8]>>(data: T, truncate_len: Option<usize>) -> String {
    let data = data.as_ref();

    // check if message body is ascii printable
    let mut printable = true;
    for c in data {
        if *c < 32 || *c > 126 {
            printable = false;
            break;
        }
    }

    let (data, truncated) = if let Some(truncate_len) = truncate_len {
        if data.len() > truncate_len {
            (&data[0..truncate_len], true)
        } else {
            (data, false)
        }
    } else {
        (data, false)
    };

    let strdata = if printable {
        String::from_utf8_lossy(data).to_string()
    } else {
        let sw = shell_words::quote(String::from_utf8_lossy(data).as_ref()).to_string();
        let always_hex = sw
            .chars()
            .any(|c| !c.is_ascii() || c.is_ascii_control() || c.is_ascii_graphic());
        let h = hex::encode(data);
        if always_hex || h.len() < sw.len() {
            h
        } else {
            sw
        }
    };
    if truncated {
        format!("{}...", strdata)
    } else {
        strdata
    }
}

/// Format [`fmt::Display`] values to [`String`], respecting a formatter-style alternate flag.
///
/// Implemented for [`fmt::Formatter`] (uses [`fmt::Formatter::alternate`]), [`DefaultDisplayFormat`]
/// (`{}` only), and [`AlternateDisplayFormat`] (`{:#}` only). List helpers match the behavior of the
/// former `ToMultilineString` / `ToTableString` traits from `veilid-core`.
pub trait FormatterToString {
    /// Format values to [`String`], respecting a formatter-style alternate flag.
    fn to_string<T: fmt::Display>(&self, value: T) -> String;

    /// Format `Option<>` values to [`String`], using [`FormatterToString::to_string`] for the inner value.
    fn to_string_opt<T: fmt::Display>(&self, value: Option<T>) -> String {
        if let Some(ts) = value {
            self.to_string(ts)
        } else {
            "None".to_string()
        }
    }

    /// One line per item, each formatted via [`FormatterToString::to_string`].
    fn to_multiline_string<'a, T, I>(&self, items: I) -> String
    where
        T: fmt::Display + 'a,
        I: IntoIterator<Item = T>,
    {
        let mut out = Vec::new();
        for x in items {
            out.push(self.to_string(x));
        }
        out.join("\n")
    }

    /// Like [`FormatterToString::to_multiline_string`], but each item is prefixed with its index and
    /// indented (four spaces per line of the item's string form).
    fn to_multiline_indexed_string<'a, T, I>(&self, items: I) -> String
    where
        T: fmt::Display + 'a,
        I: IntoIterator<Item = T>,
    {
        let mut out = Vec::new();
        for (n, x) in items.into_iter().enumerate() {
            let block = self.to_string(x);
            if block.contains('\n') {
                out.push(format!("{n}:"));
                out.push(indent_all_by(DEFAULT_INDENT_SPACES, block));
            } else {
                out.push(format!("{n}: {block}"));
            }
        }
        out.join("\n")
    }

    /// Format items as a bracketed table with default layout (32 columns, 4-space indent).
    fn to_table_string<T: fmt::Display>(&self, items: &[T]) -> String {
        self.to_table_string_custom(items, 32, 4, true, true)
    }

    /// Format items as a comma-separated table. `columns` items per row (0 disables wrapping),
    /// each row indented by `indent` spaces, optionally wrapped in `[]` and split across lines.
    fn to_table_string_custom<T: fmt::Display, I: IntoIterator<Item = T>>(
        &self,
        items: I,
        columns: usize,
        indent: usize,
        brackets: bool,
        newlines: bool,
    ) -> String {
        let strings: Vec<String> = items.into_iter().map(|s| self.to_string(s)).collect();
        table_string_from_strings(&strings, columns, indent, brackets, newlines)
    }
}

fn table_string_from_strings(
    strings: &[String],
    columns: usize,
    indent: usize,
    brackets: bool,
    newlines: bool,
) -> String {
    let len = strings.len();
    let mut col = 0usize;
    let mut out = String::new();
    let mut left = len;

    let use_columns = columns > 0 && columns < len;

    if brackets {
        out.push('[');
    }
    if use_columns && newlines {
        out.push('\n');
    }
    let indent_str = " ".repeat(indent);

    for sc in strings {
        if use_columns && col == 0 {
            out.push_str(&indent_str);
        }
        out.push_str(sc);
        col += 1;
        left -= 1;
        if left != 0 {
            out.push(',');
            if use_columns && col == columns {
                col = 0;
                out.push('\n');
            }
        }
    }
    if use_columns && newlines {
        out.push('\n');
    }
    if brackets {
        out.push(']');
    }
    out
}

impl FormatterToString for fmt::Formatter<'_> {
    fn to_string<T: fmt::Display>(&self, value: T) -> String {
        if self.alternate() {
            format!("{:#}", value)
        } else {
            format!("{}", value)
        }
    }
}

/// Formats values with [`fmt::Display`] using `{}` only (ignores alternate form).
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultDisplayFormat;

impl FormatterToString for DefaultDisplayFormat {
    fn to_string<T: fmt::Display>(&self, value: T) -> String {
        value.to_string()
    }
}

/// Formats values with [`fmt::Display`] using `{:#}` (alternate form always on).
#[derive(Debug, Clone, Copy, Default)]
pub struct AlternateDisplayFormat;

impl FormatterToString for AlternateDisplayFormat {
    fn to_string<T: fmt::Display>(&self, value: T) -> String {
        format!("{:#}", value)
    }
}

/// Generate multiline human-readable output
pub trait ToMultilineString {
    /// One item per line.
    fn to_multiline_string(&self) -> String;
    /// One item per line, each prefixed with its index.
    fn to_multiline_indexed_string(&self) -> String;
}

impl<T> ToMultilineString for Vec<T>
where
    T: fmt::Display,
{
    fn to_multiline_string(&self) -> String {
        AlternateDisplayFormat.to_multiline_string(self.iter())
    }

    fn to_multiline_indexed_string(&self) -> String {
        AlternateDisplayFormat.to_multiline_indexed_string(self.iter())
    }
}

/// Trait for printing values in a human-readable table format
pub trait ToTableString {
    /// Format with default layout (32 columns, 4-space indent, brackets, newlines).
    fn to_table_string(&self) -> String {
        self.to_table_string_custom(32, 4, true, true)
    }
    /// Format with explicit column count, indent, bracketing, and line-wrapping.
    fn to_table_string_custom(
        &self,
        columns: usize,
        indent: usize,
        brackets: bool,
        newlines: bool,
    ) -> String;
}

impl<T> ToTableString for Vec<T>
where
    T: fmt::Display,
{
    fn to_table_string_custom(
        &self,
        columns: usize,
        indent: usize,
        brackets: bool,
        newlines: bool,
    ) -> String {
        AlternateDisplayFormat.to_table_string_custom(self, columns, indent, brackets, newlines)
    }
}

/// Substitute a placeholder for an empty string.
pub trait StringIfEmpty<X: ToString> {
    /// Return `empty_string` if this is empty, otherwise the value itself.
    fn string_if_empty(&self, empty_string: X) -> String;
}

impl<S: AsRef<str>, X: ToString> StringIfEmpty<X> for S {
    fn string_if_empty(&self, empty_string: X) -> String {
        let s = self.as_ref();
        if s.is_empty() {
            empty_string.to_string()
        } else {
            s.to_string()
        }
    }
}

//////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Misc string tools

/// Split a `host:port` string into host and optional port at the last `:`.
///
/// Errors with a `String` if a `:` is present but the trailing portion does not parse as a `u16`.
pub fn split_port(name: &str) -> Result<(String, Option<u16>), String> {
    if let Some(split) = name.rfind(':') {
        let hoststr = &name[0..split];
        let portstr = &name[split + 1..];
        let port: u16 = portstr
            .parse::<u16>()
            .map_err(|e| format!("invalid port: {}", e))?;

        Ok((hoststr.to_string(), Some(port)))
    } else {
        Ok((name.to_string(), None))
    }
}

/// Prepend a `/` unless the string already starts with one.
#[must_use]
pub fn prepend_slash(s: String) -> String {
    if s.starts_with('/') {
        return s;
    }
    let mut out = "/".to_owned();
    out.push_str(s.as_str());
    out
}

/// Call [`ToString::to_string`], for use as a `.map()` argument.
pub fn map_to_string<X: ToString>(arg: X) -> String {
    arg.to_string()
}

#[cfg(test)]
mod formatter_to_string_tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct AltEcho(u32);

    impl fmt::Display for AltEcho {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if f.alternate() {
                write!(f, "[{}]", self.0)
            } else {
                write!(f, "{}", self.0)
            }
        }
    }

    struct Wrap<'a, T: fmt::Display>(&'a T);

    impl<T: fmt::Display> fmt::Display for Wrap<'_, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&f.to_string(self.0))
        }
    }

    #[test]
    fn nested_format_inherits_alternate() {
        assert_eq!(format!("{}", Wrap(&AltEcho(3))), "3");
        assert_eq!(format!("{:#}", Wrap(&AltEcho(3))), "[3]");
    }

    #[test]
    fn default_table_string_matches_single_line_when_short() {
        let v = [1u32, 2, 3];
        assert_eq!(AlternateDisplayFormat.to_table_string(&v), "[1,2,3]");
    }

    #[test]
    fn alternate_indexed_multiline_uses_hash_flag() {
        let v = [AltEcho(7)];
        let s = AlternateDisplayFormat.to_multiline_indexed_string(v.iter());
        assert!(s.contains("[7]"), "got {:?}", s);
    }
}
