//! Editor syntax highlighting and block diagnostics for CBS templates.
//!
//! Both the coloring ([`highlight`]) and the block-matching diagnostics
//! ([`check_blocks`]) consume the shared [`crate::cbs::tokenizer`]
//! ([`tokenizer::scan_spanned`]) for `{{ }}` boundaries — the single source of
//! truth also used by the parser (§4.1: no independent re-scan). The
//! highlighter keeps only what is genuinely its own: depth/alternation coloring
//! and markdown highlighting of the text between tags.

use crate::cbs::tokenizer::{self, SpanItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Control,
    Macro,
    Variable,
    Bracket,
    /// Literal text inside an `#escape` region — rendered plain, not as CBS.
    Escape,
    /// A markdown construct in the text outside `{{ }}` (heading, bold, etc.).
    Markdown(MarkdownKind),
}

/// The kind of markdown construct, mapped to a CSS class by the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownKind {
    Heading,
    Bold,
    Italic,
    Code,
    Link,
    ListItem,
    Quote,
}

impl TokenKind {
    /// Stable string tag for the frontend token-class map. Kept here so callers
    /// (e.g. `commands_cbs.rs`) need no exhaustive `match` that breaks when a
    /// variant is added.
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenKind::Control => "control",
            TokenKind::Macro => "macro",
            TokenKind::Variable => "variable",
            TokenKind::Bracket => "bracket",
            TokenKind::Escape => "escape",
            TokenKind::Markdown(MarkdownKind::Heading) => "md-heading",
            TokenKind::Markdown(MarkdownKind::Bold) => "md-bold",
            TokenKind::Markdown(MarkdownKind::Italic) => "md-italic",
            TokenKind::Markdown(MarkdownKind::Code) => "md-code",
            TokenKind::Markdown(MarkdownKind::Link) => "md-link",
            TokenKind::Markdown(MarkdownKind::ListItem) => "md-list",
            TokenKind::Markdown(MarkdownKind::Quote) => "md-quote",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HighlightToken {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
    pub depth: usize,
    /// true when this is an alternating (second shade) block at this depth
    pub alt: bool,
}

const DEPTH_COLORS: &[&str] = &[
    "#8be9fd", // cyan
    "#50fa7b", // green
    "#ffb86c", // orange
    "#ff79c6", // pink
    "#bd93f9", // purple
];

pub fn depth_color(depth: usize) -> &'static str {
    DEPTH_COLORS[depth % DEPTH_COLORS.len()]
}

pub fn highlight(input: &str) -> Vec<HighlightToken> {
    let mut tokens = Vec::new();
    let mut depth = 0usize;

    // Track alternation per depth level: each new opening block at a given depth
    // increments the counter, and odd counters get alt=true.
    let mut depth_counter: [usize; 64] = [0; 64];
    // Stack of (depth, alt) for matching closing tags with their opening tag's shade.
    let mut open_stack: Vec<(usize, bool)> = Vec::new();
    // When the previous tag opened an `#escape`, the next Text span is its
    // literal interior — colored plain (Escape), never markdown-highlighted.
    let mut next_text_is_escape = false;

    for item in tokenizer::scan_spanned(input) {
        match item {
            SpanItem::Text { start, end } => {
                if next_text_is_escape {
                    tokens.push(HighlightToken {
                        start,
                        end,
                        kind: TokenKind::Escape,
                        depth,
                        alt: false,
                    });
                    next_text_is_escape = false;
                } else {
                    highlight_markdown(&input[start..end], start, depth, &mut tokens);
                }
            }
            SpanItem::Tag { open, inner, close } => {
                let trimmed = input[inner.0..inner.1].trim();
                let kind = classify(trimmed);

                let (token_depth, token_alt) = if trimmed.starts_with('#') {
                    // Opening tag: tokens get current (outer) depth, then increment.
                    let d = depth;
                    let capped = d.min(63);
                    let alt = depth_counter[capped] % 2 == 1;
                    open_stack.push((d, alt));
                    depth_counter[capped] += 1;
                    depth += 1;
                    if trimmed.to_lowercase().starts_with("#escape") {
                        next_text_is_escape = true;
                    }
                    (d, alt)
                } else if trimmed.starts_with('/') && !trimmed.starts_with("//") {
                    // Closing tag: pop stack, use matched opening's depth and alt.
                    depth = depth.saturating_sub(1);
                    if let Some((matched_depth, matched_alt)) = open_stack.pop() {
                        (matched_depth, matched_alt)
                    } else {
                        (depth, false)
                    }
                } else if trimmed.starts_with(':') {
                    // :else and similar: belongs to the enclosing block's depth and alt.
                    if let Some(&(matched_depth, matched_alt)) = open_stack.last() {
                        (matched_depth, matched_alt)
                    } else {
                        (depth.saturating_sub(1), false)
                    }
                } else {
                    // Plain variables/macros: current depth, no alt.
                    (depth, false)
                };

                tokens.push(HighlightToken {
                    start: open.0,
                    end: open.1,
                    kind: TokenKind::Bracket,
                    depth: token_depth,
                    alt: token_alt,
                });
                tokens.push(HighlightToken {
                    start: inner.0,
                    end: inner.1,
                    kind,
                    depth: token_depth,
                    alt: token_alt,
                });
                tokens.push(HighlightToken {
                    start: close.0,
                    end: close.1,
                    kind: TokenKind::Bracket,
                    depth: token_depth,
                    alt: token_alt,
                });
            }
        }
    }

    tokens
}

fn classify(content: &str) -> TokenKind {
    let trimmed = content.trim();
    if trimmed.starts_with('#')
        || trimmed.starts_with('/')
        || trimmed.starts_with(':')
        || trimmed.starts_with("//")
        || trimmed.starts_with("? ")
    {
        TokenKind::Control
    } else if trimmed.contains("::") {
        TokenKind::Macro
    } else {
        TokenKind::Variable
    }
}

/// Emit markdown highlight tokens for a run of plain text outside `{{ }}`,
/// at absolute byte offset `base`. Only marked-up spans get tokens; plain
/// stretches produce none (the editor renders the gaps verbatim). Handles
/// line-start constructs (headings, lists, quotes) plus inline constructs
/// (code, bold, italic, links).
fn highlight_markdown(
    text: &str,
    base: usize,
    depth: usize,
    out: &mut Vec<HighlightToken>,
) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut at_line_start = true;

    while i < text.len() {
        // Line-start constructs.
        if at_line_start {
            if let Some((len, kind)) = match_line_start(&text[i..]) {
                push_md(out, base + i, base + i + len, kind, depth);
                i += len;
                at_line_start = text.as_bytes().get(i.wrapping_sub(1)) == Some(&b'\n');
                continue;
            }
        }
        // Inline constructs.
        if let Some((len, kind)) = match_inline(&text[i..]) {
            push_md(out, base + i, base + i + len, kind, depth);
            i += len;
            at_line_start = false;
            continue;
        }
        // Plain character: no token. Advance one UTF-8 char.
        let ch_len = utf8_len(bytes[i]);
        at_line_start = bytes[i] == b'\n';
        i += ch_len;
    }
}

fn push_md(out: &mut Vec<HighlightToken>, start: usize, end: usize, kind: MarkdownKind, depth: usize) {
    out.push(HighlightToken {
        start,
        end,
        kind: TokenKind::Markdown(kind),
        depth,
        alt: false,
    });
}

/// Match a line-start markdown construct at the beginning of `s`. Returns the
/// matched byte length (excluding the construct's content for spanning the whole
/// line where appropriate) and its kind.
fn match_line_start(s: &str) -> Option<(usize, MarkdownKind)> {
    // ATX heading: 1-6 '#' then a space, spanning to end of line.
    let hashes = s.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && s.as_bytes().get(hashes) == Some(&b' ') {
        let line_len = s.find('\n').unwrap_or(s.len());
        return Some((line_len, MarkdownKind::Heading));
    }
    // Unordered list marker: `- `, `* `, `+ `.
    if let Some(rest) = s.strip_prefix(['-', '*', '+']) {
        if rest.starts_with(' ') {
            return Some((2, MarkdownKind::ListItem));
        }
    }
    // Ordered list marker: digits then `. `.
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && s[digits..].starts_with(". ") {
        return Some((digits + 2, MarkdownKind::ListItem));
    }
    // Block quote: `> `.
    if s.starts_with("> ") {
        return Some((2, MarkdownKind::Quote));
    }
    None
}

/// Match an inline markdown construct at the beginning of `s`.
fn match_inline(s: &str) -> Option<(usize, MarkdownKind)> {
    // Inline code: `…`
    if let Some(len) = delimited(s, '`', '`') {
        return Some((len, MarkdownKind::Code));
    }
    // Bold: **…**
    if s.starts_with("**") {
        if let Some(end) = s[2..].find("**") {
            return Some((2 + end + 2, MarkdownKind::Bold));
        }
    }
    // Italic: *…* (single star, not part of **)
    if s.starts_with('*') && !s.starts_with("**") {
        if let Some(end_rel) = s[1..].find('*') {
            // Reject if the closing star is itself a `**` (belongs to bold).
            return Some((1 + end_rel + 1, MarkdownKind::Italic));
        }
    }
    // Link: [text](url)
    if s.starts_with('[') {
        if let Some(rb) = s.find(']') {
            if s[rb + 1..].starts_with('(') {
                if let Some(rp) = s[rb + 1..].find(')') {
                    return Some((rb + 1 + rp + 1, MarkdownKind::Link));
                }
            }
        }
    }
    None
}

/// Length of a same-delimiter inline span `open…close` (no newline inside),
/// including both delimiters, or None.
fn delimited(s: &str, open: char, close: char) -> Option<usize> {
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    if first != open {
        return None;
    }
    for (i, c) in chars {
        if c == '\n' {
            return None;
        }
        if c == close {
            return Some(i + c.len_utf8());
        }
    }
    None
}

fn utf8_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte < 0xE0 {
        2
    } else if first_byte < 0xF0 {
        3
    } else {
        4
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub line: usize,
    pub message: String,
}

/// CBS-like keywords that suggest a single `{` was meant to be `{{`.
fn looks_like_cbs(s: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "#if", "#when", "#each", "#pure", "#func", "#escape", "#code",
        "/if", "/when", "/each", "/pure", "/func", "/escape", "/code",
        ":else",
        "char", "bot", "user", "persona",
        "getvar", "setvar", "getglobalvar", "addvar", "settempvar", "tempvar",
        "setdefault", "slot",
        "calc", "random", "print", "comment", "//",
    ];
    let lower = s.to_ascii_lowercase();
    PREFIXES.iter().any(|p| lower.starts_with(p))
}

/// Diagnose block-matching problems and single-brace typos, consuming the shared
/// [`tokenizer::scan_spanned`] so escape interiors (literal `{{/0}}`…`{{/57}}`,
/// `{{/if_pure}}`) are NOT mistaken for real block closes — the escape-region fix.
///
/// Unlike the lenient tokenizer (which recovers from an unclosed block by
/// running its body to end of input), this checker intentionally *reports*
/// incompleteness (§1.2): it runs its own open/close stack over the shared
/// boundary tokens.
pub fn check_blocks(input: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // Stack of (block name, line of the opening tag).
    let mut stack: Vec<(String, usize)> = Vec::new();

    for item in tokenizer::scan_spanned(input) {
        match item {
            SpanItem::Tag { inner, .. } => {
                let tag = input[inner.0..inner.1].trim();
                let line = line_of(input, inner.0);
                if let Some(rest) = tag.strip_prefix('#') {
                    let name = first_token(rest);
                    stack.push((name.to_string(), line));
                } else if let Some(rest) = tag.strip_prefix('/') {
                    let close_name = first_token(rest);
                    if close_name.is_empty() {
                        stack.pop();
                    } else if let Some(idx) =
                        stack.iter().rposition(|(n, _)| n == close_name)
                    {
                        stack.truncate(idx);
                    } else {
                        diagnostics.push(Diagnostic {
                            line,
                            message: format!("unmatched closing tag: {}", close_name),
                        });
                    }
                }
            }
            SpanItem::Text { start, end } => {
                check_single_braces(&input[start..end], start, input, &mut diagnostics);
            }
        }
    }

    for (name, line) in stack {
        diagnostics.push(Diagnostic {
            line,
            message: format!("unclosed block: #{}", name),
        });
    }

    diagnostics
}

/// First whitespace/`::`-delimited token of a tag name (`#if::x` → `if`).
fn first_token(s: &str) -> &str {
    s.split("::")
        .next()
        .unwrap_or(s)
        .split_whitespace()
        .next()
        .unwrap_or("")
}

/// 1-based line number of byte offset `at` within `input`.
fn line_of(input: &str, at: usize) -> usize {
    input[..at.min(input.len())].bytes().filter(|b| *b == b'\n').count() + 1
}

/// Within a literal text run (at absolute byte offset `base`), flag any single
/// `{` immediately followed by CBS-looking content — a likely `{{` typo.
fn check_single_braces(text: &str, base: usize, input: &str, out: &mut Vec<Diagnostic>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // A real `{{` can appear in a Text run only as an escape-region
            // literal or an unclosed-delimiter fallback — neither is a typo.
            // Skip the whole `{{` so its second brace isn't mistaken for a lone one.
            if bytes.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            let ahead = &text[i + 1..];
            let ahead = if ahead.len() > 20 {
                &ahead[..floor_char_boundary(ahead, 20)]
            } else {
                ahead
            };
            if looks_like_cbs(ahead) {
                out.push(Diagnostic {
                    line: line_of(input, base + i),
                    message: "single {{ — did you mean {{{{?".to_string(),
                });
            }
        }
        i += 1;
    }
}

/// Largest char boundary at or below `idx` in `s`.
fn floor_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_highlight() {
        let tokens = highlight("Hello {{char}}!");
        assert_eq!(tokens.len(), 3); // open bracket, content, close bracket
        assert_eq!(tokens[1].kind, TokenKind::Variable);
    }

    #[test]
    fn macro_highlight() {
        let tokens = highlight("{{setvar::x::1}}");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Macro));
    }

    #[test]
    fn control_highlight() {
        let tokens = highlight("{{#if 1}}yes{{/if}}");
        let controls: Vec<_> = tokens.iter().filter(|t| t.kind == TokenKind::Control).collect();
        assert_eq!(controls.len(), 2);
    }

    #[test]
    fn depth_colors_cycle() {
        assert_eq!(depth_color(0), "#8be9fd");
        assert_eq!(depth_color(5), "#8be9fd");
    }

    #[test]
    fn block_diagnostics_ok() {
        let diags = check_blocks("{{#if 1}}hello{{/if}}");
        assert!(diags.is_empty());
    }

    #[test]
    fn block_diagnostics_unclosed() {
        let diags = check_blocks("{{#if 1}}hello");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unclosed"));
    }

    #[test]
    fn block_diagnostics_unmatched() {
        let diags = check_blocks("hello{{/if}}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unmatched"));
    }

    // --- Depth tracking tests ---

    #[test]
    fn depth_simple_if_block() {
        // {{#if 1}}yes{{/if}}
        // Tokens: {{ #if_1 }} {{ /if }}
        // All at depth 0: opening tag is outer (0), closing tag is outer (0)
        let tokens = highlight("{{#if 1}}yes{{/if}}");
        assert_eq!(tokens.len(), 6);
        let depths: Vec<usize> = tokens.iter().map(|t| t.depth).collect();
        assert_eq!(depths, vec![0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn depth_nested_when_blocks() {
        // {{#when a}}outer{{#when b}}inner{{/when}}outer2{{/when}}
        let tokens = highlight("{{#when a}}outer{{#when b}}inner{{/when}}outer2{{/when}}");
        assert_eq!(tokens.len(), 12);
        let depths: Vec<usize> = tokens.iter().map(|t| t.depth).collect();
        assert_eq!(
            depths,
            vec![
                0, 0, 0, // {{#when a}}
                1, 1, 1, // {{#when b}}
                1, 1, 1, // {{/when}} closes inner, back to depth 1
                0, 0, 0, // {{/when}} closes outer, back to depth 0
            ]
        );
    }

    #[test]
    fn depth_triple_nested() {
        // {{#when a}}{{#when b}}{{#when c}}deep{{/when}}{{/when}}{{/when}}
        let tokens = highlight("{{#when a}}{{#when b}}{{#when c}}deep{{/when}}{{/when}}{{/when}}");
        assert_eq!(tokens.len(), 18);
        let depths: Vec<usize> = tokens.iter().map(|t| t.depth).collect();
        assert_eq!(
            depths,
            vec![
                0, 0, 0, // {{#when a}}
                1, 1, 1, // {{#when b}}
                2, 2, 2, // {{#when c}}
                2, 2, 2, // {{/when}} closes c
                1, 1, 1, // {{/when}} closes b
                0, 0, 0, // {{/when}} closes a
            ]
        );
    }

    #[test]
    fn depth_else_handling() {
        // {{#if x}}a{{:else}}b{{/if}}
        // :else belongs to the #if block at depth 0
        let tokens = highlight("{{#if x}}a{{:else}}b{{/if}}");
        assert_eq!(tokens.len(), 9);
        let depths: Vec<usize> = tokens.iter().map(|t| t.depth).collect();
        assert_eq!(
            depths,
            vec![
                0, 0, 0, // {{#if x}}
                0, 0, 0, // {{:else}}
                0, 0, 0, // {{/if}}
            ]
        );
    }

    #[test]
    fn depth_mixed_variables_inside_block() {
        // {{#when a}}Hello {{char}}{{/when}}
        // {{char}} is inside the block at depth 1
        let tokens = highlight("{{#when a}}Hello {{char}}{{/when}}");
        assert_eq!(tokens.len(), 9);
        let depths: Vec<usize> = tokens.iter().map(|t| t.depth).collect();
        assert_eq!(
            depths,
            vec![
                0, 0, 0, // {{#when a}}
                1, 1, 1, // {{char}}
                0, 0, 0, // {{/when}}
            ]
        );
    }

    #[test]
    fn depth_else_in_nested_block() {
        // {{#when a}}{{#if x}}yes{{:else}}no{{/if}}{{/when}}
        let tokens = highlight("{{#when a}}{{#if x}}yes{{:else}}no{{/if}}{{/when}}");
        assert_eq!(tokens.len(), 15);
        let depths: Vec<usize> = tokens.iter().map(|t| t.depth).collect();
        assert_eq!(
            depths,
            vec![
                0, 0, 0, // {{#when a}}
                1, 1, 1, // {{#if x}}
                1, 1, 1, // {{:else}} belongs to #if at depth 1
                1, 1, 1, // {{/if}}
                0, 0, 0, // {{/when}}
            ]
        );
    }

    #[test]
    fn classify_else_as_control() {
        assert_eq!(classify(":else"), TokenKind::Control);
    }

    // --- Alternation tests ---

    #[test]
    fn alt_single_block_not_alt() {
        // First block at depth 0 should have alt=false
        let tokens = highlight("{{#if 1}}yes{{/if}}");
        let alts: Vec<bool> = tokens.iter().map(|t| t.alt).collect();
        assert_eq!(alts, vec![false, false, false, false, false, false]);
    }

    #[test]
    fn alt_two_consecutive_blocks_same_depth() {
        // Two consecutive blocks at depth 0: first alt=false, second alt=true
        let tokens = highlight("{{#if 1}}a{{/if}}{{#if 2}}b{{/if}}");
        assert_eq!(tokens.len(), 12);
        let alts: Vec<bool> = tokens.iter().map(|t| t.alt).collect();
        assert_eq!(
            alts,
            vec![
                false, false, false, // {{#if 1}} ... {{/if}} — first block
                false, false, false, // {{/if}} closing first block
                true, true, true,    // {{#if 2}} — second block at depth 0
                true, true, true,    // {{/if}} closing second block
            ]
        );
    }

    #[test]
    fn alt_three_consecutive_blocks_cycle() {
        // Three blocks: false, true, false (cycles back)
        let tokens = highlight("{{#if 1}}a{{/if}}{{#if 2}}b{{/if}}{{#if 3}}c{{/if}}");
        assert_eq!(tokens.len(), 18);
        let alts: Vec<bool> = tokens.iter().map(|t| t.alt).collect();
        // Block 1: alt=false, Block 2: alt=true, Block 3: alt=false
        assert!(!alts[0]); // first block
        assert!(alts[6]);  // second block
        assert!(!alts[12]); // third block
    }

    #[test]
    fn alt_else_matches_opening() {
        // :else should match the alt of its enclosing #if
        let tokens = highlight("{{#if x}}a{{:else}}b{{/if}}");
        assert_eq!(tokens.len(), 9);
        let alts: Vec<bool> = tokens.iter().map(|t| t.alt).collect();
        // All should be false (first block at depth 0)
        assert_eq!(alts, vec![false, false, false, false, false, false, false, false, false]);
    }

    #[test]
    fn alt_closing_matches_opening() {
        // The closing tag should have the same alt as its opening tag
        let tokens = highlight("{{#if 1}}a{{/if}}{{#if 2}}b{{/if}}");
        // First block tokens: indices 0-5 (alt=false)
        // Second block tokens: indices 6-11 (alt=true)
        assert!(!tokens[0].alt); // opening bracket of first
        assert!(!tokens[3].alt); // closing bracket of first's /if opening
        assert!(tokens[6].alt);  // opening bracket of second
        assert!(tokens[9].alt);  // closing bracket of second's /if opening
    }

    // --- Single brace typo detection tests ---

    #[test]
    fn single_brace_typo_detected() {
        let diags = check_blocks("{char}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("single"));
        assert!(diags[0].message.contains("did you mean"));
    }

    #[test]
    fn single_brace_non_cbs_no_warning() {
        // A single { with non-CBS content should not warn
        let diags = check_blocks("{hello world}");
        assert!(diags.is_empty());
    }

    #[test]
    fn double_brace_no_warning() {
        // Proper {{char}} should not trigger single-brace warning
        let diags = check_blocks("{{char}}");
        assert!(diags.is_empty());
    }

    #[test]
    fn single_brace_setvar_detected() {
        let diags = check_blocks("{setvar::x::1}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("single"));
    }

    #[test]
    fn single_brace_block_tag_detected() {
        let diags = check_blocks("{#if true}yes{/if}");
        assert_eq!(diags.len(), 2); // both { are detected
    }

    // --- Escape-region: interior literal, no spurious diagnostics (escape fix) ---

    #[test]
    fn escape_interior_not_diagnosed_as_blocks() {
        // {{/if_pure}} and {{/0}}..{{/57}} inside #escape must NOT raise
        // unmatched/unclosed diagnostics — this was the screenshot spam.
        let input = "{{#escape}}{{/if_pure}}{{/0}}{{/57}}{{/escape}}";
        let diags = check_blocks(input);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);
    }

    #[test]
    fn escape_interior_colored_plain() {
        // The escape body is a single Escape token; its interior {{ }} are NOT
        // colored as control/bracket tokens.
        let tokens = highlight("{{#escape}}{{/if_pure}}lit{{/escape}}");
        // {{#escape}} = 3 tokens, body = 1 Escape token, {{/escape}} = 3 tokens.
        assert_eq!(tokens.len(), 7);
        assert_eq!(tokens[3].kind, TokenKind::Escape);
        // No control tokens come from the literal interior {{/if_pure}}.
        let controls = tokens.iter().filter(|t| t.kind == TokenKind::Control).count();
        assert_eq!(controls, 2); // only #escape and /escape
    }

    #[test]
    fn if_pure_inline_no_false_positive() {
        // A balanced inline #if_pure block must NOT report "unclosed #if_pure".
        let diags = check_blocks("{{#if_pure 1}}{{char}}{{/if_pure}}");
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);
    }

    // --- Markdown highlighting of text outside {{ }} ---

    #[test]
    fn markdown_heading_and_bold() {
        let tokens = highlight("# Title\nsome **bold** text");
        let kinds: Vec<&TokenKind> = tokens.iter().map(|t| &t.kind).collect();
        assert!(kinds.contains(&&TokenKind::Markdown(MarkdownKind::Heading)));
        assert!(kinds.contains(&&TokenKind::Markdown(MarkdownKind::Bold)));
    }

    #[test]
    fn markdown_inline_code_and_link() {
        let tokens = highlight("see `code` and [text](http://x)");
        let kinds: Vec<&TokenKind> = tokens.iter().map(|t| &t.kind).collect();
        assert!(kinds.contains(&&TokenKind::Markdown(MarkdownKind::Code)));
        assert!(kinds.contains(&&TokenKind::Markdown(MarkdownKind::Link)));
    }

    #[test]
    fn markdown_not_applied_inside_tags() {
        // `**` inside a tag argument must not become a markdown token.
        let tokens = highlight("{{setvar::x::**not bold**}}");
        assert!(tokens.iter().all(|t| !matches!(t.kind, TokenKind::Markdown(_))));
    }

    #[test]
    fn markdown_token_spans_are_correct() {
        let input = "**bold**";
        let tokens = highlight(input);
        let md = tokens.iter().find(|t| matches!(t.kind, TokenKind::Markdown(_))).unwrap();
        assert_eq!(&input[md.start..md.end], "**bold**");
    }

    #[test]
    fn as_str_is_stable() {
        assert_eq!(TokenKind::Control.as_str(), "control");
        assert_eq!(TokenKind::Bracket.as_str(), "bracket");
        assert_eq!(TokenKind::Escape.as_str(), "escape");
        assert_eq!(TokenKind::Markdown(MarkdownKind::Heading).as_str(), "md-heading");
    }
}
