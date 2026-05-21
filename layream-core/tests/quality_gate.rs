//! Code quality gate tests -- verify production invariants automatically.
//! These tests scan source files and fail if violations are introduced.

use std::fs;
use std::path::{Path, PathBuf};

/// Recursively collect all `.rs` files under `dir`.
fn source_files(dir: &str) -> Vec<PathBuf> {
    let mut result = Vec::new();
    collect_rs_files(Path::new(dir), &mut result);
    result
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Return `(line_number, line)` pairs that are OUTSIDE `#[cfg(test)]` blocks.
///
/// Tracks brace depth: when `#[cfg(test)]` is seen, it waits for the next `{`
/// to start counting depth, and everything until the matching `}` is excluded.
fn production_lines(source: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut in_test_attr = false; // saw #[cfg(test)], waiting for opening brace
    let mut test_depth: usize = 0; // brace depth inside a test module (0 = not in test)
    let mut waiting_for_brace = false; // between #[cfg(test)] and the opening {

    for (idx, line) in source.lines().enumerate() {
        let lineno = idx + 1;
        let trimmed = line.trim();

        // Detect #[cfg(test)] attribute (possibly with whitespace variations)
        if !in_test_attr && test_depth == 0 && trimmed.contains("#[cfg(test)]") {
            in_test_attr = true;
            waiting_for_brace = true;
            // This line itself is test infrastructure, skip it
            // But first check if the opening brace is on this same line
            if let Some(brace_pos) = trimmed.find('{') {
                // There might be content before #[cfg(test)] on the same line (unlikely but handle)
                waiting_for_brace = false;
                in_test_attr = false;
                test_depth = 1;
                // Count any additional braces on this line after the opening one
                for ch in trimmed[brace_pos + 1..].chars() {
                    match ch {
                        '{' => test_depth += 1,
                        '}' => {
                            test_depth = test_depth.saturating_sub(1);
                            if test_depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            continue;
        }

        // If we saw #[cfg(test)] and are waiting for the opening brace
        if waiting_for_brace {
            if let Some(brace_pos) = trimmed.find('{') {
                waiting_for_brace = false;
                in_test_attr = false;
                test_depth = 1;
                for ch in trimmed[brace_pos + 1..].chars() {
                    match ch {
                        '{' => test_depth += 1,
                        '}' => {
                            test_depth = test_depth.saturating_sub(1);
                            if test_depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Either way, this line is test infrastructure
            continue;
        }

        // Inside a test module -- track braces but don't emit lines
        if test_depth > 0 {
            for ch in trimmed.chars() {
                match ch {
                    '{' => test_depth += 1,
                    '}' => {
                        test_depth = test_depth.saturating_sub(1);
                        if test_depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        // Production line
        result.push((lineno, line));
    }

    result
}

/// Return `(line_number, line)` for ALL lines (including test code).
fn all_lines(source: &str) -> Vec<(usize, &str)> {
    source
        .lines()
        .enumerate()
        .map(|(idx, line)| (idx + 1, line))
        .collect()
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn core_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn app_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../layream-app/src-tauri/src")
}

// ---------------------------------------------------------------------------
// Violation collector
// ---------------------------------------------------------------------------

struct Violations {
    items: Vec<String>,
}

impl Violations {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn add(&mut self, file: &Path, lineno: usize, content: &str) {
        self.items.push(format!(
            "VIOLATION: {}:{}: {}",
            file.display(),
            lineno,
            content.trim()
        ));
    }

    fn assert_empty(self, rule: &str) {
        if !self.items.is_empty() {
            let report = self.items.join("\n");
            panic!(
                "\n{rule}: {} violation(s) found:\n{report}\n",
                self.items.len()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn no_unwrap_in_production_core() {
    let mut violations = Violations::new();
    for path in source_files(core_src().to_str().expect("non-UTF-8 path")) {
        let source = fs::read_to_string(&path).expect("failed to read source file");
        for (lineno, line) in production_lines(&source) {
            if line.contains(".unwrap()")
                && !line.contains(".unwrap_or(")
                && !line.contains(".unwrap_or_default(")
                && !line.contains(".unwrap_or_else(")
            {
                violations.add(&path, lineno, line);
            }
        }
    }
    violations.assert_empty("no_unwrap_in_production_core");
}

#[test]
fn no_unwrap_in_production_app() {
    let app = app_src();
    if !app.exists() {
        // App crate may not be checked out in all environments
        return;
    }
    let mut violations = Violations::new();
    for path in source_files(app.to_str().expect("non-UTF-8 path")) {
        let source = fs::read_to_string(&path).expect("failed to read source file");
        for (lineno, line) in production_lines(&source) {
            if line.contains(".unwrap()")
                && !line.contains(".unwrap_or(")
                && !line.contains(".unwrap_or_default(")
                && !line.contains(".unwrap_or_else(")
            {
                violations.add(&path, lineno, line);
            }
        }
    }
    violations.assert_empty("no_unwrap_in_production_app");
}

#[test]
fn no_eprintln_in_production() {
    let mut violations = Violations::new();

    let dirs = [core_src(), app_src()];
    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        for path in source_files(dir.to_str().expect("non-UTF-8 path")) {
            let source = fs::read_to_string(&path).expect("failed to read source file");
            for (lineno, line) in production_lines(&source) {
                if line.contains("eprintln!") {
                    violations.add(&path, lineno, line);
                }
            }
        }
    }

    violations.assert_empty("no_eprintln_in_production");
}

#[test]
fn no_allow_attributes() {
    let mut violations = Violations::new();

    let dirs = [core_src(), app_src()];
    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        for path in source_files(dir.to_str().expect("non-UTF-8 path")) {
            let source = fs::read_to_string(&path).expect("failed to read source file");
            // Scan ALL lines -- #[allow()] is never OK, even in tests
            for (lineno, line) in all_lines(&source) {
                if line.contains("#[allow(") {
                    violations.add(&path, lineno, line);
                }
            }
        }
    }

    violations.assert_empty("no_allow_attributes");
}

#[test]
fn no_todo_or_unimplemented() {
    let mut violations = Violations::new();

    let dirs = [core_src(), app_src()];
    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        for path in source_files(dir.to_str().expect("non-UTF-8 path")) {
            let source = fs::read_to_string(&path).expect("failed to read source file");
            for (lineno, line) in production_lines(&source) {
                if line.contains("todo!()") || line.contains("unimplemented!()") {
                    violations.add(&path, lineno, line);
                }
            }
        }
    }

    violations.assert_empty("no_todo_or_unimplemented");
}

// ---------------------------------------------------------------------------
// Self-tests for production_lines parser
// ---------------------------------------------------------------------------

#[cfg(test)]
mod parser_tests {
    use super::production_lines;

    #[test]
    fn excludes_cfg_test_block() {
        let src = r#"
fn real_code() {}

#[cfg(test)]
mod tests {
    fn test_helper() {
        something.unwrap();
    }

    #[test]
    fn it_works() {
        assert!(true);
    }
}
"#;
        let prod: Vec<_> = production_lines(src);
        // Should only contain the blank line and fn real_code() line
        assert!(
            prod.iter().all(|(_, l)| !l.contains("unwrap")),
            "unwrap inside #[cfg(test)] should be excluded"
        );
        assert!(
            prod.iter().any(|(_, l)| l.contains("real_code")),
            "production code should be included"
        );
    }

    #[test]
    fn handles_nested_braces() {
        let src = r#"
fn prod() {}

#[cfg(test)]
mod tests {
    fn helper() {
        if true {
            let x = {
                something.unwrap()
            };
        }
    }
}

fn also_prod() {}
"#;
        let prod: Vec<_> = production_lines(src);
        assert!(prod.iter().all(|(_, l)| !l.contains("unwrap")));
        assert!(prod.iter().any(|(_, l)| l.contains("prod")));
        assert!(prod.iter().any(|(_, l)| l.contains("also_prod")));
    }

    #[test]
    fn handles_cfg_test_and_brace_on_same_line() {
        let src = r#"
fn prod() {}

#[cfg(test)] mod tests {
    fn t() { x.unwrap(); }
}

fn after() {}
"#;
        let prod: Vec<_> = production_lines(src);
        assert!(prod.iter().all(|(_, l)| !l.contains("unwrap")));
        assert!(prod.iter().any(|(_, l)| l.contains("after")));
    }

    #[test]
    fn no_test_block_returns_all_lines() {
        let src = "fn main() {}\nfn helper() {}";
        let prod = production_lines(src);
        assert_eq!(prod.len(), 2);
    }

    #[test]
    fn multiple_test_modules() {
        let src = r#"
fn a() {}

#[cfg(test)]
mod tests1 {
    fn t1() { x.unwrap(); }
}

fn b() {}

#[cfg(test)]
mod tests2 {
    fn t2() { y.unwrap(); }
}

fn c() {}
"#;
        let prod: Vec<_> = production_lines(src);
        assert!(prod.iter().all(|(_, l)| !l.contains("unwrap")));
        assert!(prod.iter().any(|(_, l)| l.contains("fn a")));
        assert!(prod.iter().any(|(_, l)| l.contains("fn b")));
        assert!(prod.iter().any(|(_, l)| l.contains("fn c")));
    }
}
