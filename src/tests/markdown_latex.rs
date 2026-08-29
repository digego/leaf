use super::{rendered_non_empty_lines, test_assets, test_md_theme};
use crate::markdown::parse_markdown;

/// Block-level LaTeX written with `\[ ... \]` should be parsed the same
/// way as the dollar-delimited form `$$ ... $$`.
#[test]
fn bracket_form_renders_identically_to_dollar_form() {
    let (ss, theme) = test_assets();
    let bracket = "Lead-in line.\n\n\\[\nx = y + z\n\\]\n\nTrailing line.\n";
    let dollar = "Lead-in line.\n\n$$\nx = y + z\n$$\n\nTrailing line.\n";

    let (lhs, _, _, _) = parse_markdown(bracket, &ss, &theme, &test_md_theme(), false, true).into();
    let (rhs, _, _, _) = parse_markdown(dollar, &ss, &theme, &test_md_theme(), false, true).into();

    let lhs_rendered = rendered_non_empty_lines(&lhs);
    let rhs_rendered = rendered_non_empty_lines(&rhs);

    assert_eq!(lhs_rendered, rhs_rendered);
    // Sanity: the latex block was actually recognised (there should be a
    // "latex" header line emitted by push_latex_block_lines).
    assert!(
        lhs_rendered.iter().any(|l| l.contains("latex")),
        "expected a LaTeX block header in rendered output, got: {lhs_rendered:#?}",
    );
}

/// Indented `\[` / `\]` delimiter lines (e.g. inside a list item or just
/// with leading whitespace) should still trigger the rewrite.
#[test]
fn indented_bracket_delimiters_are_recognised() {
    let (ss, theme) = test_assets();
    let src = "Intro.\n\n    \\[\n    a = b\n    \\]\n\nOutro.\n";
    let dollar = "Intro.\n\n$$\na = b\n$$\n\nOutro.\n";

    let (lhs, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let (rhs, _, _, _) = parse_markdown(dollar, &ss, &theme, &test_md_theme(), false, true).into();

    // Both should produce a recognised latex block; only check for the
    // presence of the latex header rather than exact equality since the
    // dollar form has different surrounding whitespace/indent semantics.
    let lhs_rendered = rendered_non_empty_lines(&lhs);
    let rhs_rendered = rendered_non_empty_lines(&rhs);
    assert!(lhs_rendered.iter().any(|l| l.contains("latex")));
    assert!(rhs_rendered.iter().any(|l| l.contains("latex")));
}

/// `\[` / `\]` shown as *literal text* inside a fenced code block must
/// not be rewritten -- otherwise the rendered code sample would show
/// `$$` where the author wrote `\[`.
#[test]
fn fenced_code_block_protects_bracket_delimiters() {
    let (ss, theme) = test_assets();
    let src = "Example:\n\n```text\n\\[\nthis is shown as-is\n\\]\n```\n";

    let (lines, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let rendered = rendered_non_empty_lines(&lines);

    let joined = rendered.join("\n");
    assert!(
        joined.contains("\\["),
        "expected literal \\[ to survive inside fenced code, got:\n{joined}",
    );
    assert!(
        joined.contains("\\]"),
        "expected literal \\] to survive inside fenced code, got:\n{joined}",
    );
    assert!(
        !joined.contains("latex"),
        "fenced code block should not be rewritten to a latex block; got:\n{joined}",
    );
}

/// Inline `\[` and `\]` mid-paragraph are Markdown escape sequences for
/// literal brackets and must not be hijacked as LaTeX delimiters.
#[test]
fn inline_escaped_brackets_in_prose_are_untouched() {
    let (ss, theme) = test_assets();
    // pulldown-cmark resolves `\[` to a literal `[` in the Text event.
    let src = "See appendix \\[A\\] for details.\n";

    let (lines, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let rendered = rendered_non_empty_lines(&lines);

    let joined = rendered.join("\n");
    assert!(
        joined.contains("[A]"),
        "expected escaped brackets to render as literal '[A]', got:\n{joined}",
    );
    assert!(
        !joined.contains("latex"),
        "inline escaped brackets must not produce a latex block; got:\n{joined}",
    );
}

/// Sources that contain no `\[` / `\]` / `\(` / `\)` at all must
/// produce the same output as before (regression guard for the
/// normalizer's fast path).
#[test]
fn sources_without_latex_delimiters_are_unchanged() {
    let (ss, theme) = test_assets();
    let src = "# Title\n\nA paragraph with no math at all.\n";

    let (lines, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let rendered = rendered_non_empty_lines(&lines);

    assert!(rendered.iter().any(|l| l.contains("Title")));
    assert!(rendered
        .iter()
        .any(|l| l.contains("A paragraph with no math at all.")));
}

// ---------------------------------------------------------------------
// Inline \(...\) coverage
// ---------------------------------------------------------------------

/// Inline `\(...\)` should render identically to its dollar-delimited
/// equivalent `$...$`.
#[test]
fn inline_paren_form_matches_dollar_form() {
    let (ss, theme) = test_assets();
    let paren = "Consider \\(x = y + z\\) in this case.\n";
    let dollar = "Consider $x = y + z$ in this case.\n";

    let (lhs, _, _, _) = parse_markdown(paren, &ss, &theme, &test_md_theme(), false, true).into();
    let (rhs, _, _, _) = parse_markdown(dollar, &ss, &theme, &test_md_theme(), false, true).into();

    assert_eq!(rendered_non_empty_lines(&lhs), rendered_non_empty_lines(&rhs));
}

/// Multiple inline math spans on the same line must each be rewritten.
#[test]
fn multiple_inline_spans_on_one_line() {
    let (ss, theme) = test_assets();
    let paren = "We have \\(a\\) and \\(b\\) together.\n";
    let dollar = "We have $a$ and $b$ together.\n";

    let (lhs, _, _, _) = parse_markdown(paren, &ss, &theme, &test_md_theme(), false, true).into();
    let (rhs, _, _, _) = parse_markdown(dollar, &ss, &theme, &test_md_theme(), false, true).into();

    assert_eq!(rendered_non_empty_lines(&lhs), rendered_non_empty_lines(&rhs));
}

/// `\(...\)` inside an inline-code span must be preserved verbatim --
/// it's a code example, not math.
#[test]
fn inline_paren_inside_code_span_is_untouched() {
    let (ss, theme) = test_assets();
    let src = "The literal text `\\(x\\)` is shown as-is.\n";

    let (lines, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let joined = rendered_non_empty_lines(&lines).join("\n");

    assert!(
        joined.contains("\\(x\\)"),
        "expected literal \\(x\\) inside inline code, got:\n{joined}",
    );
}

/// `\(...\)` inside a fenced code block must be preserved verbatim.
#[test]
fn inline_paren_inside_fenced_code_is_untouched() {
    let (ss, theme) = test_assets();
    let src = "```text\nuse \\(x\\) for inline math\n```\n";

    let (lines, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let joined = rendered_non_empty_lines(&lines).join("\n");

    assert!(
        joined.contains("\\(x\\)"),
        "expected literal \\(x\\) inside fenced code, got:\n{joined}",
    );
}

/// A `\(` with no matching `\)` on the same line must be left untouched
/// rather than half-rewritten.
#[test]
fn unmatched_inline_opener_is_left_intact() {
    let (ss, theme) = test_assets();
    // `\(` resolves through standard Markdown escapes to a literal `(`.
    let src = "An opener with no closer: \\(x is fine.\n";

    let (lines, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let joined = rendered_non_empty_lines(&lines).join("\n");

    // pulldown-cmark's text escape turns `\(` into `(` in the rendered
    // text; the important thing is that no `$` snuck in.
    assert!(
        !joined.contains('$'),
        "unmatched opener must not introduce a stray $, got:\n{joined}",
    );
    assert!(joined.contains("(x is fine"));
}

/// An escaped backslash `\\(` (literal `\` followed by `(`) must NOT be
/// interpreted as a math opener.
#[test]
fn double_backslash_paren_is_not_a_math_opener() {
    let (ss, theme) = test_assets();
    // Source: `\\(x\\)`  -- two backslashes then `(`, content, then two backslashes then `)`.
    // Standard Markdown reads this as a literal `\` + `(x` + literal `\` + `)`.
    let src = "Literal backslash-paren: \\\\(x\\\\) here.\n";

    let (lines, _, _, _) = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true).into();
    let joined = rendered_non_empty_lines(&lines).join("\n");

    assert!(
        !joined.contains('$'),
        "escaped backslash must not yield a math span, got:\n{joined}",
    );
}

/// Triple backslash `\\\(` is an escaped backslash followed by a real
/// math opener `\(`. The math span SHOULD be rewritten.
#[test]
fn triple_backslash_paren_is_a_math_opener() {
    let (ss, theme) = test_assets();
    let paren = "Edge: \\\\\\(x\\\\\\) end.\n";
    let dollar = "Edge: \\\\$x\\\\$ end.\n";

    let (lhs, _, _, _) = parse_markdown(paren, &ss, &theme, &test_md_theme(), false, true).into();
    let (rhs, _, _, _) = parse_markdown(dollar, &ss, &theme, &test_md_theme(), false, true).into();

    assert_eq!(rendered_non_empty_lines(&lhs), rendered_non_empty_lines(&rhs));
}

