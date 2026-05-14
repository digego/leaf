use anyhow::{bail, Result};
use ratatui::{
    style::{Color, Modifier},
    text::Line,
};
use std::io::Write;

const DEFAULT_WIDTH: usize = 80;
const MIN_WIDTH: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineSpec {
    pub(crate) format: InlineFormat,
    pub(crate) width: Option<usize>,
    pub(crate) gutter: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineFormat {
    Auto,
    Ansi,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolvedFormat {
    Ansi,
    Plain,
}

/// Cheap predicate used by the CLI to decide whether a bare positional
/// argument should be consumed as the `--inline` spec or treated as a file
/// path. Recognises *intent* rather than full validity: anything that
/// starts with the `ansi`/`plain` keyword or a bare numeric width is
/// treated as an attempted spec. Detailed syntactic and semantic errors
/// (too many segments, gutter >= width, etc.) are surfaced by
/// `parse_inline_spec`, so a typo doesn't silently fall through to being
/// interpreted as a filename.
pub(crate) fn is_inline_spec(value: &str) -> bool {
    if value.starts_with('-') {
        return false;
    }
    let head = value.trim().split(':').next().unwrap_or("");
    if head.is_empty() {
        return false;
    }
    let lower = head.to_ascii_lowercase();
    lower == "ansi" || lower == "plain" || head.bytes().all(|b| b.is_ascii_digit())
}

/// Parse an `--inline` spec of the form `[fmt][:width[:gutter]]` where
/// `fmt` is `ansi` or `plain` (case-insensitive) and `width`/`gutter` are
/// non-negative integers. At least one segment must be present.
pub(crate) fn parse_inline_spec(value: &str) -> Result<InlineSpec> {
    let value = value.trim();
    let (format, width_str, gutter_str) = lex_inline_spec(value)?;
    let width = width_str.map(parse_width).transpose()?;
    let gutter = gutter_str.map(parse_gutter).transpose()?.unwrap_or(0);

    // A gutter that meets or exceeds the content width leaves zero columns
    // for actual text and would soft-wrap every character forever. Reject
    // it up-front rather than letting the renderer hang.
    if let Some(w) = width {
        if gutter >= w {
            bail!("gutter ({gutter}) must be smaller than width ({w})");
        }
    }

    Ok(InlineSpec {
        format,
        width,
        gutter,
    })
}

/// Lexical pass over an inline spec: returns the format keyword and the
/// (unparsed) width and gutter segments without performing any numeric or
/// semantic validation. This is the single source of truth for what the
/// CLI recognises as an inline spec versus a file path.
fn lex_inline_spec(value: &str) -> Result<(InlineFormat, Option<&str>, Option<&str>)> {
    if value.is_empty() {
        bail!("Empty inline spec");
    }
    let mut it = value.splitn(4, ':');
    let head = it.next().expect("splitn yields at least one element");
    if head.is_empty() {
        bail!("Invalid inline spec: {value} (expected [fmt][:width[:gutter]])");
    }

    let (format, width_seg) = match head.to_ascii_lowercase().as_str() {
        "ansi" => (InlineFormat::Ansi, it.next()),
        "plain" => (InlineFormat::Plain, it.next()),
        _ if head.bytes().all(|b| b.is_ascii_digit()) => (InlineFormat::Auto, Some(head)),
        _ => bail!("Unknown inline format: {head} (expected 'ansi' or 'plain')"),
    };

    if let Some(w) = width_seg {
        if w.is_empty() || !w.bytes().all(|b| b.is_ascii_digit()) {
            bail!("Invalid width: {w}");
        }
    }
    let gutter_seg = it.next();
    if let Some(g) = gutter_seg {
        if g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit()) {
            bail!("Invalid gutter: {g}");
        }
    }
    if it.next().is_some() {
        bail!("Invalid inline spec: {value} (expected [fmt][:width[:gutter]])");
    }
    Ok((format, width_seg, gutter_seg))
}

fn parse_width(s: &str) -> Result<usize> {
    let w: usize = s
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid width: {s}"))?;
    if w == 0 {
        bail!("Width must be a positive integer");
    }
    Ok(w.max(MIN_WIDTH))
}

fn parse_gutter(s: &str) -> Result<usize> {
    s.parse::<usize>()
        .map_err(|_| anyhow::anyhow!("Invalid gutter: {s}"))
}

pub(crate) fn render_width(spec: &InlineSpec, is_stdout_terminal: bool) -> usize {
    if let Some(w) = spec.width {
        return w.max(MIN_WIDTH);
    }
    if is_stdout_terminal {
        crossterm::terminal::size()
            .map(|(cols, _)| (cols as usize).max(MIN_WIDTH))
            .unwrap_or(DEFAULT_WIDTH)
    } else {
        DEFAULT_WIDTH
    }
}

pub(crate) fn resolve_format(spec: &InlineSpec, is_stdout_terminal: bool) -> ResolvedFormat {
    match spec.format {
        InlineFormat::Ansi => ResolvedFormat::Ansi,
        InlineFormat::Plain => ResolvedFormat::Plain,
        InlineFormat::Auto if is_stdout_terminal => ResolvedFormat::Ansi,
        InlineFormat::Auto => ResolvedFormat::Plain,
    }
}

pub(crate) fn write_lines<W: Write>(
    lines: &[Line<'_>],
    format: ResolvedFormat,
    max_width: usize,
    gutter: usize,
    writer: &mut W,
) -> Result<()> {
    // Defense in depth: parse_inline_spec already enforces gutter < width,
    // but InlineSpec can also be constructed directly (e.g. bare --inline).
    // Clamp here so a bad caller can never trigger an infinite soft-wrap.
    let gutter = gutter.min(max_width.saturating_sub(1));
    let gutter_bytes = vec![b' '; gutter];
    for line in lines {
        match format {
            ResolvedFormat::Ansi => write_line_ansi(line, max_width, &gutter_bytes, writer)?,
            ResolvedFormat::Plain => write_line_plain(line, max_width, &gutter_bytes, writer)?,
        }
    }
    Ok(())
}

fn write_line_ansi<W: Write>(
    line: &Line<'_>,
    max_width: usize,
    gutter_bytes: &[u8],
    writer: &mut W,
) -> Result<()> {
    let mut col = 0usize;
    if !gutter_bytes.is_empty() {
        write_bytes(writer, gutter_bytes)?;
    }
    for span in &line.spans {
        let style = &span.style;
        let mods = style.add_modifier;
        let fg = style.fg.filter(|c| !matches!(c, Color::Reset));
        let bg = style.bg.filter(|c| !matches!(c, Color::Reset));
        let has_style = fg.is_some() || bg.is_some() || !mods.is_empty();

        if has_style {
            write_ansi_style(writer, fg, bg, mods)?;
        }

        for ch in span.content.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if col + ch_width > max_width && col > 0 {
                if has_style {
                    write_bytes(writer, b"\x1b[0m")?;
                }
                write_bytes(writer, b"\n")?;
                col = 0;
                if !gutter_bytes.is_empty() {
                    write_bytes(writer, gutter_bytes)?;
                }
                if has_style {
                    write_ansi_style(writer, fg, bg, mods)?;
                }
            }
            let mut buf = [0u8; 4];
            write_bytes(writer, ch.encode_utf8(&mut buf).as_bytes())?;
            col += ch_width;
        }

        if has_style {
            write_bytes(writer, b"\x1b[0m")?;
        }
    }
    write_bytes(writer, b"\x1b[0m\n")?;
    Ok(())
}

fn write_line_plain<W: Write>(
    line: &Line<'_>,
    max_width: usize,
    gutter_bytes: &[u8],
    writer: &mut W,
) -> Result<()> {
    let mut col = 0usize;
    if !gutter_bytes.is_empty() {
        write_bytes(writer, gutter_bytes)?;
    }
    for span in &line.spans {
        for ch in span.content.chars() {
            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if col + ch_width > max_width && col > 0 {
                write_bytes(writer, b"\n")?;
                col = 0;
                if !gutter_bytes.is_empty() {
                    write_bytes(writer, gutter_bytes)?;
                }
            }
            let mut buf = [0u8; 4];
            write_bytes(writer, ch.encode_utf8(&mut buf).as_bytes())?;
            col += ch_width;
        }
    }
    write_bytes(writer, b"\n")?;
    Ok(())
}

fn write_ansi_style<W: Write>(
    writer: &mut W,
    fg: Option<Color>,
    bg: Option<Color>,
    mods: Modifier,
) -> Result<()> {
    write_bytes(writer, b"\x1b[")?;
    let mut need_sep = false;

    if let Some(c) = fg {
        if let Some(code) = color_ansi_code(c, false) {
            write_bytes(writer, code)?;
        } else {
            write_extended_color(writer, c, false)?;
        }
        need_sep = true;
    }
    if let Some(c) = bg {
        if need_sep {
            write_bytes(writer, b";")?;
        }
        if let Some(code) = color_ansi_code(c, true) {
            write_bytes(writer, code)?;
        } else {
            write_extended_color(writer, c, true)?;
        }
        need_sep = true;
    }

    for (flag, code) in [
        (Modifier::BOLD, &b"1"[..]),
        (Modifier::ITALIC, b"3"),
        (Modifier::UNDERLINED, b"4"),
        (Modifier::CROSSED_OUT, b"9"),
    ] {
        if mods.contains(flag) {
            if need_sep {
                write_bytes(writer, b";")?;
            }
            write_bytes(writer, code)?;
            need_sep = true;
        }
    }

    write_bytes(writer, b"m")?;
    Ok(())
}

fn write_bytes<W: Write>(writer: &mut W, bytes: &[u8]) -> Result<()> {
    match writer.write_all(bytes) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn color_ansi_code(color: Color, bg: bool) -> Option<&'static [u8]> {
    #[rustfmt::skip]
    static TABLE: [[&[u8]; 2]; 16] = [
        [b"30",  b"40"],   // Black
        [b"31",  b"41"],   // Red
        [b"32",  b"42"],   // Green
        [b"33",  b"43"],   // Yellow
        [b"34",  b"44"],   // Blue
        [b"35",  b"45"],   // Magenta
        [b"36",  b"46"],   // Cyan
        [b"37",  b"47"],   // Gray
        [b"90",  b"100"],  // DarkGray
        [b"91",  b"101"],  // LightRed
        [b"92",  b"102"],  // LightGreen
        [b"93",  b"103"],  // LightYellow
        [b"94",  b"104"],  // LightBlue
        [b"95",  b"105"],  // LightMagenta
        [b"96",  b"106"],  // LightCyan
        [b"97",  b"107"],  // White
    ];
    let idx = bg as usize;
    match color {
        Color::Reset => None,
        Color::Black => Some(TABLE[0][idx]),
        Color::Red => Some(TABLE[1][idx]),
        Color::Green => Some(TABLE[2][idx]),
        Color::Yellow => Some(TABLE[3][idx]),
        Color::Blue => Some(TABLE[4][idx]),
        Color::Magenta => Some(TABLE[5][idx]),
        Color::Cyan => Some(TABLE[6][idx]),
        Color::Gray => Some(TABLE[7][idx]),
        Color::DarkGray => Some(TABLE[8][idx]),
        Color::LightRed => Some(TABLE[9][idx]),
        Color::LightGreen => Some(TABLE[10][idx]),
        Color::LightYellow => Some(TABLE[11][idx]),
        Color::LightBlue => Some(TABLE[12][idx]),
        Color::LightMagenta => Some(TABLE[13][idx]),
        Color::LightCyan => Some(TABLE[14][idx]),
        Color::White => Some(TABLE[15][idx]),
        Color::Indexed(_) | Color::Rgb(_, _, _) => None,
    }
}

fn write_extended_color<W: Write>(writer: &mut W, color: Color, bg: bool) -> Result<()> {
    use std::io::Cursor;
    let base: u8 = if bg { 48 } else { 38 };
    let mut buf = [0u8; 20];
    let mut cur = Cursor::new(&mut buf[..]);
    match color {
        Color::Indexed(n) => {
            let _ = write!(cur, "{base};5;{n}");
        }
        Color::Rgb(r, g, b) => {
            let _ = write!(cur, "{base};2;{r};{g};{b}");
        }
        _ => return Ok(()),
    }
    let len = cur.position() as usize;
    write_bytes(writer, &buf[..len])
}





