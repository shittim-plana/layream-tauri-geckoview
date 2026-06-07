//! Evaluation of a parsed CBS template.
//!
//! Walks the [`Node`] tree produced by [`crate::cbs::parser`]. Tag arguments and
//! block bodies are raw strings that get re-evaluated here (via
//! [`crate::cbs::parser::evaluate_depth`]), preserving CBS's string-rewriting
//! semantics. The math sub-language is delegated to the LALRPOP grammar.

use std::collections::HashMap;

use crate::cbs::ast::{MathExpr, MathToken, Node};
use crate::cbs::parser::evaluate_depth;

#[derive(Debug, Clone)]
pub struct CbsContext {
    pub char_name: String,
    pub user_name: String,
    pub persona: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub example_dialogue: String,
    pub main_prompt: String,
    pub jailbreak: String,
    pub global_note: String,
    pub messages: Vec<CbsMessage>,
    pub variables: HashMap<String, String>,
    pub temp_variables: HashMap<String, String>,
    pub global_variables: HashMap<String, String>,
    pub toggles: HashMap<String, String>,
    pub chat_index: usize,
    pub model: String,
    /// Current request role (`system`/`user`/`assistant`), backing `{{role}}`.
    pub role: String,
    /// Whether the jailbreak prompt is toggled on, backing `{{jbtoggled}}`.
    pub jb_toggled: bool,
    /// Max context window size in tokens, backing `{{maxcontext}}`.
    pub max_context: usize,
    /// Identity used to seed deterministic hash-rand functions (`pick`, `rollp`,
    /// `hash`). A stable string derived from character + chat id; the same seed
    /// reproduces the same values. Empty = unseeded (still deterministic per input).
    pub hash_seed: String,
    /// User-defined functions from `{{#func name::args}}…{{/func}}`, keyed by
    /// name → (parameter names, body). Read by `{{call::name::args}}`.
    pub func_defs: HashMap<String, (Vec<String>, String)>,
}

#[derive(Debug, Clone)]
pub struct CbsMessage {
    pub role: String,
    pub data: String,
    pub time: Option<u64>,
}

impl Default for CbsContext {
    fn default() -> Self {
        Self {
            char_name: String::new(),
            user_name: String::new(),
            persona: String::new(),
            description: String::new(),
            personality: String::new(),
            scenario: String::new(),
            example_dialogue: String::new(),
            main_prompt: String::new(),
            jailbreak: String::new(),
            global_note: String::new(),
            messages: Vec::new(),
            variables: HashMap::new(),
            temp_variables: HashMap::new(),
            global_variables: HashMap::new(),
            toggles: HashMap::new(),
            chat_index: 0,
            model: String::new(),
            role: String::new(),
            jb_toggled: false,
            max_context: 0,
            hash_seed: String::new(),
            func_defs: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WsMode {
    Normal,
    Keep,
    Legacy,
}

/// Evaluate a sequence of nodes at the given recursion depth.
pub(crate) fn eval_nodes(nodes: &[Node], ctx: &mut CbsContext, depth: usize) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => result.push_str(t),
            Node::Tag(content) => result.push_str(&eval_tag(content, ctx, depth)),
            Node::Block { header, body } => {
                result.push_str(&eval_block(header, body, ctx, depth))
            }
        }
    }
    result
}

fn eval_tag(tag: &str, ctx: &mut CbsContext, depth: usize) -> String {
    let tag = tag.trim();

    if tag.starts_with("//") {
        return String::new();
    }

    if tag.starts_with("? ") || tag.starts_with('?') {
        let expr = tag
            .strip_prefix("? ")
            .or_else(|| tag.strip_prefix('?'))
            .unwrap_or("");
        return eval_math(expr);
    }

    let parts: Vec<&str> = tag.split("::").collect();
    let func_name = normalize_func_name(parts[0]);
    let args: Vec<String> = parts[1..]
        .iter()
        .map(|a| evaluate_depth(a.trim(), ctx, depth + 1))
        .collect();

    if func_name == "call" {
        return call_user_func(&args, ctx, depth);
    }

    call_function(&func_name, &args, ctx, depth)
}

/// Invoke a `{{#func}}`-defined function: `{{call::name::arg1::arg2}}`. Each
/// declared parameter is bound to the corresponding argument as a variable for
/// the duration of the body evaluation, then the prior variable map is restored
/// (function-local scoping). An unknown name yields empty output.
fn call_user_func(args: &[String], ctx: &mut CbsContext, depth: usize) -> String {
    let Some(name) = args.first() else {
        return String::new();
    };
    let Some((params, body)) = ctx.func_defs.get(name).cloned() else {
        return String::new();
    };
    let call_args = &args[1..];
    let saved = ctx.variables.clone();
    for (i, param) in params.iter().enumerate() {
        let value = call_args.get(i).cloned().unwrap_or_default();
        ctx.variables.insert(param.clone(), value);
    }
    let result = evaluate_depth(&body, ctx, depth + 1);
    ctx.variables = saved;
    result
}

fn eval_block(tag: &str, body: &str, ctx: &mut CbsContext, depth: usize) -> String {
    let tag_lower = tag.to_lowercase();

    if tag_lower.starts_with("#puredisplay") || tag_lower.starts_with("#pure") {
        return body.replace("{{", "\\{\\{").replace("}}", "\\}\\}");
    }

    if tag_lower.starts_with("#if_pure") || tag_lower.starts_with("#ifpure") {
        let condition = tag
            .splitn(2, |c: char| c == ' ' || c == ':')
            .nth(1)
            .unwrap_or("")
            .trim_start_matches(':');
        let cond_val = evaluate_depth(condition.trim(), ctx, depth + 1);
        if is_truthy(&cond_val) {
            return evaluate_depth(body, ctx, depth + 1);
        }
        return String::new();
    }

    if tag_lower.starts_with("#if") {
        let condition = tag.splitn(2, |c: char| c == ' ').nth(1).unwrap_or("");
        let cond_val = evaluate_depth(condition.trim(), ctx, depth + 1);
        if is_truthy(&cond_val) {
            return trim_block_lines(&evaluate_depth(body, ctx, depth + 1), WsMode::Legacy);
        }
        return String::new();
    }

    if tag_lower.starts_with("#when") || tag_lower.starts_with(":when") {
        return evaluate_when_block(tag, body, ctx, depth);
    }

    if tag_lower.starts_with("#each") || tag_lower.starts_with(":each") {
        return evaluate_each_block(tag, body, ctx, depth);
    }

    if tag_lower.starts_with("#code") {
        // Strip newlines/tabs from the body, then evaluate. Used to write a CBS
        // program across multiple indented lines without the whitespace leaking
        // into the output.
        let stripped: String = body.chars().filter(|c| *c != '\n' && *c != '\t').collect();
        return evaluate_depth(&stripped, ctx, depth + 1);
    }

    if tag_lower.starts_with("#func") {
        // Define a callable: `#func name::param1::param2`. Body is stored raw and
        // re-evaluated at each `{{call::name::...}}`. Definition emits nothing.
        let rest = tag
            .splitn(2, |c: char| c == ' ' || c == ':')
            .nth(1)
            .unwrap_or("")
            .trim_start_matches(':');
        let mut parts = rest.split("::").map(|s| s.trim().to_string());
        if let Some(name) = parts.next().filter(|n| !n.is_empty()) {
            let params: Vec<String> = parts.filter(|p| !p.is_empty()).collect();
            ctx.func_defs.insert(name, (params, body.to_string()));
        }
        return String::new();
    }

    body.to_string()
}

fn evaluate_when_block(tag: &str, body: &str, ctx: &mut CbsContext, depth: usize) -> String {
    let raw_args = if tag.contains("::") {
        let rest = tag.splitn(2, "::").nth(1).unwrap_or("");
        rest.split("::").map(|s| s.to_string()).collect::<Vec<_>>()
    } else {
        let rest = tag.splitn(2, ' ').nth(1).unwrap_or("").trim();
        if rest.is_empty() {
            vec![]
        } else {
            vec![rest.to_string()]
        }
    };

    let mut ws_mode = WsMode::Normal;
    let mut statement: Vec<String> = Vec::new();

    for arg in &raw_args {
        let lower = arg.to_lowercase();
        match lower.as_str() {
            "keep" => ws_mode = WsMode::Keep,
            "legacy" => ws_mode = WsMode::Legacy,
            _ => statement.push(arg.clone()),
        }
    }

    let condition_result = evaluate_when_condition(&mut statement, ctx, depth);

    if ws_mode == WsMode::Legacy {
        let (true_body, false_body) = split_else(body);
        if condition_result {
            return trim_block_lines(&evaluate_depth(true_body, ctx, depth + 1), WsMode::Legacy);
        }
        return match false_body {
            Some(fb) => trim_block_lines(&evaluate_depth(fb, ctx, depth + 1), WsMode::Legacy),
            None => String::new(),
        };
    }

    let (true_body, false_body) = split_else(body);

    let content = if condition_result {
        true_body
    } else {
        false_body.unwrap_or("")
    };

    let evaluated = evaluate_depth(content, ctx, depth + 1);
    trim_block_lines(&evaluated, ws_mode)
}

fn evaluate_when_condition(
    statement: &mut Vec<String>,
    ctx: &mut CbsContext,
    depth: usize,
) -> bool {
    if statement.is_empty() {
        return false;
    }

    let i = 0;
    while i + 2 < statement.len() {
        let left = statement[i].clone();
        let op = statement[i + 1].clone();
        let right = statement[i + 2].clone();

        let op_lower = op.to_lowercase();
        let result = match op_lower.as_str() {
            "and" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                Some(is_truthy(&l) && is_truthy(&r))
            }
            "or" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                Some(is_truthy(&l) || is_truthy(&r))
            }
            "is" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                Some(l == r)
            }
            "isnot" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                Some(l != r)
            }
            ">" => match (
                evaluate_depth(&left, ctx, depth + 1).parse::<f64>(),
                evaluate_depth(&right, ctx, depth + 1).parse::<f64>(),
            ) {
                (Ok(l), Ok(r)) => Some(l > r),
                _ => Some(false),
            },
            "<" => match (
                evaluate_depth(&left, ctx, depth + 1).parse::<f64>(),
                evaluate_depth(&right, ctx, depth + 1).parse::<f64>(),
            ) {
                (Ok(l), Ok(r)) => Some(l < r),
                _ => Some(false),
            },
            ">=" => match (
                evaluate_depth(&left, ctx, depth + 1).parse::<f64>(),
                evaluate_depth(&right, ctx, depth + 1).parse::<f64>(),
            ) {
                (Ok(l), Ok(r)) => Some(l >= r),
                _ => Some(false),
            },
            "<=" => match (
                evaluate_depth(&left, ctx, depth + 1).parse::<f64>(),
                evaluate_depth(&right, ctx, depth + 1).parse::<f64>(),
            ) {
                (Ok(l), Ok(r)) => Some(l <= r),
                _ => Some(false),
            },
            "vis" => {
                let var_val = ctx.variables.get(&right).cloned().unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                Some(var_val == l)
            }
            "vnotis" | "visnot" => {
                let var_val = ctx.variables.get(&right).cloned().unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                Some(var_val != l)
            }
            "tis" => {
                let toggle_key = right.to_string();
                let toggle_val = ctx
                    .toggles
                    .get(&toggle_key)
                    .or_else(|| ctx.global_variables.get(&toggle_key))
                    .cloned()
                    .unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                Some(toggle_val == l)
            }
            "tnotis" | "tisnot" => {
                let toggle_key = right.to_string();
                let toggle_val = ctx
                    .toggles
                    .get(&toggle_key)
                    .or_else(|| ctx.global_variables.get(&toggle_key))
                    .cloned()
                    .unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                Some(toggle_val != l)
            }
            _ => None,
        };

        match result {
            Some(b) => {
                statement.splice(
                    i..i + 3,
                    std::iter::once(if b { "1".to_string() } else { "0".to_string() }),
                );
            }
            None => {
                // Unknown operator: evaluate remaining as single value
                let r = evaluate_depth(&right, ctx, depth + 1);
                return is_truthy(&r);
            }
        }
    }

    if statement.len() == 2 {
        let (Some(right), Some(op)) = (statement.pop(), statement.pop()) else {
            return false;
        };
        let op_lower = op.to_lowercase();
        match op_lower.as_str() {
            "not" => {
                let r = evaluate_depth(&right, ctx, depth + 1);
                return !is_truthy(&r);
            }
            "var" => {
                let var_val = ctx.variables.get(&right).cloned().unwrap_or_default();
                return is_truthy(&var_val);
            }
            "toggle" => {
                let toggle_key = right.to_string();
                let toggle_val = ctx
                    .toggles
                    .get(&toggle_key)
                    .or_else(|| ctx.global_variables.get(&toggle_key))
                    .cloned()
                    .unwrap_or_default();
                return is_truthy(&toggle_val);
            }
            _ => {
                let l = evaluate_depth(&op, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                return is_truthy(&l) || is_truthy(&r);
            }
        }
    }

    if statement.len() == 1 {
        let val = evaluate_depth(&statement[0], ctx, depth + 1);
        return is_truthy(&val);
    }

    false
}

fn split_else(body: &str) -> (&str, Option<&str>) {
    if let Some(idx) = body.find("{{:else}}") {
        let true_part = &body[..idx];
        let false_part = &body[idx + 9..];
        return (true_part, Some(false_part));
    }

    let lines: Vec<&str> = body.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "{{:else}}" {
            let true_end = body.lines().take(i).map(|l| l.len() + 1).sum::<usize>();
            let false_start = true_end + line.len() + 1;
            if false_start <= body.len() {
                return (
                    &body[..true_end.saturating_sub(1)],
                    Some(&body[false_start.min(body.len())..]),
                );
            }
        }
    }

    (body, None)
}

fn evaluate_each_block(tag: &str, body: &str, ctx: &mut CbsContext, depth: usize) -> String {
    let rest = if tag.contains("::") {
        tag.splitn(2, "::").nth(1).unwrap_or("")
    } else {
        tag.splitn(2, ' ').nth(1).unwrap_or("")
    };

    let (array_str, var_name) = if let Some(as_idx) = rest.rfind(" as ") {
        (&rest[..as_idx], rest[as_idx + 4..].trim())
    } else if let Some(space_idx) = rest.rfind(' ') {
        (&rest[..space_idx], rest[space_idx + 1..].trim())
    } else {
        (rest, "slot")
    };

    let array_str_eval = evaluate_depth(array_str.trim(), ctx, depth + 1);
    let items = parse_array_for_each(&array_str_eval);

    let slot_pattern = format!("{{{{slot::{}}}}}", var_name);
    let mut result = String::new();
    for item in &items {
        let replaced = body.replace(&slot_pattern, item);
        result.push_str(&evaluate_depth(&replaced, ctx, depth + 1));
    }

    trim_block_lines(&result, WsMode::Normal)
}

fn parse_array_for_each(s: &str) -> Vec<String> {
    let trimmed = s.trim();
    if trimmed.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
            return arr
                .into_iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                })
                .collect();
        }
    }
    trimmed.split('\u{00A7}').map(|s| s.to_string()).collect()
}

fn trim_block_lines(content: &str, mode: WsMode) -> String {
    match mode {
        WsMode::Keep => content.to_string(),
        WsMode::Normal | WsMode::Legacy => {
            let mut lines: Vec<&str> = content.lines().collect();
            while lines.first().is_some_and(|l| l.trim().is_empty()) {
                lines.remove(0);
            }
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            lines.join("\n")
        }
    }
}

fn normalize_func_name(name: &str) -> String {
    name.trim().to_lowercase().replace(['_', '-', ' '], "")
}

fn call_function(name: &str, args: &[String], ctx: &mut CbsContext, _depth: usize) -> String {
    match name.as_ref() {
        "char" | "bot" => ctx.char_name.clone(),
        "user" => ctx.user_name.clone(),
        "persona" => ctx.persona.clone(),
        "description" | "chardesc" => ctx.description.clone(),
        "personality" | "charpersona" => ctx.personality.clone(),
        "scenario" => ctx.scenario.clone(),
        "exampledialogue" | "examplemessage" => ctx.example_dialogue.clone(),
        "mainprompt" | "systemprompt" => ctx.main_prompt.clone(),
        "jb" | "jailbreak" => ctx.jailbreak.clone(),
        "globalnote" => ctx.global_note.clone(),
        "model" => ctx.model.clone(),
        "chatindex" => ctx.chat_index.to_string(),

        "br" | "newline" => "\n".to_string(),
        "cbr" | "cnl" | "cnewline" => "\\n".to_string(),
        "blank" | "none" => String::new(),
        "bo" | "ddecbo" => "{{".to_string(),
        "bc" | "ddecbc" => "}}".to_string(),
        "decbo" => "{".to_string(),
        "decbc" => "}".to_string(),
        "dec" | "displayescapedcolon" => ":".to_string(),

        "tempvar" | "gettempvar" => {
            let key = args.first().map(|s| s.as_str()).unwrap_or("");
            ctx.temp_variables.get(key).cloned().unwrap_or_default()
        }
        "settempvar" => {
            if args.len() >= 2 {
                ctx.temp_variables.insert(args[0].clone(), args[1].clone());
            }
            String::new()
        }
        "getvar" => {
            let key = args.first().map(|s| s.as_str()).unwrap_or("");
            ctx.variables.get(key).cloned().unwrap_or_default()
        }
        "setvar" => {
            if args.len() >= 2 {
                ctx.variables.insert(args[0].clone(), args[1].clone());
            }
            String::new()
        }
        "setdefaultvar" => {
            if args.len() >= 2 && !ctx.variables.contains_key(&args[0]) {
                ctx.variables.insert(args[0].clone(), args[1].clone());
            }
            String::new()
        }
        "getglobalvar" => {
            let key = args.first().map(|s| s.as_str()).unwrap_or("");
            ctx.global_variables.get(key).cloned().unwrap_or_default()
        }

        "calc" => {
            let expr = args.first().map(|s| s.as_str()).unwrap_or("");
            eval_math(expr)
        }
        "round" => {
            let n: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            format!("{}", n.round() as i64)
        }
        "floor" => {
            let n: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            format!("{}", n.floor() as i64)
        }
        "ceil" => {
            let n: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            format!("{}", n.ceil() as i64)
        }
        "abs" => {
            let n: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            if n == n.floor() {
                format!("{}", n.abs() as i64)
            } else {
                format!("{}", n.abs())
            }
        }
        "pow" => {
            let base: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let exp: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            format!("{}", base.powf(exp))
        }
        "min" => {
            let vals: Vec<f64> = args.iter().filter_map(|s| s.parse().ok()).collect();
            vals.into_iter()
                .reduce(f64::min)
                .map(|v| format!("{}", v))
                .unwrap_or_default()
        }
        "max" => {
            let vals: Vec<f64> = args.iter().filter_map(|s| s.parse().ok()).collect();
            vals.into_iter()
                .reduce(f64::max)
                .map(|v| format!("{}", v))
                .unwrap_or_default()
        }
        "remaind" => {
            let a: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let b: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1.0);
            format!("{}", a % b)
        }
        "fixnum" | "fixnumber" => {
            let n: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let d: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            format!("{:.prec$}", n, prec = d)
        }
        "randint" => {
            let min: i64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            let max: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
            if min >= max {
                return min.to_string();
            }
            use rand::Rng;
            let n = rand::rng().random_range(min..=max);
            n.to_string()
        }

        "length" => {
            let s = args.first().map(|s| s.as_str()).unwrap_or("");
            s.chars().count().to_string()
        }
        "lower" => args.first().map(|s| s.to_lowercase()).unwrap_or_default(),
        "upper" => args.first().map(|s| s.to_uppercase()).unwrap_or_default(),
        "capitalize" => {
            let s = args.first().map(|s| s.as_str()).unwrap_or("");
            let mut chars = s.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        }
        "trim" => args.first().map(|s| s.trim().to_string()).unwrap_or_default(),
        "replace" => {
            if args.len() >= 3 {
                args[0].replace(&*args[1], &args[2])
            } else {
                args.first().cloned().unwrap_or_default()
            }
        }
        "contains" => if args.len() >= 2 {
            if args[0].contains(&*args[1]) { "1" } else { "0" }
        } else {
            "0"
        }
        .to_string(),
        "startswith" => if args.len() >= 2 {
            if args[0].starts_with(&*args[1]) { "1" } else { "0" }
        } else {
            "0"
        }
        .to_string(),
        "endswith" => if args.len() >= 2 {
            if args[0].ends_with(&*args[1]) { "1" } else { "0" }
        } else {
            "0"
        }
        .to_string(),
        "reverse" => args
            .first()
            .map(|s| s.chars().rev().collect())
            .unwrap_or_default(),
        "tonumber" => {
            let s = args.first().map(|s| s.as_str()).unwrap_or("");
            s.chars()
                .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect()
        }

        "equal" => if args.len() >= 2 {
            if args[0] == args[1] { "1" } else { "0" }
        } else {
            "0"
        }
        .to_string(),
        "notequal" => if args.len() >= 2 {
            if args[0] != args[1] { "1" } else { "0" }
        } else {
            "0"
        }
        .to_string(),
        "greater" => cmp_num(args, |a, b| a > b),
        "less" => cmp_num(args, |a, b| a < b),
        "greaterequal" => cmp_num(args, |a, b| a >= b),
        "lessequal" => cmp_num(args, |a, b| a <= b),
        "and" => if args.len() >= 2 {
            if is_truthy(&args[0]) && is_truthy(&args[1]) { "1" } else { "0" }
        } else {
            "0"
        }
        .to_string(),
        "or" => if args.len() >= 2 {
            if is_truthy(&args[0]) || is_truthy(&args[1]) { "1" } else { "0" }
        } else {
            "0"
        }
        .to_string(),
        "not" => if is_truthy(args.first().map(|s| s.as_str()).unwrap_or("")) {
            "0"
        } else {
            "1"
        }
        .to_string(),

        "random" => {
            if args.is_empty() {
                use rand::Rng;
                format!("{}", rand::rng().random::<f64>())
            } else {
                use rand::Rng;
                let idx = rand::rng().random_range(0..args.len());
                args[idx].clone()
            }
        }

        "history" | "messages" => ctx
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.data))
            .collect::<Vec<_>>()
            .join("\n"),
        "lastmessage" => ctx.messages.last().map(|m| m.data.clone()).unwrap_or_default(),
        "lastmessageid" | "lastmessageindex" => ctx.messages.len().saturating_sub(1).to_string(),
        "isfirstmsg" | "isfirstmessage" => if ctx.chat_index == 0 { "1" } else { "0" }.to_string(),

        "addvar" => {
            if args.len() >= 2 {
                let key = &args[0];
                let add: f64 = args[1].parse().unwrap_or(0.0);
                let cur: f64 = ctx.variables.get(key).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                let new_val = format!("{}", cur + add);
                ctx.variables.insert(key.clone(), new_val);
            }
            String::new()
        }

        "unixtime" => now_unix().to_string(),

        // ── Group A: arrays ──────────────────────────────────────────────
        "makearray" | "array" | "a" => make_array(args),
        "arrayelement" => {
            let arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            let idx: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            // Negative indices count from the end (Array.at semantics).
            let len = arr.len() as i64;
            let resolved = if idx < 0 { len + idx } else { idx };
            if resolved >= 0 && resolved < len {
                arr[resolved as usize].clone()
            } else {
                "null".to_string()
            }
        }
        "arraylength" => parse_array(args.first().map(|s| s.as_str()).unwrap_or(""))
            .len()
            .to_string(),
        "arraypush" => {
            let mut arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            arr.push(args.get(1).cloned().unwrap_or_default());
            make_array(&arr)
        }
        "arraypop" => {
            let mut arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            arr.pop();
            make_array(&arr)
        }
        "arrayshift" => {
            let mut arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            if !arr.is_empty() {
                arr.remove(0);
            }
            make_array(&arr)
        }
        "arraysplice" => {
            let mut arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            let start: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let delete: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let insert = args.get(3).cloned();
            let start = start.min(arr.len());
            let end = (start + delete).min(arr.len());
            let tail: Vec<String> = arr.split_off(end);
            arr.truncate(start);
            if let Some(item) = insert {
                arr.push(item);
            }
            arr.extend(tail);
            make_array(&arr)
        }
        "arrayassert" => {
            let mut arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            let idx: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let val = args.get(2).cloned().unwrap_or_default();
            if idx >= arr.len() {
                arr.resize(idx + 1, String::new());
                arr[idx] = val;
            }
            make_array(&arr)
        }
        "filter" => {
            let arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            let mode = args.get(1).map(|s| s.as_str()).unwrap_or("all");
            let first_index = |v: &str, a: &[String]| a.iter().position(|x| x == v);
            let out: Vec<String> = arr
                .iter()
                .enumerate()
                .filter(|(i, f)| match mode {
                    "nonempty" => !f.is_empty(),
                    "unique" => first_index(f, &arr) == Some(*i),
                    // "all" (default): drop empties and duplicates.
                    _ => !f.is_empty() && first_index(f, &arr) == Some(*i),
                })
                .map(|(_, f)| f.clone())
                .collect();
            make_array(&out)
        }
        "spread" => parse_array(args.first().map(|s| s.as_str()).unwrap_or("")).join("::"),
        "split" => {
            let parts: Vec<String> = if args.len() >= 2 {
                args[0].split(&*args[1]).map(|s| s.to_string()).collect()
            } else {
                args.first().map(|s| vec![s.clone()]).unwrap_or_default()
            };
            make_array(&parts)
        }
        "join" => {
            let arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            let sep = args.get(1).map(|s| s.as_str()).unwrap_or("");
            arr.join(sep)
        }

        // ── Group A: dictionaries ────────────────────────────────────────
        "makedict" | "dict" | "d" | "makeobject" | "object" | "o" => {
            let mut map = serde_json::Map::new();
            for arg in args {
                if let Some(eq) = arg.find('=') {
                    let key = arg[..eq].to_string();
                    let value = arg[eq + 1..].to_string();
                    map.insert(key, serde_json::Value::String(value));
                }
            }
            serde_json::Value::Object(map).to_string()
        }
        "dictelement" | "objectelement" => {
            let dict = parse_dict(args.first().map(|s| s.as_str()).unwrap_or(""));
            let key = args.get(1).map(|s| s.as_str()).unwrap_or("");
            dict.get(key)
                .map(|v| json_value_to_string(v.clone()))
                .unwrap_or_else(|| "null".to_string())
        }
        "objectassert" | "dictassert" => {
            let mut dict = parse_dict(args.first().map(|s| s.as_str()).unwrap_or(""));
            let key = args.get(1).cloned().unwrap_or_default();
            let val = args.get(2).cloned().unwrap_or_default();
            if !dict.contains_key(&key) {
                dict.insert(key, serde_json::Value::String(val));
            }
            serde_json::Value::Object(dict).to_string()
        }
        "element" | "ele" => {
            let mut current = args.first().cloned().unwrap_or_default();
            for key in &args[1..] {
                let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&current) else {
                    return "null".to_string();
                };
                let next = match &parsed {
                    serde_json::Value::Object(map) => map.get(key).cloned(),
                    serde_json::Value::Array(arr) => {
                        key.parse::<usize>().ok().and_then(|i| arr.get(i).cloned())
                    }
                    _ => None,
                };
                match next {
                    Some(v) => current = json_value_to_string(v),
                    None => return "null".to_string(),
                }
                if current.is_empty() {
                    return "null".to_string();
                }
            }
            current
        }

        // ── Group A: booleans / aggregates ───────────────────────────────
        "all" => {
            let list = if args.len() > 1 {
                args.to_vec()
            } else {
                parse_array(args.first().map(|s| s.as_str()).unwrap_or(""))
            };
            if list.iter().all(|f| f == "1") { "1" } else { "0" }.to_string()
        }
        "any" => {
            let list = if args.len() > 1 {
                args.to_vec()
            } else {
                parse_array(args.first().map(|s| s.as_str()).unwrap_or(""))
            };
            if list.iter().any(|f| f == "1") { "1" } else { "0" }.to_string()
        }
        "sum" => num_to_string(numeric_operands(args).iter().sum()),
        "average" => {
            let ops = numeric_operands(args);
            if ops.is_empty() {
                "NaN".to_string()
            } else {
                num_to_string(ops.iter().sum::<f64>() / ops.len() as f64)
            }
        }
        "range" => {
            let arr = parse_array(args.first().map(|s| s.as_str()).unwrap_or(""));
            let nums: Vec<i64> = arr.iter().map(|s| num_or_zero(s) as i64).collect();
            let (start, end, step) = match nums.len() {
                0 => (0, 0, 1),
                1 => (0, nums[0], 1),
                2 => (nums[0], nums[1], 1),
                _ => (nums[0], nums[1], if nums[2] == 0 { 1 } else { nums[2] }),
            };
            let mut out = Vec::new();
            let mut i = start;
            if step > 0 {
                while i < end {
                    out.push(i.to_string());
                    i += step;
                }
            } else {
                while i > end {
                    out.push(i.to_string());
                    i += step;
                }
            }
            make_array(&out)
        }

        // ── Group A: time ────────────────────────────────────────────────
        "time" => {
            // No format argument at all → h:m:s now. An explicit (even empty)
            // format argument is passed through to the formatter (empty → "").
            if args.is_empty() {
                let t = utc_from_unix(now_unix());
                format!("{}:{}:{}", t.hour, t.minute, t.second)
            } else {
                let secs = args.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0) / 1000;
                date_time_format(&args[0], secs)
            }
        }
        "date" | "datetimeformat" => {
            if args.is_empty() {
                let t = utc_from_unix(now_unix());
                format!("{}-{}-{}", t.year, t.month, t.day)
            } else {
                let secs = args.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0) / 1000;
                date_time_format(&args[0], secs)
            }
        }
        "isotime" => {
            let t = utc_from_unix(now_unix());
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                t.year, t.month, t.day, t.hour, t.minute, t.second
            )
        }
        "isodate" => {
            let t = utc_from_unix(now_unix());
            format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
        }

        // ── Group A: escape-output (emit actual characters) ──────────────
        "debo" | "displayescapedbracketopen" => "(".to_string(),
        "debc" | "displayescapedbracketclose" => ")".to_string(),
        "deabo" | "displayescapedanglebracketopen" => "<".to_string(),
        "deabc" | "displayescapedanglebracketclose" => ">".to_string(),
        "displayescapedsemicolon" => ";".to_string(),

        // ── Group A: crypto / encoding ───────────────────────────────────
        "xor" | "xorencrypt" | "xorencode" | "xore" => {
            use base64::Engine;
            let bytes: Vec<u8> = args
                .first()
                .map(|s| s.bytes().map(|b| b ^ 0xFF).collect())
                .unwrap_or_default();
            base64::engine::general_purpose::STANDARD.encode(bytes)
        }
        "xordecrypt" | "xordecode" | "xord" => {
            use base64::Engine;
            let input = args.first().map(|s| s.as_str()).unwrap_or("");
            match base64::engine::general_purpose::STANDARD.decode(input) {
                Ok(bytes) => {
                    let decoded: Vec<u8> = bytes.into_iter().map(|b| b ^ 0xFF).collect();
                    String::from_utf8_lossy(&decoded).to_string()
                }
                Err(_) => String::new(),
            }
        }
        "crypt" | "crypto" | "caesar" | "encrypt" | "decrypt" => {
            let input = args.first().map(|s| s.as_str()).unwrap_or("");
            let shift: i64 = args
                .get(1)
                .and_then(|s| s.parse().ok())
                .filter(|_| args.get(1).is_some_and(|s| !s.is_empty()))
                .unwrap_or(32768);
            input
                .chars()
                .map(|c| {
                    let code = c as u32;
                    if code > 65535 {
                        return c.to_string();
                    }
                    let shifted = (code as i64 + shift).rem_euclid(65536) as u32;
                    char::from_u32(shifted).map(|x| x.to_string()).unwrap_or_default()
                })
                .collect()
        }
        "unicodeencode" => {
            let s = args.first().map(|s| s.as_str()).unwrap_or("");
            let idx: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            // charCodeAt semantics → UTF-16 code unit.
            s.encode_utf16()
                .nth(idx)
                .map(|u| u.to_string())
                .unwrap_or_else(|| "NaN".to_string())
        }
        "unicodedecode" => {
            let code: u32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            char::from_u32(code).map(|c| c.to_string()).unwrap_or_default()
        }
        "u" | "ue" | "unicodedecodefromhex" | "unicodeencodefromhex" => {
            let code = args
                .first()
                .and_then(|s| u32::from_str_radix(s.trim(), 16).ok())
                .unwrap_or(0);
            char::from_u32(code).map(|c| c.to_string()).unwrap_or_default()
        }
        "fromhex" => {
            let v = args
                .first()
                .and_then(|s| i64::from_str_radix(s.trim(), 16).ok());
            v.map(|n| n.to_string()).unwrap_or_else(|| "NaN".to_string())
        }
        "tohex" => {
            // parseInt(args[0]).toString(16) — base-10 in, base-16 out.
            args.first()
                .and_then(|s| parse_leading_int(s))
                .map(|n| format!("{:x}", n))
                .unwrap_or_else(|| "NaN".to_string())
        }

        // ── Group A: hash / random ───────────────────────────────────────
        "hash" => {
            let word = args.first().map(|s| s.as_str()).unwrap_or("");
            let v = pick_hash_rand(0, word) * 10_000_000.0 + 1.0;
            format!("{:07}", v as i64)
        }
        "pick" => {
            // Deterministic counterpart of `random`, seeded on ctx.hash_seed and
            // the message count (chat_index here).
            let factor = pick_hash_rand(ctx.chat_index as i64, &ctx.hash_seed);
            if args.is_empty() {
                factor.to_string()
            } else {
                pick_element(args, factor)
            }
        }
        "roll" => roll_dice(args, |_| None),
        "rollp" | "rollpick" => {
            let seed = ctx.hash_seed.clone();
            let base = ctx.chat_index as i64;
            roll_dice(args, |i| Some(pick_hash_rand(base + (i as i64 * 15), &seed)))
        }
        "dice" => roll_dice(args, |_| None),

        // ── Group C: no context in core — empty (justified missing) ───────
        "metadata" | "iserror" | "prefillsupported" | "prefill" | "screenwidth"
        | "screenheight" | "assetlist" | "emotionlist" | "chardisplayasset"
        | "moduleassetlist" | "moduleenabled" | "position" | "button" | "risu"
        | "file" | "hiddenkey" => String::new(),

        // ── Group B: message context ─────────────────────────────────────
        // Ported from archive/cbs-subagent. The preset usage is snake_case
        // (`message_date`, `idle_duration`, …); `normalize_func_name` collapses
        // `_`/`-`/space so those resolve to the collapsed names matched here.
        "previouscharchat" | "lastcharmessage" => ctx
            .messages
            .iter()
            .rev()
            .find(|m| m.role != "user")
            .map(|m| m.data.clone())
            .unwrap_or_default(),
        "previoususerchat" | "lastusermessage" => ctx
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.data.clone())
            .unwrap_or_default(),
        "previouschatlog" => {
            let idx: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            ctx.messages
                .get(idx)
                .map(|m| m.data.clone())
                .unwrap_or_else(|| "Out of range".to_string())
        }
        "messagedate" => message_time_field(ctx, args, |t| {
            let u = utc_from_unix(t);
            format!("{}-{}-{}", u.year, u.month, u.day)
        }),
        "messagetime" => message_time_field(ctx, args, |t| {
            let u = utc_from_unix(t);
            format!("{}:{}:{}", u.hour, u.minute, u.second)
        }),
        "messageunixtimearray" => make_array(
            &ctx.messages
                .iter()
                .map(|m| m.time.map(|t| t.to_string()).unwrap_or_default())
                .collect::<Vec<_>>(),
        ),
        "idleduration" | "messageidleduration" => {
            // Faithful to the archive: with no current-clock/idle semantics wired
            // into this context yet, idle duration is the HH:MM:SS zero stub. No
            // fabricated diff (§2.2) — real timing is a later follow-up.
            "00:00:00".to_string()
        }
        "userhistory" => ctx
            .messages
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| m.data.clone())
            .collect::<Vec<_>>()
            .join("\n"),
        "charhistory" => ctx
            .messages
            .iter()
            .filter(|m| m.role != "user")
            .map(|m| m.data.clone())
            .collect::<Vec<_>>()
            .join("\n"),

        // ── Group B: context-backed status ───────────────────────────────
        "role" => ctx.role.clone(),
        "jbtoggled" => if ctx.jb_toggled { "1" } else { "0" }.to_string(),
        "maxcontext" => ctx.max_context.to_string(),

        _ => format!(
            "{{{{{}}}}}",
            if args.is_empty() {
                name.to_string()
            } else {
                format!("{}::{}", name, args.join("::"))
            }
        ),
    }
}

fn cmp_num(args: &[String], op: fn(f64, f64) -> bool) -> String {
    if args.len() >= 2 {
        let a: f64 = args[0].parse().unwrap_or(0.0);
        let b: f64 = args[1].parse().unwrap_or(0.0);
        if op(a, b) { "1" } else { "0" }
    } else {
        "0"
    }
    .to_string()
}

fn is_truthy(s: &str) -> bool {
    s == "1" || s.eq_ignore_ascii_case("true")
}

// ── Array / dict helpers (JSON-encoded list wire format) ──────────────────────
//
// CBS arrays are JSON arrays of strings serialized into one tag argument. A
// non-JSON argument falls back to splitting on U+00A7 (§). `make_array`
// re-escapes literal `::` inside elements as `::` so a serialized
// array survives a later `::` split.

const COLON_ESCAPE: &str = "\\u003A\\u003A";

/// Parse a CBS array argument into its string elements.
fn parse_array(s: &str) -> Vec<String> {
    if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(s) {
        return arr.into_iter().map(json_value_to_string).collect();
    }
    s.split('\u{00A7}').map(|p| p.to_string()).collect()
}

/// Parse a CBS dict argument (a JSON object) into a key→string map. Non-object
/// input yields an empty map.
fn parse_dict(s: &str) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::from_str::<serde_json::Value>(s) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}

/// Render a JSON value as the bare string CBS uses (strings unquoted, others via
/// their JSON form).
fn json_value_to_string(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

/// Serialize string elements into a CBS array, escaping embedded `::`.
fn make_array(items: &[String]) -> String {
    let escaped: Vec<serde_json::Value> = items
        .iter()
        .map(|s| serde_json::Value::String(s.replace("::", COLON_ESCAPE)))
        .collect();
    serde_json::Value::Array(escaped).to_string()
}

/// Coerce a CBS argument to f64, treating non-numeric input as 0
/// (`Number(x) || 0` semantics for the numeric aggregate functions).
fn num_or_zero(s: &str) -> f64 {
    s.trim().parse::<f64>().unwrap_or(0.0)
}

/// The numeric aggregate functions accept either a single array argument or many
/// scalar arguments. Returns the operand list as f64 (non-numeric → 0).
fn numeric_operands(args: &[String]) -> Vec<f64> {
    let raw = if args.len() > 1 {
        args.to_vec()
    } else {
        parse_array(args.first().map(|s| s.as_str()).unwrap_or(""))
    };
    raw.iter().map(|s| num_or_zero(s)).collect()
}

/// Format a number the way JS `Number.toString()` would: integers without a
/// trailing `.0`, everything else in its shortest round-trip form.
fn num_to_string(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else if v == v.floor() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

// ── Deterministic hash-rand (seeded hash + small deterministic PRNG) ──────────

/// One PRNG step. Produces an f64 in [0, 1) from a small deterministic
/// generator, including the 32-bit wrapping arithmetic (`| 0` ≡ wrapping i32).
struct Sfc32 {
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

impl Sfc32 {
    fn next(&mut self) -> f64 {
        let t = self.a.wrapping_add(self.b).wrapping_add(self.d);
        self.d = self.d.wrapping_add(1);
        self.a = self.b ^ (self.b >> 9);
        self.b = self.c.wrapping_add(self.c << 3);
        self.c = (self.c << 21) | (self.c >> 11);
        self.c = self.c.wrapping_add(t);
        (t as f64) / 4294967296.0
    }
}

/// Deterministic pseudo-random value in [0, 1), keyed on a counter `cid` and a
/// seed word. Same (cid, word) always yields the same value — this is what makes
/// `pick`/`rollp`/`hash` stable across re-renders of the same message.
fn pick_hash_rand(cid: i64, word: &str) -> f64 {
    let mut hash_address: i32 = 5515;
    let mut rand = |w: &str| -> u32 {
        for ch in w.chars() {
            // ((h << 5) + h) + charCode, all in wrapping 32-bit space (JS `<<`).
            hash_address = (hash_address << 5)
                .wrapping_add(hash_address)
                .wrapping_add(ch as i32);
        }
        hash_address as u32
    };
    let mut gen = Sfc32 {
        a: rand(word),
        b: rand(word),
        c: rand(word),
        d: rand(word),
    };
    let v = cid.rem_euclid(1000);
    for _ in 0..v {
        gen.next();
    }
    gen.next()
}

/// Pick an element from a CBS list using a [0,1) random factor, matching the
/// `pick`/`random` element-selection: a single `[...]` argument is parsed as an
/// array, otherwise the argument is split on `:` or `,` (with `\,` preserved).
fn pick_element(args: &[String], factor: f64) -> String {
    let items: Vec<String> = if args.len() == 1 {
        let a = &args[0];
        if a.starts_with('[') && a.ends_with(']') {
            parse_array(a)
        } else {
            a.replace("\\,", "\u{0007}")
                .split([':', ','])
                .map(|s| s.replace('\u{0007}', ","))
                .collect()
        }
    } else {
        args.to_vec()
    };
    if items.is_empty() {
        return String::new();
    }
    let idx = ((factor * items.len() as f64).floor() as usize).min(items.len() - 1);
    items[idx].clone()
}

/// Parse the leading integer of a string (JS `parseInt` semantics: stop at the
/// first non-digit, allow a leading sign).
fn parse_leading_int(s: &str) -> Option<i64> {
    let s = s.trim();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if (c == '-' || c == '+') && i == 0 {
            end = i + 1;
        } else if c.is_ascii_digit() {
            end = i + 1;
        } else {
            break;
        }
    }
    s[..end].parse().ok()
}

/// Roll dice in `XdY` notation (default `1d6`). `factor` supplies a [0,1) value
/// for die `i` when deterministic rolling is wanted (`rollp`); returning `None`
/// falls back to thread randomness (`roll`/`dice`).
fn roll_dice(args: &[String], factor: impl Fn(usize) -> Option<f64>) -> String {
    let Some(notation) = args.first().filter(|s| !s.is_empty()) else {
        return "1".to_string();
    };
    let parts: Vec<&str> = notation.split('d').collect();
    // `XdY` → X dice of Y sides; bare `Y` → 1 die of Y sides. A token that
    // fails to parse (e.g. "abc") yields NaN, matching the Number() path.
    let (num_str, sides_str) = match parts.as_slice() {
        [n, s] => (if n.is_empty() { "1" } else { *n }, *s),
        [s] => ("1", *s),
        _ => return "NaN".to_string(),
    };
    let (Some(num), Some(sides)) = (
        num_str.trim().parse::<i64>().ok(),
        sides_str.trim().parse::<i64>().ok(),
    ) else {
        return "NaN".to_string();
    };
    if num < 1 || sides < 1 {
        return "NaN".to_string();
    }
    let mut total: i64 = 0;
    for i in 0..num {
        let r = match factor(i as usize) {
            Some(f) => (f * sides as f64).floor() as i64 + 1,
            None => {
                use rand::Rng;
                rand::rng().random_range(1..=sides)
            }
        };
        total += r;
    }
    total.to_string()
}

// ── Date/time (token-based date formatting; UTC, dependency-free) ─────────────
//
// A browser front-end would format in the local timezone, but the headless core
// has no timezone context (§1.4: make the implicit explicit), so all date/time
// output is UTC. Callers that need local time pass an already-offset timestamp.

/// Calendar breakdown of a unix-second instant, in UTC.
struct Utc {
    year: i64,
    month: u32, // 1..=12
    day: u32,   // 1..=31
    hour: u32,
    minute: u32,
    second: u32,
}

/// Convert unix seconds to a UTC calendar date via Howard Hinnant's civil-from-
/// days algorithm (a known, branch-free structure for this exact problem).
fn utc_from_unix(unix_secs: i64) -> Utc {
    let days = unix_secs.div_euclid(86_400);
    let secs_of_day = unix_secs.rem_euclid(86_400);

    // days since 1970-01-01 → civil (y, m, d). era-based, proleptic Gregorian.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    Utc {
        year,
        month: m as u32,
        day: d as u32,
        hour: (secs_of_day / 3600) as u32,
        minute: ((secs_of_day % 3600) / 60) as u32,
        second: (secs_of_day % 60) as u32,
    }
}

/// Current unix time in whole seconds.
fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Format a unix-second timestamp (`0` = now) with the CBS token vocabulary,
/// in UTC. Locale-name tokens (MMMM, dddd, …) are unsupported and left as-is.
fn date_time_format(fmt: &str, unix_secs: i64) -> String {
    let mut fmt = fmt.trim();
    if let Some(rest) = fmt.strip_prefix(':') {
        fmt = rest;
    }
    if fmt.is_empty() || fmt.len() > 300 {
        return String::new();
    }
    let secs = if unix_secs == 0 { now_unix() } else { unix_secs };
    let t = utc_from_unix(secs);
    let hour12 = {
        let h = t.hour % 12;
        if h == 0 { 12 } else { h }
    };
    // Order matters: longest tokens first so `YYYY` wins over `YY`, etc.
    fmt.replace("YYYY", &t.year.to_string())
        .replace("YY", &format!("{:02}", t.year.rem_euclid(100)))
        .replace("MM", &format!("{:02}", t.month))
        .replace("DD", &format!("{:02}", t.day))
        .replace("HH", &format!("{:02}", t.hour))
        .replace("hh", &format!("{:02}", hour12))
        .replace("mm", &format!("{:02}", t.minute))
        .replace("ss", &format!("{:02}", t.second))
        .replace('X', &secs.to_string())
        .replace('x', &(secs * 1000).to_string())
        .replace('A', if t.hour >= 12 { "PM" } else { "AM" })
}

/// Shared body for `messagedate`/`messagetime`: format a message's timestamp.
/// An optional index argument selects a specific message; absent it, the last
/// message is used. A message with no recorded `time` yields `[Cannot get time]`
/// (graceful, no fabricated timestamp — §2.2).
fn message_time_field(ctx: &CbsContext, args: &[String], fmt: impl Fn(i64) -> String) -> String {
    let msg = match args.first().and_then(|s| s.trim().parse::<usize>().ok()) {
        Some(idx) => ctx.messages.get(idx),
        None => ctx.messages.last(),
    };
    match msg.and_then(|m| m.time) {
        Some(t) => fmt(t as i64),
        None => "[Cannot get time]".to_string(),
    }
}

// ── Math sub-language ──────────────────────────────────────────────────────

/// Evaluate a math expression string, matching the legacy `eval_math` output
/// formatting. Unparseable input yields `"NaN"`.
fn eval_math(expr: &str) -> String {
    use logos::Logos;

    // Skip lex errors (e.g. stray letters), mirroring the old tokenizer which
    // silently dropped unrecognized characters.
    let tokens: Vec<Result<(usize, MathToken, usize), ()>> = MathToken::lexer(expr)
        .spanned()
        .filter_map(|(res, span)| res.ok().map(|tok| Ok((span.start, tok, span.end))))
        .collect();

    match crate::cbs::grammar::MathParser::new().parse(tokens) {
        Ok(expr) => format_math(eval_math_expr(&expr)),
        Err(_) => eval_math_fallback(expr),
    }
}

fn eval_math_expr(e: &MathExpr) -> f64 {
    match e {
        MathExpr::Num(n) => *n,
        MathExpr::Neg(x) => -eval_math_expr(x),
        MathExpr::Bin(op, a, b) => {
            let (a, b) = (eval_math_expr(a), eval_math_expr(b));
            match op {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                '/' => a / b,
                '%' => a % b,
                '^' => a.powf(b),
                _ => f64::NAN,
            }
        }
    }
}

fn format_math(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v == v.floor() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

// ── Shunting-yard RPN fallback ───────────────────────────────────────────
//
// When the LALRPOP grammar rejects an expression (e.g. exotic whitespace
// patterns, mixed-in non-numeric chars the logos lexer stripped), this
// hand-written tokenizer + shunting-yard evaluator gives a second chance.
// Ported from the pre-LALRPOP implementation (draft/backend-cbs-opus45).

fn eval_math_fallback(expr: &str) -> String {
    let tokens = tokenize_fallback(expr);
    let rpn = to_rpn(&tokens);
    match eval_rpn(&rpn) {
        Some(v) => format_math(v),
        None => "NaN".to_string(),
    }
}

#[derive(Debug, Clone)]
enum FallbackToken {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

fn tokenize_fallback(expr: &str) -> Vec<FallbackToken> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let mut num_str = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() || d == '.' {
                    num_str.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(n) = num_str.parse::<f64>() {
                tokens.push(FallbackToken::Num(n));
            }
        } else if c == '-'
            && (tokens.is_empty()
                || matches!(
                    tokens.last(),
                    Some(FallbackToken::Op(_)) | Some(FallbackToken::LParen)
                ))
        {
            chars.next();
            let mut num_str = String::from("-");
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() || d == '.' {
                    num_str.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(n) = num_str.parse::<f64>() {
                tokens.push(FallbackToken::Num(n));
            }
        } else if "+-*/%^".contains(c) {
            tokens.push(FallbackToken::Op(c));
            chars.next();
        } else if c == '(' {
            tokens.push(FallbackToken::LParen);
            chars.next();
        } else if c == ')' {
            tokens.push(FallbackToken::RParen);
            chars.next();
        } else {
            // Skip unrecognized characters (matching legacy behavior)
            chars.next();
        }
    }
    tokens
}

fn fallback_precedence(op: char) -> u8 {
    match op {
        '+' | '-' => 1,
        '*' | '/' | '%' => 2,
        '^' => 3,
        _ => 0,
    }
}

fn to_rpn(tokens: &[FallbackToken]) -> Vec<FallbackToken> {
    let mut output = Vec::new();
    let mut ops: Vec<FallbackToken> = Vec::new();

    for token in tokens {
        match token {
            FallbackToken::Num(_) => output.push(token.clone()),
            FallbackToken::Op(op) => {
                while let Some(FallbackToken::Op(top)) = ops.last() {
                    let dominated = if *op == '^' {
                        fallback_precedence(*top) > fallback_precedence(*op)
                    } else {
                        fallback_precedence(*top) >= fallback_precedence(*op)
                    };
                    if dominated {
                        if let Some(popped) = ops.pop() {
                            output.push(popped);
                        }
                    } else {
                        break;
                    }
                }
                ops.push(token.clone());
            }
            FallbackToken::LParen => ops.push(token.clone()),
            FallbackToken::RParen => {
                while let Some(top) = ops.last() {
                    if matches!(top, FallbackToken::LParen) {
                        ops.pop();
                        break;
                    }
                    if let Some(popped) = ops.pop() {
                        output.push(popped);
                    }
                }
            }
        }
    }
    while let Some(popped) = ops.pop() {
        output.push(popped);
    }
    output
}

fn eval_rpn(tokens: &[FallbackToken]) -> Option<f64> {
    let mut stack = Vec::new();
    for token in tokens {
        match token {
            FallbackToken::Num(n) => stack.push(*n),
            FallbackToken::Op(op) => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                let result = match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => a / b,
                    '%' => a % b,
                    '^' => a.powf(b),
                    _ => return None,
                };
                stack.push(result);
            }
            _ => {}
        }
    }
    stack.pop()
}
