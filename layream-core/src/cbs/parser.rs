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

#[derive(Debug, Clone, Copy, PartialEq)]
enum WsMode {
    Normal,
    Keep,
    Legacy,
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
                let tag = input[pos + 2..end].trim();

                if tag.starts_with('#') || tag.starts_with(":each") {
                    let block_name = extract_block_name(tag);
                    let after_tag = end + 2;
                    if let Some((body, close_end)) = find_block_end(input, after_tag, &block_name) {
                        let evaluated = evaluate_block_with_body(tag, body, ctx, depth);
                        result.push_str(&evaluated);
                        pos = close_end;
                        continue;
                    }
                }

                let evaluated = evaluate_tag(tag, ctx, depth);
                result.push_str(&evaluated);
                pos = end + 2;
                continue;
            }
        }
        let ch_len = utf8_char_len(bytes[pos]);
        result.push_str(&input[pos..pos + ch_len]);
        pos += ch_len;
    }

    result
}

fn extract_block_name(tag: &str) -> String {
    let tag_lower = tag.to_lowercase();
    let name_part = if tag_lower.starts_with(":each") || tag_lower.starts_with("#each") {
        "each"
    } else if let Some(rest) = tag_lower.strip_prefix('#') {
        if let Some(idx) = rest.find(|c: char| c == ' ' || c == ':') {
            &rest[..idx]
        } else {
            rest
        }
    } else {
        return String::new();
    };
    name_part.to_string()
}

fn find_block_end<'a>(input: &'a str, start: usize, block_name: &str) -> Option<(&'a str, usize)> {
    let close_tag = format!("/{}", block_name);
    let open_prefix = format!("#{}", block_name);
    let open_prefix_colon = format!(":{}", block_name);
    let mut nest = 1u32;
    let mut i = start;
    let bytes = input.as_bytes();

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = find_closing(input, i + 2) {
                let inner = input[i + 2..end].trim().to_lowercase();
                if inner.starts_with(&open_prefix) || inner.starts_with(&open_prefix_colon) {
                    nest += 1;
                } else if inner.starts_with(&close_tag) {
                    nest -= 1;
                    if nest == 0 {
                        let body = &input[start..i];
                        return Some((body, end + 2));
                    }
                }
                i = end + 2;
                continue;
            }
        }
        i += 1;
    }
    None
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

fn evaluate_block_with_body(tag: &str, body: &str, ctx: &mut CbsContext, depth: usize) -> String {
    let tag_lower = tag.to_lowercase();

    if tag_lower.starts_with("#puredisplay") || tag_lower.starts_with("#pure") {
        return body.replace("{{", "\\{\\{").replace("}}", "\\}\\}");
    }

    if tag_lower.starts_with("#if_pure") || tag_lower.starts_with("#ifpure") {
        let condition = tag.splitn(2, |c: char| c == ' ' || c == ':')
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
        let condition = tag.splitn(2, |c: char| c == ' ')
            .nth(1)
            .unwrap_or("");
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
        if condition_result {
            return trim_block_lines(&evaluate_depth(body, ctx, depth + 1), WsMode::Legacy);
        }
        return String::new();
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

fn evaluate_when_condition(statement: &mut Vec<String>, ctx: &mut CbsContext, depth: usize) -> bool {
    if statement.is_empty() {
        return false;
    }

    while statement.len() >= 3 {
        let right = statement.pop().expect("len >= 3");
        let op = statement.pop().expect("len >= 3");
        let left = statement.pop().expect("len >= 3");

        let op_lower = op.to_lowercase();
        let result = match op_lower.as_str() {
            "and" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                is_truthy(&l) && is_truthy(&r)
            }
            "or" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                is_truthy(&l) || is_truthy(&r)
            }
            "is" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                l == r
            }
            "isnot" => {
                let l = evaluate_depth(&left, ctx, depth + 1);
                let r = evaluate_depth(&right, ctx, depth + 1);
                l != r
            }
            ">" => {
                let l: f64 = evaluate_depth(&left, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                let r: f64 = evaluate_depth(&right, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                l > r
            }
            "<" => {
                let l: f64 = evaluate_depth(&left, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                let r: f64 = evaluate_depth(&right, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                l < r
            }
            ">=" => {
                let l: f64 = evaluate_depth(&left, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                let r: f64 = evaluate_depth(&right, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                l >= r
            }
            "<=" => {
                let l: f64 = evaluate_depth(&left, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                let r: f64 = evaluate_depth(&right, ctx, depth + 1).parse().unwrap_or(f64::NAN);
                l <= r
            }
            "vis" => {
                let var_val = ctx.variables.get(&right).cloned().unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                var_val == l
            }
            "vnotis" | "visnot" => {
                let var_val = ctx.variables.get(&right).cloned().unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                var_val != l
            }
            "tis" => {
                let toggle_key = format!("toggle_{}", right);
                let toggle_val = ctx.toggles.get(&toggle_key)
                    .or_else(|| ctx.global_variables.get(&toggle_key))
                    .cloned()
                    .unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                toggle_val == l
            }
            "tnotis" | "tisnot" => {
                let toggle_key = format!("toggle_{}", right);
                let toggle_val = ctx.toggles.get(&toggle_key)
                    .or_else(|| ctx.global_variables.get(&toggle_key))
                    .cloned()
                    .unwrap_or_default();
                let l = evaluate_depth(&left, ctx, depth + 1);
                toggle_val != l
            }
            _ => {
                statement.push(left);
                statement.push(op);
                let r = evaluate_depth(&right, ctx, depth + 1);
                return is_truthy(&r);
            }
        };
        statement.push(if result { "1".to_string() } else { "0".to_string() });
    }

    if statement.len() == 2 {
        let right = statement.pop().expect("len == 2");
        let op = statement.pop().expect("len == 2");
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
                let toggle_key = format!("toggle_{}", right);
                let toggle_val = ctx.toggles.get(&toggle_key)
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
                return (&body[..true_end.saturating_sub(1)], Some(&body[false_start.min(body.len())..]));
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
            return arr.into_iter().map(|v| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            }).collect();
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

fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte < 0x80 { 1 }
    else if first_byte < 0xE0 { 2 }
    else if first_byte < 0xF0 { 3 }
    else { 4 }
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
                        output.push(ops.pop().expect("ops.last() was Some"));
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
                    output.push(ops.pop().expect("ops.last() was Some"));
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

    #[test]
    fn when_true() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#when 1}}yes{{/when}}", &mut ctx), "yes");
    }

    #[test]
    fn when_false() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#when 0}}yes{{/when}}", &mut ctx), "");
    }

    #[test]
    fn when_else() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#when 1}}yes{{:else}}no{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when 0}}yes{{:else}}no{{/when}}", &mut ctx), "no");
    }

    #[test]
    fn when_not() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#when::not::0}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::not::1}}yes{{/when}}", &mut ctx), "");
    }

    #[test]
    fn when_and_or() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#when::1::and::1}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::1::and::0}}yes{{/when}}", &mut ctx), "");
        assert_eq!(evaluate("{{#when::0::or::1}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::0::or::0}}yes{{/when}}", &mut ctx), "");
    }

    #[test]
    fn when_is_isnot() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#when::hello::is::hello}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::hello::isnot::world}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::hello::is::world}}yes{{/when}}", &mut ctx), "");
    }

    #[test]
    fn when_comparison() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#when::5::>::3}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::3::<::5}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::5::>=::5}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::3::<=::5}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::3::>::5}}yes{{/when}}", &mut ctx), "");
    }

    #[test]
    fn when_var() {
        let mut ctx = CbsContext::default();
        ctx.variables.insert("flag".into(), "1".into());
        assert_eq!(evaluate("{{#when::var::flag}}yes{{/when}}", &mut ctx), "yes");
        ctx.variables.insert("flag".into(), "0".into());
        assert_eq!(evaluate("{{#when::var::flag}}yes{{/when}}", &mut ctx), "");
    }

    #[test]
    fn when_vis_vnotis() {
        let mut ctx = CbsContext::default();
        ctx.variables.insert("color".into(), "red".into());
        assert_eq!(evaluate("{{#when::red::vis::color}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::blue::vis::color}}yes{{/when}}", &mut ctx), "");
        assert_eq!(evaluate("{{#when::blue::vnotis::color}}yes{{/when}}", &mut ctx), "yes");
    }

    #[test]
    fn when_toggle() {
        let mut ctx = CbsContext::default();
        ctx.toggles.insert("toggle_dark".into(), "1".into());
        assert_eq!(evaluate("{{#when::toggle::dark}}yes{{/when}}", &mut ctx), "yes");
    }

    #[test]
    fn when_tis_tnotis() {
        let mut ctx = CbsContext::default();
        ctx.toggles.insert("toggle_theme".into(), "dark".into());
        assert_eq!(evaluate("{{#when::dark::tis::theme}}yes{{/when}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#when::light::tnotis::theme}}yes{{/when}}", &mut ctx), "yes");
    }

    #[test]
    fn when_keep_whitespace() {
        let mut ctx = CbsContext::default();
        let input = "{{#when::keep::1}}\n  hello\n  world\n{{/when}}";
        assert_eq!(evaluate(input, &mut ctx), "\n  hello\n  world\n");
    }

    #[test]
    fn when_nested() {
        let mut ctx = CbsContext::default();
        let input = "{{#when 1}}outer{{#when 1}}inner{{/when}}{{/when}}";
        assert_eq!(evaluate(input, &mut ctx), "outerinner");
    }

    #[test]
    fn when_with_cbs_functions() {
        let mut ctx = CbsContext::default();
        ctx.char_name = "Alice".into();
        assert_eq!(
            evaluate("{{#when 1}}Hello {{char}}!{{/when}}", &mut ctx),
            "Hello Alice!"
        );
    }

    #[test]
    fn if_deprecated() {
        let mut ctx = CbsContext::default();
        assert_eq!(evaluate("{{#if 1}}yes{{/if}}", &mut ctx), "yes");
        assert_eq!(evaluate("{{#if 0}}yes{{/if}}", &mut ctx), "");
    }

    #[test]
    fn puredisplay() {
        let mut ctx = CbsContext::default();
        let result = evaluate("{{#puredisplay}}{{char}} hello{{/puredisplay}}", &mut ctx);
        assert_eq!(result, "\\{\\{char\\}\\} hello");
    }

    #[test]
    fn each_array() {
        let mut ctx = CbsContext::default();
        let input = r#"{{#each ["a","b","c"] as item}}[{{slot::item}}]{{/each}}"#;
        assert_eq!(evaluate(input, &mut ctx), "[a][b][c]");
    }

    #[test]
    fn each_separator() {
        let mut ctx = CbsContext::default();
        let input = "{{#each x\u{00A7}y\u{00A7}z as v}}({{slot::v}}){{/each}}";
        assert_eq!(evaluate(input, &mut ctx), "(x)(y)(z)");
    }

    #[test]
    fn when_else_multiline() {
        let mut ctx = CbsContext::default();
        let input = "{{#when 0}}\ntrue content\n{{:else}}\nfalse content\n{{/when}}";
        let result = evaluate(input, &mut ctx);
        assert_eq!(result, "false content");
    }
}
