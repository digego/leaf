//! Normalise LaTeX-style display- and inline-math delimiters into the
//! Markdown math-extension form that pulldown-cmark understands.
//!
//! Many sources (Pandoc, MathJax, scientific notes) use:
//!
//! * `\[ ... \]` for block / display math, and
//! * `\( ... \)` for inline math.
//!
//! pulldown-cmark only recognises the dollar-delimited forms `$$ ... $$`
//! and `$ ... $`, so this preprocessor rewrites the LaTeX-style
//! delimiters into the dollar forms before parsing.
//!
//! Conservatism is important:
//!
//! * In standard Markdown `\[` and `\(` are escape sequences for literal
//!   `[` and `(`. We must not silently change that semantics for
//!   non-math content, so:
//!   - Block form: only *standalone* delimiter lines (the canonical
//!     Pandoc/MathJax form) are rewritten. Inline `\[A\]` in running
//!     prose is left untouched.
//!   - Inline form: `\(...\)` is rewritten only when a matching `\)`
//!     appears on the same line, not inside an inline-code span, and
//!     not preceded by an even number of backslashes (which would mean
//!     the `\` is itself escaped and the paren is just a literal).
//! * Content inside fenced code blocks (``` or `~~~`) is preserved
//!   verbatim so example snippets that *show* `\[/\]` or `\(/\)` as
//!   text still render as written.

use std::borrow::Cow;

/// Rewrite LaTeX-style math delimiters to their dollar-delimited
/// pulldown-cmark equivalents:
///
/// * Standalone `\[` / `\]` lines become `$$`.
/// * Paired `\(...\)` spans on a single line become `$...$`.
///
/// Returns a borrowed view of the original source if no change was needed,
/// otherwise an owned String with the substitutions applied.
pub(super) fn normalize_latex_delimiters(src: &str) -> Cow<'_, str> {
    if !needs_normalization(src) {
        return Cow::Borrowed(src);
    }

    let mut out = String::with_capacity(src.len());
    let mut fence: Option<(char, usize)> = None;
    let mut changed = false;

    for line in src.split_inclusive('\n') {
        let (body, has_eol) = split_eol(line);

        // Inside a fenced block, only watch for the matching close fence
        // and copy everything else through verbatim.
        if let Some((ch, len)) = fence {
            if is_fence_close(body, ch, len) {
                fence = None;
            }
            out.push_str(line);
            continue;
        }

        // Outside a fenced block, look for an opening fence first so we
        // don't accidentally rewrite delimiters inside a code sample.
        if let Some(open) = detect_fence_open(body) {
            fence = Some(open);
            out.push_str(line);
            continue;
        }

        // Block-level delimiter: a line whose only non-whitespace content
        // is `\[` or `\]`. Replace the whole line with `$$`.
        let trimmed = body.trim();
        if trimmed == "\\[" || trimmed == "\\]" {
            out.push_str("$$");
            if has_eol {
                out.push('\n');
            }
            changed = true;
            continue;
        }

        // Inline-level delimiters: rewrite `\(...\)` to `$...$` for
        // matched pairs on this single line, skipping inline-code spans.
        match rewrite_inline_math(body) {
            Some(rewritten) => {
                out.push_str(&rewritten);
                if has_eol {
                    out.push('\n');
                }
                changed = true;
            }
            None => out.push_str(line),
        }
    }

    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(src)
    }
}

fn needs_normalization(src: &str) -> bool {
    // Cheap reject: if none of the LaTeX delimiter prefixes appear at
    // all, we can skip the whole line-by-line scan.
    src.contains("\\[") || src.contains("\\]") || src.contains("\\(") || src.contains("\\)")
}

/// Split `line` (which may include a trailing `\n` or `\r\n`) into its
/// body (without the newline) and a flag indicating whether a newline was
/// present.
fn split_eol(line: &str) -> (&str, bool) {
    if let Some(stripped) = line.strip_suffix('\n') {
        let stripped = stripped.strip_suffix('\r').unwrap_or(stripped);
        (stripped, true)
    } else {
        (line, false)
    }
}

/// Detect a fenced-code opener: 3+ backticks or 3+ tildes after optional
/// leading whitespace. Returns the fence character and its run length.
fn detect_fence_open(body: &str) -> Option<(char, usize)> {
    let s = body.trim_start();
    let ch = s.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let count = s.chars().take_while(|c| *c == ch).count();
    if count >= 3 {
        Some((ch, count))
    } else {
        None
    }
}

/// A closing fence is the same character as the opener, repeated at least
/// `min_len` times, with no other non-whitespace content on the line.
fn is_fence_close(body: &str, ch: char, min_len: usize) -> bool {
    let trimmed = body.trim();
    if trimmed.chars().any(|c| c != ch) {
        return false;
    }
    trimmed.chars().count() >= min_len
}

/// Rewrite paired `\(...\)` inline-math spans on a single line into
/// `$...$`. Returns `Some(new_line)` when at least one pair was
/// rewritten, or `None` to signal "no change, use the original".
///
/// Rules:
/// * `\(` / `\)` inside an inline-code span (delimited by matching
///   backtick runs) are ignored.
/// * A `\(` whose backslash is itself escaped -- i.e. preceded by an
///   even number of backslashes -- is *not* a math opener (the backslash
///   pairs with the previous backslash, leaving the paren bare).
/// * An opener with no matching closer on the same line is left as-is.
fn rewrite_inline_math(body: &str) -> Option<String> {
    // Operates on bytes because all delimiter characters (`\`, `(`, `)`,
    // `` ` ``) are ASCII; UTF-8 multibyte chars never share these bytes.
    let bytes = body.as_bytes();
    let mut positions: Vec<(usize, bool)> = Vec::new(); // (byte index of `\`, is_open)
    let mut i = 0;
    let mut code_fence: Option<usize> = None;

    while i < bytes.len() {
        let b = bytes[i];

        // Inline-code fence: consume the whole backtick run.
        if b == b'`' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            let run = i - start;
            match code_fence {
                None => code_fence = Some(run),
                Some(open) if open == run => code_fence = None,
                // Mismatched run inside code stays inside code.
                _ => {}
            }
            continue;
        }

        if code_fence.is_some() {
            i += 1;
            continue;
        }

        // Backslash run: count consecutive backslashes; only an odd-length
        // run leaves the final `\` unescaped and able to pair with the
        // following character.
        if b == b'\\' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'\\' {
                i += 1;
            }
            let run = i - start;
            if i < bytes.len() && run % 2 == 1 {
                let next = bytes[i];
                if next == b'(' || next == b')' {
                    positions.push((i - 1, next == b'('));
                    i += 1; // consume the paren
                    continue;
                }
            }
            continue;
        }

        i += 1;
    }

    // Pair each opener with the next closer.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut iter = positions.into_iter().peekable();
    while let Some((idx, is_open)) = iter.next() {
        if !is_open {
            continue; // dangling closer
        }
        while let Some(&(c_idx, c_open)) = iter.peek() {
            if c_open {
                // Another opener before the next closer: the first opener
                // has no matching closer with non-math content. Drop it
                // and start trying to pair the new opener instead.
                break;
            }
            pairs.push((idx, c_idx));
            iter.next();
            break;
        }
    }

    if pairs.is_empty() {
        return None;
    }

    // Build the output, replacing each `\(` and `\)` (2 bytes each) with
    // a single `$`.
    let mut out = String::with_capacity(body.len());
    let mut cursor = 0;
    for (open, close) in pairs {
        out.push_str(&body[cursor..open]);
        out.push('$');
        out.push_str(&body[open + 2..close]);
        out.push('$');
        cursor = close + 2;
    }
    out.push_str(&body[cursor..]);
    Some(out)
}

