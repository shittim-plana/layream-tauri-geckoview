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

    call_function(&func_name, &args, ctx, depth)
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

        "unixtime" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string()
        }

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
