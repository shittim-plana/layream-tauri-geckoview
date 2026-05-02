use regex::Regex;
use std::collections::HashMap;

use crate::types::CustomScript;

pub fn apply_regex(
    text: &str,
    scripts: &[CustomScript],
    flags: Option<&HashMap<String, bool>>,
) -> String {
    let mut result = text.to_string();
    for script in scripts {
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
        if let Ok(re) = Regex::new(&script.pattern) {
            result = re.replace_all(&result, script.out.as_str()).into_owned();
        }
    }
    result
}

pub struct RegexTestResult {
    pub output: String,
    pub error: Option<String>,
}

pub fn test_regex(pattern: &str, replacement: &str, input: &str) -> RegexTestResult {
    match Regex::new(pattern) {
        Ok(re) => RegexTestResult {
            output: re.replace_all(input, replacement).into_owned(),
            error: None,
        },
        Err(e) => RegexTestResult {
            output: input.to_string(),
            error: Some(e.to_string()),
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
        }
    }

    #[test]
    fn basic_replacement() {
        let scripts = vec![make_script("hello", "world")];
        assert_eq!(apply_regex("hello there", &scripts, None), "world there");
    }

    #[test]
    fn sequential_replacement() {
        let scripts = vec![
            make_script("a", "b"),
            make_script("b", "c"),
        ];
        assert_eq!(apply_regex("a", &scripts, None), "c");
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
        }];
        assert_eq!(apply_regex("x", &scripts, None), "x");
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
        }];
        let mut flags = HashMap::new();
        flags.insert("myFlag".to_string(), true);
        assert_eq!(apply_regex("x", &scripts, Some(&flags)), "y");
    }

    #[test]
    fn invalid_regex_skipped() {
        let scripts = vec![make_script("[invalid", "replacement")];
        assert_eq!(apply_regex("test", &scripts, None), "test");
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
}
