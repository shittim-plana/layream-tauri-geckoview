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
                tokens.push(HighlightToken {
                    start: bracket_start,
                    end: bracket_start + 2,
                    kind: TokenKind::Bracket,
                    depth,
                });

                let content = &input[bracket_start + 2..end];
                let trimmed = content.trim();
                let kind = classify(trimmed);

                if trimmed.starts_with('#') {
                    depth += 1;
                }

                tokens.push(HighlightToken {
                    start: bracket_start + 2,
                    end,
                    kind,
                    depth: if trimmed.starts_with('/') {
                        depth
                    } else {
                        depth.max(1) - if trimmed.starts_with('#') { 1 } else { 0 }
                    },
                });

                if trimmed.starts_with('/') && depth > 0 {
                    depth -= 1;
                }

                tokens.push(HighlightToken {
                    start: end,
                    end: end + 2,
                    kind: TokenKind::Bracket,
                    depth,
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
}
