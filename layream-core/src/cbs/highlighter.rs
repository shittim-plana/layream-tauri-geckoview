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

    while pos < bytes.len() {
        if pos + 1 < bytes.len() && bytes[pos] == b'{' && bytes[pos + 1] == b'{' {
            let bracket_start = pos;
            pos += 2;

            if let Some(end) = find_close(bytes, pos) {
                let content = &input[bracket_start + 2..end];
                let trimmed = content.trim();
                let kind = classify(trimmed);

                let token_depth = if trimmed.starts_with('#') {
                    // Opening tag: tokens get current (outer) depth, then increment
                    let d = depth;
                    depth += 1;
                    d
                } else if trimmed.starts_with('/') {
                    // Closing tag: decrement first, then tokens get new (outer) depth
                    depth = depth.saturating_sub(1);
                    depth
                } else if trimmed.starts_with(':') {
                    // :else and similar: belongs to the enclosing block's depth
                    depth.saturating_sub(1)
                } else {
                    // Plain variables/macros: current depth
                    depth
                };

                tokens.push(HighlightToken {
                    start: bracket_start,
                    end: bracket_start + 2,
                    kind: TokenKind::Bracket,
                    depth: token_depth,
                });

                tokens.push(HighlightToken {
                    start: bracket_start + 2,
                    end,
                    kind,
                    depth: token_depth,
                });

                tokens.push(HighlightToken {
                    start: end,
                    end: end + 2,
                    kind: TokenKind::Bracket,
                    depth: token_depth,
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

pub fn check_blocks(input: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();

    for (line_num, line) in input.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut pos = 0;
        while pos + 3 < bytes.len() {
            if bytes[pos] == b'{' && bytes[pos + 1] == b'{' {
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
}
