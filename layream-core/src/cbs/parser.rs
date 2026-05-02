use std::collections::HashMap;

const MAX_DEPTH: usize = 20;

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

#[allow(dead_code)]
enum BlockType {
    When { active: bool },
    Pure,
    Code,
    Escape,
    Each { items: Vec<String>, var_name: String },
    Func { name: String, args: Vec<String> },
}

pub fn evaluate(input: &str, ctx: &mut CbsContext) -> String {
    evaluate_depth(input, ctx, 0)
}

fn evaluate_depth(input: &str, ctx: &mut CbsContext, depth: usize) -> String {
    if depth > MAX_DEPTH {
        return input.to_string();
    }

    let mut result = String::new();
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < bytes.len() {
        if pos + 1 < bytes.len() && bytes[pos] == b'{' && bytes[pos + 1] == b'{' {
            if let Some(end) = find_closing(input, pos + 2) {
                let tag = &input[pos + 2..end];
                let evaluated = evaluate_tag(tag, ctx, depth);
                result.push_str(&evaluated);
                pos = end + 2;
                continue;
            }
        }
        result.push(bytes[pos] as char);
        pos += 1;
    }

    result
}

fn find_closing(input: &str, start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut depth = 1u32;
    let mut i = start;
    while i < bytes.len() {
        if i + 1 < bytes.len() {
            if bytes[i] == b'{' && bytes[i + 1] == b'{' {
                depth += 1;
                i += 2;
                continue;
            }
            if bytes[i] == b'}' && bytes[i + 1] == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    None
}

fn evaluate_tag(tag: &str, ctx: &mut CbsContext, depth: usize) -> String {
    let tag = tag.trim();

    if tag.starts_with('#') {
        return evaluate_block(tag, ctx, depth);
    }

    if tag.starts_with("//") {
        return String::new();
    }

    if tag.starts_with("? ") || tag.starts_with("?") {
        let expr = tag.strip_prefix("? ").or_else(|| tag.strip_prefix("?")).unwrap_or("");
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

fn evaluate_block(tag: &str, _ctx: &mut CbsContext, _depth: usize) -> String {
    let _ = tag;
    String::new()
}

fn normalize_func_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .replace(['_', '-', ' '], "")
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
        "contains" => {
            if args.len() >= 2 {
                if args[0].contains(&*args[1]) { "1" } else { "0" }
            } else {
                "0"
            }
            .to_string()
        }
        "startswith" => {
            if args.len() >= 2 {
                if args[0].starts_with(&*args[1]) { "1" } else { "0" }
            } else {
                "0"
            }
            .to_string()
        }
        "endswith" => {
            if args.len() >= 2 {
                if args[0].ends_with(&*args[1]) { "1" } else { "0" }
            } else {
                "0"
            }
            .to_string()
        }
        "reverse" => {
            args.first()
                .map(|s| s.chars().rev().collect())
                .unwrap_or_default()
        }
        "tonumber" => {
            let s = args.first().map(|s| s.as_str()).unwrap_or("");
            s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect()
        }

        "equal" => {
            if args.len() >= 2 { if args[0] == args[1] { "1" } else { "0" } } else { "0" }
                .to_string()
        }
        "notequal" => {
            if args.len() >= 2 { if args[0] != args[1] { "1" } else { "0" } } else { "0" }
                .to_string()
        }
        "greater" => cmp_num(args, |a, b| a > b),
        "less" => cmp_num(args, |a, b| a < b),
        "greaterequal" => cmp_num(args, |a, b| a >= b),
        "lessequal" => cmp_num(args, |a, b| a <= b),
        "and" => {
            if args.len() >= 2 {
                if is_truthy(&args[0]) && is_truthy(&args[1]) { "1" } else { "0" }
            } else {
                "0"
            }
            .to_string()
        }
        "or" => {
            if args.len() >= 2 {
                if is_truthy(&args[0]) || is_truthy(&args[1]) { "1" } else { "0" }
            } else {
                "0"
            }
            .to_string()
        }
        "not" => {
            if is_truthy(args.first().map(|s| s.as_str()).unwrap_or("")) {
                "0"
            } else {
                "1"
            }
            .to_string()
        }

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

        "history" | "messages" => {
            ctx.messages
                .iter()
                .map(|m| format!("{}: {}", m.role, m.data))
                .collect::<Vec<_>>()
                .join("\n")
        }
        "lastmessage" => {
            ctx.messages.last().map(|m| m.data.clone()).unwrap_or_default()
        }
        "lastmessageid" | "lastmessageindex" => {
            ctx.messages.len().saturating_sub(1).to_string()
        }
        "isfirstmsg" | "isfirstmessage" => {
            if ctx.chat_index == 0 { "1" } else { "0" }.to_string()
        }

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

        _ => format!("{{{{{}}}}}", if args.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", name, args.join("::"))
        }),
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

fn eval_math(expr: &str) -> String {
    let tokens = tokenize_expr(expr);
    let rpn = to_rpn(&tokens);
    match eval_rpn(&rpn) {
        Some(v) if v.is_nan() => "NaN".to_string(),
        Some(v) if v == v.floor() && v.abs() < 1e15 => format!("{}", v as i64),
        Some(v) => format!("{}", v),
        None => "NaN".to_string(),
    }
}

#[derive(Debug, Clone)]
enum Token {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

fn tokenize_expr(expr: &str) -> Vec<Token> {
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
                tokens.push(Token::Num(n));
            }
        } else if c == '-'
            && (tokens.is_empty()
                || matches!(tokens.last(), Some(Token::Op(_)) | Some(Token::LParen)))
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
                tokens.push(Token::Num(n));
            }
        } else if "+-*/%^".contains(c) {
            tokens.push(Token::Op(c));
            chars.next();
        } else if c == '(' {
            tokens.push(Token::LParen);
            chars.next();
        } else if c == ')' {
            tokens.push(Token::RParen);
            chars.next();
        } else {
            chars.next();
        }
    }
    tokens
}

fn precedence(op: char) -> u8 {
    match op {
        '+' | '-' => 1,
        '*' | '/' | '%' => 2,
        '^' => 3,
        _ => 0,
    }
}

fn to_rpn(tokens: &[Token]) -> Vec<Token> {
    let mut output = Vec::new();
    let mut ops: Vec<Token> = Vec::new();

    for token in tokens {
        match token {
            Token::Num(_) => output.push(token.clone()),
            Token::Op(op) => {
                while let Some(Token::Op(top)) = ops.last() {
                    if precedence(*top) >= precedence(*op) {
                        output.push(ops.pop().unwrap());
                    } else {
                        break;
                    }
                }
                ops.push(token.clone());
            }
            Token::LParen => ops.push(token.clone()),
            Token::RParen => {
                while let Some(top) = ops.last() {
                    if matches!(top, Token::LParen) {
                        ops.pop();
                        break;
                    }
                    output.push(ops.pop().unwrap());
                }
            }
        }
    }
    while let Some(op) = ops.pop() {
        output.push(op);
    }
    output
}

fn eval_rpn(tokens: &[Token]) -> Option<f64> {
    let mut stack = Vec::new();
    for token in tokens {
        match token {
            Token::Num(n) => stack.push(*n),
            Token::Op(op) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_variable() {
        let mut ctx = CbsContext::default();
        ctx.char_name = "Alice".into();
        ctx.user_name = "Bob".into();
        assert_eq!(evaluate("Hello {{char}}!", &mut ctx), "Hello Alice!");
        assert_eq!(evaluate("{{user}} says hi", &mut ctx), "Bob says hi");
    }

    #[test]
    fn nested_tags() {
        let mut ctx = CbsContext::default();
        ctx.char_name = "Alice".into();
        assert_eq!(evaluate("{{upper::{{char}}}}", &mut ctx), "ALICE");
    }

    #[test]
    fn math_calc() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{calc::2 + 3 * 4}}", &mut ctx), "14");
        assert_eq!(evaluate("{{? 10 / 3}}", &mut ctx), "3.3333333333333335");
        assert_eq!(evaluate("{{calc::(2 + 3) * 4}}", &mut ctx), "20");
    }

    #[test]
    fn string_functions() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{lower::HELLO}}", &mut ctx), "hello");
        assert_eq!(evaluate("{{upper::hello}}", &mut ctx), "HELLO");
        assert_eq!(evaluate("{{length::hello}}", &mut ctx), "5");
        assert_eq!(evaluate("{{capitalize::hello}}", &mut ctx), "Hello");
        assert_eq!(evaluate("{{replace::hello world::world::rust}}", &mut ctx), "hello rust");
    }

    #[test]
    fn comparison() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{equal::a::a}}", &mut ctx), "1");
        assert_eq!(evaluate("{{equal::a::b}}", &mut ctx), "0");
        assert_eq!(evaluate("{{greater::5::3}}", &mut ctx), "1");
        assert_eq!(evaluate("{{less::5::3}}", &mut ctx), "0");
    }

    #[test]
    fn variables() {
        let mut ctx = CbsContext::default();
        evaluate("{{setvar::count::10}}", &mut ctx);
        assert_eq!(evaluate("{{getvar::count}}", &mut ctx), "10");
        evaluate("{{addvar::count::5}}", &mut ctx);
        assert_eq!(evaluate("{{getvar::count}}", &mut ctx), "15");
    }

    #[test]
    fn temp_variables() {
        let mut ctx = CbsContext::default();
        evaluate("{{settempvar::x::hello}}", &mut ctx);
        assert_eq!(evaluate("{{tempvar::x}}", &mut ctx), "hello");
    }

    #[test]
    fn logical_ops() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{and::1::1}}", &mut ctx), "1");
        assert_eq!(evaluate("{{and::1::0}}", &mut ctx), "0");
        assert_eq!(evaluate("{{or::0::1}}", &mut ctx), "1");
        assert_eq!(evaluate("{{not::0}}", &mut ctx), "1");
        assert_eq!(evaluate("{{not::1}}", &mut ctx), "0");
    }

    #[test]
    fn escape_sequences() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{br}}", &mut ctx), "\n");
        assert_eq!(evaluate("{{bo}}", &mut ctx), "{{");
        assert_eq!(evaluate("{{bc}}", &mut ctx), "}}");
        assert_eq!(evaluate("{{dec}}", &mut ctx), ":");
    }

    #[test]
    fn comment_ignored() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("hello{{// this is comment}}world", &mut ctx), "helloworld");
    }

    #[test]
    fn unknown_function_preserved() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{unknownfunc}}", &mut ctx), "{{unknownfunc}}");
    }

    #[test]
    fn math_negative() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{calc::-5 + 3}}", &mut ctx), "-2");
    }
}
