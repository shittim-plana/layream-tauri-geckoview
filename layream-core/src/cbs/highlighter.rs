#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Control,
    Macro,
    Variable,
    Bracket,
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
    let bytes = input.as_bytes();
    let mut pos = 0;
    let mut depth = 0usize;

    // Track alternation per depth level: each new opening block at a given depth
    // increments the counter, and odd counters get alt=true.
    let mut depth_counter: [usize; 64] = [0; 64];
    // Stack of (depth, alt) for matching closing tags with their opening tag's shade.
    let mut open_stack: Vec<(usize, bool)> = Vec::new();

    while pos < bytes.len() {
        if pos + 1 < bytes.len() && bytes[pos] == b'{' && bytes[pos + 1] == b'{' {
            let bracket_start = pos;
            pos += 2;

            if let Some(end) = find_close(bytes, pos) {
                let content = &input[bracket_start + 2..end];
                let trimmed = content.trim();
                let kind = classify(trimmed);

                let (token_depth, token_alt) = if trimmed.starts_with('#') {
                    // Opening tag: tokens get current (outer) depth, then increment
                    let d = depth;
                    let capped = d.min(63);
                    let alt = depth_counter[capped] % 2 == 1;
                    open_stack.push((d, alt));
                    depth_counter[capped] += 1;
                    depth += 1;
                    (d, alt)
                } else if trimmed.starts_with('/')
                    && !trimmed.starts_with("//")
                {
                    // Closing tag: pop stack, use matched opening's depth and alt
                    depth = depth.saturating_sub(1);
                    if let Some((matched_depth, matched_alt)) = open_stack.pop() {
                        (matched_depth, matched_alt)
                    } else {
                        (depth, false)
                    }
                } else if trimmed.starts_with(':') {
                    // :else and similar: belongs to the enclosing block's depth and alt
                    if let Some(&(matched_depth, matched_alt)) = open_stack.last() {
                        (matched_depth, matched_alt)
                    } else {
                        (depth.saturating_sub(1), false)
                    }
                } else {
                    // Plain variables/macros: current depth, no alt
                    (depth, false)
                };

                tokens.push(HighlightToken {
                    start: bracket_start,
                    end: bracket_start + 2,
                    kind: TokenKind::Bracket,
                    depth: token_depth,
                    alt: token_alt,
                });

                tokens.push(HighlightToken {
                    start: bracket_start + 2,
                    end,
                    kind,
                    depth: token_depth,
                    alt: token_alt,
                });

                tokens.push(HighlightToken {
                    start: end,
                    end: end + 2,
                    kind: TokenKind::Bracket,
                    depth: token_depth,
                    alt: token_alt,
                });

                pos = end + 2;
            } else {
                pos = bracket_start + 1;
            }
        } else {
            pos += 1;
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

fn find_close(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1u32;
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            depth += 1;
            i += 2;
        } else if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    None
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

pub fn check_blocks(input: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();

    for (line_num, line) in input.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut pos = 0;
        while pos < bytes.len() {
            if pos + 3 < bytes.len() && bytes[pos] == b'{' && bytes[pos + 1] == b'{' {
                if let Some(end) = find_close_in(bytes, pos + 2) {
                    let tag = line[pos + 2..end].trim();
                    if let Some(name) = tag.strip_prefix('#') {
                        let block_name = name.split("::").next().unwrap_or(name)
                            .split_whitespace().next().unwrap_or(name);
                        stack.push((block_name.to_string(), line_num + 1));
                    } else if tag.starts_with('/') {
                        let close_name = tag[1..].split("::").next().unwrap_or(&tag[1..])
                            .split_whitespace().next().unwrap_or(&tag[1..]);
                        if close_name.is_empty() || close_name == "/" {
                            stack.pop();
                        } else if let Some(idx) = stack.iter().rposition(|(n, _)| n == close_name) {
                            stack.truncate(idx);
                        } else {
                            diagnostics.push(Diagnostic {
                                line: line_num + 1,
                                message: format!("unmatched closing tag: {}", close_name),
                            });
                        }
                    }
                    pos = end + 2;
                } else {
                    pos += 1;
                }
            } else if bytes[pos] == b'{' && (pos + 1 >= bytes.len() || bytes[pos + 1] != b'{') {
                // Single `{` — check if the content after looks like CBS
                let rest = &line[pos + 1..];
                let ahead = if rest.len() > 20 { &rest[..20] } else { rest };
                if looks_like_cbs(ahead) {
                    diagnostics.push(Diagnostic {
                        line: line_num + 1,
                        message: format!("single {{{{ — did you mean {{{{{{{{?"),
                    });
                }
                pos += 1;
            } else {
                pos += 1;
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

fn find_close_in(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
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
}
