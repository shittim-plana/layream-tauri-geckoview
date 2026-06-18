use regex::RegexBuilder;
use std::collections::HashMap;

use crate::types::CustomScript;

/// Parse a JS-style `/pattern/flags` string into (pattern, flags).
/// Returns None if the input is not in that format (treat as raw regex).
fn parse_js_regex(input: &str) -> Option<(&str, &str)> {
    if !input.starts_with('/') {
        return None;
    }
    // Find the last '/' that isn't the first character
    let rest = &input[1..];
    if let Some(last_slash) = rest.rfind('/') {
        let pattern = &rest[..last_slash];
        let flags = &rest[last_slash + 1..];
        // Validate that flags only contain known characters
        if flags.chars().all(|c| matches!(c, 'i' | 'g' | 'm' | 's' | 'u')) {
            Some((pattern, flags))
        } else {
            None
        }
    } else {
        None
    }
}

/// Translate JS replacement syntax to Rust regex replacement syntax.
/// - `$&` -> `$0` (whole match)
/// - `$`` and `$'` -> not supported, pass through as literal `$`
/// - `$1`, `$2`, `${name}` -> same in Rust
fn translate_js_replacement(js_replacement: &str) -> String {
    let mut result = String::with_capacity(js_replacement.len());
    let bytes = js_replacement.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'$' && i + 1 < len {
            match bytes[i + 1] {
                b'&' => {
                    result.push_str("$0");
                    i += 2;
                }
                b'`' | b'\'' => {
                    // $` (before match) and $' (after match) not supported in Rust regex.
                    // Skip the special meaning, output literal `$`
                    result.push('$');
                    i += 1;
                }
                _ => {
                    result.push('$');
                    i += 1;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Build a compiled regex from a pattern string, handling JS `/pattern/flags` format.
fn build_regex(raw_pattern: &str) -> Result<regex::Regex, String> {
    let (pattern, flags_str) = match parse_js_regex(raw_pattern) {
        Some((p, f)) => (p, f),
        None => (raw_pattern, ""),
    };

    RegexBuilder::new(pattern)
        .case_insensitive(flags_str.contains('i'))
        .multi_line(flags_str.contains('m'))
        .dot_matches_new_line(flags_str.contains('s'))
        .build()
        .map_err(|e| e.to_string())
}

pub fn apply_regex(
    text: &str,
    scripts: &[CustomScript],
    flags: Option<&HashMap<String, bool>>,
    target_type: Option<&str>,
) -> String {
    let mut result = text.to_string();
    for script in scripts {
        // Filter by script_type if a target_type is specified
        if let Some(target) = target_type {
            if script.script_type != target {
                continue;
            }
        }

        if script.able_flag == Some(true) {
            if let Some(flag_name) = &script.flag {
                match flags {
                    Some(f) => {
                        if !f.get(flag_name).copied().unwrap_or(false) {
                            continue;
                        }
                    }
                    None => continue,
                }
            }
        }

        match build_regex(&script.pattern) {
            Ok(re) => {
                let replacement = translate_js_replacement(&script.out);
                result = re.replace_all(&result, replacement.as_str()).into_owned();
            }
            Err(e) => {
                log::warn!(
                    "Skipping script '{}': {}",
                    if script.comment.is_empty() {
                        &script.pattern
                    } else {
                        &script.comment
                    },
                    e
                );
            }
        }
    }
    result
}

pub struct RegexTestResult {
    pub output: String,
    pub error: Option<String>,
}

pub fn test_regex(pattern: &str, replacement: &str, input: &str) -> RegexTestResult {
    match build_regex(pattern) {
        Ok(re) => {
            let translated = translate_js_replacement(replacement);
            RegexTestResult {
                output: re.replace_all(input, translated.as_str()).into_owned(),
                error: None,
            }
        }
        Err(e) => RegexTestResult {
            output: input.to_string(),
            error: Some(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_script(pattern: &str, out: &str) -> CustomScript {
        CustomScript {
            comment: String::new(),
            pattern: pattern.to_string(),
            out: out.to_string(),
            script_type: String::new(),
            flag: None,
            able_flag: None,
            extra: Default::default(),
        }
    }

    fn make_typed_script(pattern: &str, out: &str, script_type: &str) -> CustomScript {
        CustomScript {
            comment: String::new(),
            pattern: pattern.to_string(),
            out: out.to_string(),
            script_type: script_type.to_string(),
            flag: None,
            able_flag: None,
            extra: Default::default(),
        }
    }

    // === Existing tests (updated for new signature) ===

    #[test]
    fn basic_replacement() {
        let scripts = vec![make_script("hello", "world")];
        assert_eq!(
            apply_regex("hello there", &scripts, None, None),
            "world there"
        );
    }

    #[test]
    fn sequential_replacement() {
        let scripts = vec![make_script("a", "b"), make_script("b", "c")];
        assert_eq!(apply_regex("a", &scripts, None, None), "c");
    }

    #[test]
    fn flag_skips_when_not_set() {
        let scripts = vec![CustomScript {
            comment: String::new(),
            pattern: "x".to_string(),
            out: "y".to_string(),
            script_type: String::new(),
            flag: Some("myFlag".to_string()),
            able_flag: Some(true),
            extra: Default::default(),
        }];
        assert_eq!(apply_regex("x", &scripts, None, None), "x");
    }

    #[test]
    fn flag_applies_when_set() {
        let scripts = vec![CustomScript {
            comment: String::new(),
            pattern: "x".to_string(),
            out: "y".to_string(),
            script_type: String::new(),
            flag: Some("myFlag".to_string()),
            able_flag: Some(true),
            extra: Default::default(),
        }];
        let mut flags = HashMap::new();
        flags.insert("myFlag".to_string(), true);
        assert_eq!(apply_regex("x", &scripts, Some(&flags), None), "y");
    }

    #[test]
    fn invalid_regex_skipped() {
        let scripts = vec![make_script("[invalid", "replacement")];
        assert_eq!(apply_regex("test", &scripts, None, None), "test");
    }

    #[test]
    fn test_regex_success() {
        let result = test_regex("(\\w+)", "[$1]", "hello world");
        assert_eq!(result.output, "[hello] [world]");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_regex_failure() {
        let result = test_regex("[bad", "x", "input");
        assert_eq!(result.output, "input");
        assert!(result.error.is_some());
    }

    // === New tests ===

    // /pattern/flags parsing

    #[test]
    fn parse_js_regex_basic() {
        assert_eq!(parse_js_regex("/hello/i"), Some(("hello", "i")));
        assert_eq!(parse_js_regex("/foo/gim"), Some(("foo", "gim")));
        assert_eq!(parse_js_regex("/bar/"), Some(("bar", "")));
    }

    #[test]
    fn parse_js_regex_not_js_format() {
        assert_eq!(parse_js_regex("hello"), None);
        assert_eq!(parse_js_regex(""), None);
    }

    #[test]
    fn parse_js_regex_with_slash_in_pattern() {
        assert_eq!(parse_js_regex("/a\\/b/i"), Some(("a\\/b", "i")));
    }

    #[test]
    fn js_regex_case_insensitive() {
        let scripts = vec![make_script("/hello/i", "world")];
        assert_eq!(
            apply_regex("HELLO there", &scripts, None, None),
            "world there"
        );
    }

    #[test]
    fn js_regex_multiline() {
        let scripts = vec![make_script("/^line/m", "START")];
        let input = "line one\nline two";
        let output = apply_regex(input, &scripts, None, None);
        assert_eq!(output, "START one\nSTART two");
    }

    #[test]
    fn js_regex_dot_matches_newline() {
        let scripts = vec![make_script("/a.b/s", "X")];
        assert_eq!(apply_regex("a\nb", &scripts, None, None), "X");
    }

    // JS replacement translation

    #[test]
    fn translate_dollar_ampersand() {
        assert_eq!(translate_js_replacement("[$&]"), "[$0]");
    }

    #[test]
    fn translate_dollar_number_unchanged() {
        assert_eq!(translate_js_replacement("$1-$2"), "$1-$2");
    }

    #[test]
    fn translate_no_specials() {
        assert_eq!(translate_js_replacement("plain text"), "plain text");
    }

    #[test]
    fn js_replacement_in_apply() {
        let scripts = vec![make_script("\\w+", "[$&]")];
        assert_eq!(
            apply_regex("hello world", &scripts, None, None),
            "[hello] [world]"
        );
    }

    // script_type filtering

    #[test]
    fn script_type_filters_correctly() {
        let scripts = vec![
            make_typed_script("hello", "HELLO", "editinput"),
            make_typed_script("world", "WORLD", "editoutput"),
        ];
        assert_eq!(
            apply_regex("hello world", &scripts, None, Some("editoutput")),
            "hello WORLD"
        );
    }

    #[test]
    fn script_type_none_applies_all() {
        let scripts = vec![
            make_typed_script("hello", "HELLO", "editinput"),
            make_typed_script("world", "WORLD", "editoutput"),
        ];
        assert_eq!(
            apply_regex("hello world", &scripts, None, None),
            "HELLO WORLD"
        );
    }

    #[test]
    fn script_type_no_match_skips_all() {
        let scripts = vec![
            make_typed_script("hello", "HELLO", "editinput"),
            make_typed_script("world", "WORLD", "editoutput"),
        ];
        assert_eq!(
            apply_regex("hello world", &scripts, None, Some("editdisplay")),
            "hello world"
        );
    }
}
