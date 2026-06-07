//! Characterization tests for the CBS parser, written against the *current*
//! implementation before the LALRPOP conversion. They pin observable behavior
//! (§1.1) using only the public API, so they survive any internal refactor.

use layream_core::cbs::parser::{evaluate, CbsContext, CbsMessage};

fn ctx() -> CbsContext {
    CbsContext::default()
}

fn eval(input: &str) -> String {
    let mut c = ctx();
    evaluate(input, &mut c)
}

// ── Text passthrough / delimiter edge cases ────────────────────────────────

#[test]
fn empty_input() {
    assert_eq!(eval(""), "");
}

#[test]
fn public_api_struct_literal_shape() {
    // Mirrors the construction in layream-app commands.rs::evaluate_cbs, pinning
    // the public API surface (pub fields + Default + evaluate signature).
    use std::collections::HashMap;
    let mut toggles = HashMap::new();
    toggles.insert("dark".to_string(), "1".to_string());
    let mut c = CbsContext {
        char_name: "Alice".to_string(),
        user_name: "Bob".to_string(),
        toggles,
        ..Default::default()
    };
    assert_eq!(evaluate("{{char}} & {{user}}", &mut c), "Alice & Bob");
}

#[test]
fn plain_text_passthrough() {
    assert_eq!(eval("hello world"), "hello world");
}

#[test]
fn literal_double_colon_outside_tag() {
    assert_eq!(eval("foo::bar baz"), "foo::bar baz");
}

#[test]
fn single_braces_passthrough() {
    assert_eq!(eval("a{b}c {d"), "a{b}c {d");
}

#[test]
fn unclosed_tag_passthrough() {
    // No closing `}}` → left as-is.
    assert_eq!(eval("a {{char b"), "a {{char b");
}

// ── Nesting / arguments ────────────────────────────────────────────────────

#[test]
fn nested_tag_without_colons_works() {
    let mut c = ctx();
    c.char_name = "Alice".into();
    // A nested zero-arg tag (no `::`) survives the naive `::` split.
    assert_eq!(evaluate("{{upper::{{char}}}}", &mut c), "ALICE");
}

#[test]
fn nested_tag_with_colons_is_split_quirk() {
    // CBS is a string-rewriting macro: the tag body is split on `::` *before*
    // inner evaluation, so a nested tag that itself contains `::` is torn apart.
    // `upper::{{lower::ABC}}` → ["upper", "{{lower", "ABC}}"] → upper("{{lower").
    assert_eq!(eval("{{upper::{{lower::ABC}}}}"), "{{LOWER");
}

#[test]
fn replace_with_spaces_in_args() {
    assert_eq!(eval("{{replace::a b c::b::X}}"), "a X c");
}

#[test]
fn unknown_tag_no_args_roundtrip() {
    assert_eq!(eval("{{unknownfunc}}"), "{{unknownfunc}}");
}

#[test]
fn unknown_tag_with_args_roundtrip() {
    assert_eq!(eval("{{foo::a::b}}"), "{{foo::a::b}}");
}

#[test]
fn unknown_tag_args_are_evaluated() {
    let mut c = ctx();
    c.char_name = "Alice".into();
    // The argument {{char}} is expanded before the unknown tag round-trips.
    assert_eq!(evaluate("{{foo::{{char}}}}", &mut c), "{{foo::Alice}}");
}

#[test]
fn unknown_tag_name_is_normalized_in_output() {
    // Current behavior: the reconstructed name is lower-cased / separators stripped.
    assert_eq!(eval("{{UnknownFunc}}"), "{{unknownfunc}}");
}

// ── Function name normalization ────────────────────────────────────────────

#[test]
fn func_name_normalization() {
    let mut c = ctx();
    evaluate("{{set_var::k::v}}", &mut c);
    assert_eq!(evaluate("{{getVar::k}}", &mut c), "v");
    assert_eq!(evaluate("{{GET-VAR::k}}", &mut c), "v");
    assert_eq!(evaluate("{{get var::k}}", &mut c), "v");
}

// ── Math ───────────────────────────────────────────────────────────────────

#[test]
fn math_power_right_associative() {
    assert_eq!(eval("{{? 2 ^ 3 ^ 2}}"), "512");
}

#[test]
fn math_modulo() {
    assert_eq!(eval("{{calc::10 % 3}}"), "1");
}

#[test]
fn math_unary_in_prefix_and_after_op() {
    assert_eq!(eval("{{? -2 + 3}}"), "1");
    assert_eq!(eval("{{calc::2 * -3}}"), "-6");
}

#[test]
fn math_float_division() {
    assert_eq!(eval("{{? 10 / 4}}"), "2.5");
}

#[test]
fn math_empty_is_nan() {
    assert_eq!(eval("{{calc::}}"), "NaN");
}

#[test]
fn math_pure_garbage_is_nan() {
    assert_eq!(eval("{{calc::abc}}"), "NaN");
}

#[test]
fn math_nested_dynamic_is_nan_quirk() {
    // Same `::` split quirk: `calc::{{getvar::n}} + 1` splits before evaluation,
    // so calc receives the literal "{{getvar" → not a number → NaN.
    let mut c = ctx();
    evaluate("{{setvar::n::5}}", &mut c);
    assert_eq!(evaluate("{{calc::{{getvar::n}} + 1}}", &mut c), "NaN");
}

// ── Math helper functions ──────────────────────────────────────────────────

#[test]
fn math_functions() {
    assert_eq!(eval("{{round::3.6}}"), "4");
    assert_eq!(eval("{{floor::3.6}}"), "3");
    assert_eq!(eval("{{ceil::3.2}}"), "4");
    assert_eq!(eval("{{abs::-5}}"), "5");
    assert_eq!(eval("{{pow::2::10}}"), "1024");
    assert_eq!(eval("{{min::3::1::2}}"), "1");
    assert_eq!(eval("{{max::3::1::2}}"), "3");
    assert_eq!(eval("{{remaind::10::3}}"), "1");
    assert_eq!(eval("{{fixnum::3.14159::2}}"), "3.14");
}

#[test]
fn randint_min_ge_max_returns_min() {
    assert_eq!(eval("{{randint::5::5}}"), "5");
    assert_eq!(eval("{{randint::9::3}}"), "9");
}

// ── String functions ───────────────────────────────────────────────────────

#[test]
fn string_functions_extra() {
    assert_eq!(eval("{{trim::  hi  }}"), "hi");
    assert_eq!(eval("{{contains::hello::ell}}"), "1");
    assert_eq!(eval("{{contains::hello::xyz}}"), "0");
    assert_eq!(eval("{{startswith::hello::he}}"), "1");
    assert_eq!(eval("{{endswith::hello::lo}}"), "1");
    assert_eq!(eval("{{reverse::abc}}"), "cba");
    assert_eq!(eval("{{tonumber::a1b2.5}}"), "12.5");
}

// ── Logic functions ────────────────────────────────────────────────────────

#[test]
fn logic_functions_extra() {
    assert_eq!(eval("{{notequal::a::b}}"), "1");
    assert_eq!(eval("{{greaterequal::5::5}}"), "1");
    assert_eq!(eval("{{lessequal::3::5}}"), "1");
    assert_eq!(eval("{{greater::5::3}}"), "1");
    assert_eq!(eval("{{less::5::3}}"), "0");
}

// ── Escapes / formatting ───────────────────────────────────────────────────

#[test]
fn escape_functions() {
    assert_eq!(eval("{{cbr}}"), "\\n");
    assert_eq!(eval("{{decbo}}"), "{");
    assert_eq!(eval("{{decbc}}"), "}");
    assert_eq!(eval("{{none}}"), "");
    assert_eq!(eval("{{blank}}"), "");
}

// ── Variables ──────────────────────────────────────────────────────────────

#[test]
fn setdefaultvar_only_sets_when_absent() {
    let mut c = ctx();
    evaluate("{{setvar::k::first}}", &mut c);
    evaluate("{{setdefaultvar::k::second}}", &mut c);
    assert_eq!(evaluate("{{getvar::k}}", &mut c), "first");

    let mut c2 = ctx();
    evaluate("{{setdefaultvar::j::deflt}}", &mut c2);
    assert_eq!(evaluate("{{getvar::j}}", &mut c2), "deflt");
}

// ── Comments ───────────────────────────────────────────────────────────────

#[test]
fn comment_standalone() {
    assert_eq!(eval("{{// just a note}}"), "");
}

// ── Blocks: if / if_pure ───────────────────────────────────────────────────

#[test]
fn if_pure_block() {
    let mut c = ctx();
    c.char_name = "Alice".into();
    assert_eq!(evaluate("{{#if_pure 1}}{{char}}{{/if_pure}}", &mut c), "Alice");
    assert_eq!(evaluate("{{#if_pure 0}}{{char}}{{/if_pure}}", &mut c), "");
}

// ── Blocks: when (left-to-right fold, not precedence) ──────────────────────

#[test]
fn when_left_fold_not_precedence() {
    // Left fold: ((1 or 0) and 0) == 0 → empty.
    // Precedence would give 1 or (0 and 0) == 1 → "yes".
    assert_eq!(eval("{{#when::1::or::0::and::0}}yes{{/when}}"), "");
}

#[test]
fn when_legacy_flag() {
    assert_eq!(eval("{{#when::legacy::1}}a{{:else}}b{{/when}}"), "a");
    assert_eq!(eval("{{#when::legacy::0}}a{{:else}}b{{/when}}"), "b");
}

// ── Blocks: each ───────────────────────────────────────────────────────────

#[test]
fn each_default_slot_name() {
    assert_eq!(eval(r#"{{#each ["x","y"]}}<{{slot::slot}}>{{/each}}"#), "<x><y>");
}

#[test]
fn each_json_numbers() {
    assert_eq!(eval("{{#each [1,2,3] as n}}{{slot::n}}{{/each}}"), "123");
}

#[test]
fn each_slot_used_twice() {
    assert_eq!(eval(r#"{{#each ["a"] as v}}{{slot::v}}{{slot::v}}{{/each}}"#), "aa");
}

// ── Blocks: puredisplay ────────────────────────────────────────────────────

#[test]
fn pure_escapes_nested_braces() {
    assert_eq!(eval("{{#pure}}a{{b}}c{{/pure}}"), "a\\{\\{b\\}\\}c");
}

// ── Context-backed functions ───────────────────────────────────────────────

#[test]
fn history_and_last_message() {
    let mut c = ctx();
    c.messages = vec![
        CbsMessage { role: "user".into(), data: "hi".into(), time: None },
        CbsMessage { role: "assistant".into(), data: "yo".into(), time: None },
    ];
    assert_eq!(evaluate("{{history}}", &mut c), "user: hi\nassistant: yo");
    assert_eq!(evaluate("{{lastmessage}}", &mut c), "yo");
    assert_eq!(evaluate("{{lastmessageid}}", &mut c), "1");
}

// ── Group B: message functions ─────────────────────────────────────────────
//
// Real-world presets call these with snake_case names (`message_date`,
// `idle_duration`, …); these tests exercise the snake_case surface to pin that
// `normalize_func_name` resolves them.

fn populated_ctx() -> CbsContext {
    let mut c = ctx();
    c.messages = vec![
        CbsMessage { role: "user".into(), data: "hello".into(), time: Some(1_700_000_000) },
        CbsMessage { role: "assistant".into(), data: "hi there".into(), time: Some(1_700_000_500) },
        CbsMessage { role: "user".into(), data: "how are you".into(), time: None },
    ];
    c
}

#[test]
fn message_date_snake_case_last_message() {
    let mut c = populated_ctx();
    // Last message has no time → graceful sentinel; index arg selects a timed one.
    assert_eq!(evaluate("{{message_date}}", &mut c), "[Cannot get time]");
    assert_eq!(evaluate("{{message_date::0}}", &mut c), "2023-11-14");
}

#[test]
fn message_time_snake_case_indexed() {
    let mut c = populated_ctx();
    assert_eq!(evaluate("{{message_time::0}}", &mut c), "22:13:20");
}

#[test]
fn message_date_empty_messages_graceful() {
    let mut c = ctx();
    assert_eq!(evaluate("{{message_date}}", &mut c), "[Cannot get time]");
    assert_eq!(evaluate("{{message_time}}", &mut c), "[Cannot get time]");
}

#[test]
fn idle_duration_snake_case_stub() {
    // Faithful to the archive: idle duration is the HH:MM:SS zero stub until real
    // timing is wired in. Holds with or without messages.
    let mut c = populated_ctx();
    assert_eq!(evaluate("{{idle_duration}}", &mut c), "00:00:00");
    let mut empty = ctx();
    assert_eq!(evaluate("{{idle_duration}}", &mut empty), "00:00:00");
}

#[test]
fn previous_char_and_user_chat() {
    let mut c = populated_ctx();
    // Last non-user message, and last user message, respectively.
    assert_eq!(evaluate("{{previouscharchat}}", &mut c), "hi there");
    assert_eq!(evaluate("{{previoususerchat}}", &mut c), "how are you");
}

#[test]
fn previous_char_and_user_chat_empty_graceful() {
    let mut c = ctx();
    assert_eq!(evaluate("{{previouscharchat}}", &mut c), "");
    assert_eq!(evaluate("{{previoususerchat}}", &mut c), "");
}

#[test]
fn chat_index_and_first_message() {
    let mut c = ctx();
    assert_eq!(evaluate("{{isfirstmsg}}", &mut c), "1");
    c.chat_index = 5;
    assert_eq!(evaluate("{{isfirstmsg}}", &mut c), "0");
    assert_eq!(evaluate("{{chatindex}}", &mut c), "5");
}

// ── Baseline tests (ported from draft/backend-cbs-opus47) ─────────────────
//
// Pin subtle implementation behaviors. If a rewrite breaks these, fix the
// rewrite -- not the assertions.

#[test]
fn baseline_if_legacy_whitespace() {
    let mut c = ctx();
    let input = "{{#if 1}}\n  body line\n{{/if}}";
    assert_eq!(evaluate(input, &mut c), "  body line");
}

#[test]
fn baseline_each_references_outer_context() {
    let mut c = ctx();
    c.char_name = "Alice".into();
    let input = r#"{{#each ["1","2"] as i}}[{{char}}-{{slot::i}}]{{/each}}"#;
    assert_eq!(evaluate(input, &mut c), "[Alice-1][Alice-2]");
}

#[test]
fn baseline_comment_with_nested_braces() {
    let mut c = ctx();
    let input = "a{{// has {{stuff}} inside}}b";
    assert_eq!(evaluate(input, &mut c), "ab");
}

#[test]
fn baseline_puredisplay_body_not_evaluated() {
    let mut c = ctx();
    c.char_name = "Alice".into();
    let r = evaluate("{{#pure}}{{char}}{{/pure}}", &mut c);
    assert_eq!(r, "\\{\\{char\\}\\}");
}

#[test]
fn baseline_addvar_numeric_accumulation() {
    let mut c = ctx();
    evaluate("{{setvar::c::1.5}}", &mut c);
    evaluate("{{addvar::c::2.5}}", &mut c);
    assert_eq!(evaluate("{{getvar::c}}", &mut c), "4");
    evaluate("{{addvar::new::3}}", &mut c);
    assert_eq!(evaluate("{{getvar::new}}", &mut c), "3");
}

#[test]
fn baseline_tempvar_isolated_from_var() {
    let mut c = ctx();
    evaluate("{{setvar::k::regular}}", &mut c);
    evaluate("{{settempvar::k::temp}}", &mut c);
    assert_eq!(evaluate("{{getvar::k}}", &mut c), "regular");
    assert_eq!(evaluate("{{tempvar::k}}", &mut c), "temp");
}

#[test]
fn baseline_block_nesting_across_kinds() {
    let mut c = ctx();
    let input = "{{#if 1}}A{{#when 1}}B{{#each [\"x\",\"y\"] as v}}({{slot::v}}){{/each}}C{{/when}}D{{/if}}";
    assert_eq!(evaluate(input, &mut c), "AB(x)(y)CD");
}

#[test]
fn baseline_tonumber_filters() {
    assert_eq!(eval("{{tonumber::abc-12.3xyz}}"), "-12.3");
    assert_eq!(eval("{{tonumber::price: $4.99}}"), "4.99");
}

#[test]
fn baseline_calc_inside_calc_breaks_on_naive_split() {
    let mut c = ctx();
    // Naive `::` split shreds the inner `calc::2+3` at the outer level:
    //   tag = "calc::{{calc::2+3}}+1" -> ["calc", "{{calc", "2+3}}+1"]
    //   first arg -> "{{calc", eval_math("{{calc") = NaN
    assert_eq!(evaluate("{{calc::{{calc::2+3}}+1}}", &mut c), "NaN");
}

#[test]
fn baseline_nested_tag_without_colon_works() {
    // A nested tag with NO `::` survives because naive split sees one part.
    assert_eq!(eval("{{upper::{{br}}}}"), "\n");
}

#[test]
fn baseline_escape_aliases() {
    assert_eq!(eval("{{ddecbo}}"), "{{");
    assert_eq!(eval("{{ddecbc}}"), "}}");
    assert_eq!(eval("{{decbo}}"), "{");
    assert_eq!(eval("{{decbc}}"), "}");
    assert_eq!(eval("{{newline}}"), "\n");
    assert_eq!(eval("{{cnl}}"), "\\n");
}

#[test]
fn baseline_unterminated_tag_passthrough() {
    assert_eq!(eval("{{unclosed"), "{{unclosed");
    assert_eq!(eval("close}}only"), "close}}only");
}

#[test]
fn baseline_when_else_inline_vs_line() {
    assert_eq!(eval("{{#when 0}}T{{:else}}F{{/when}}"), "F");
    let input = "{{#when 0}}\nT\n{{:else}}\nF\n{{/when}}";
    assert_eq!(eval(input), "F");
}

#[test]
fn baseline_math_division_by_zero() {
    assert_eq!(eval("{{? 1 / 0}}"), "inf");
    assert_eq!(eval("{{? 0 / 0}}"), "NaN");
}

#[test]
fn baseline_math_integer_format() {
    assert_eq!(eval("{{? 4 / 2}}"), "2");
    assert_eq!(eval("{{? 10 - 3}}"), "7");
    assert_eq!(eval("{{calc::6 * 7}}"), "42");
}

#[test]
fn baseline_math_right_assoc_pow_extended() {
    assert_eq!(eval("{{? -2 ^ 2}}"), "4");
    assert_eq!(eval("{{? 2 ^ -3}}"), "0.125");
}

#[test]
fn baseline_math_precedence_and_paren() {
    assert_eq!(eval("{{? 2 + 3 * 4}}"), "14");
    assert_eq!(eval("{{? (2 + 3) * 4}}"), "20");
    assert_eq!(eval("{{? 10 % 3}}"), "1");
    assert_eq!(eval("{{? 5 + -3}}"), "2");
}

#[test]
fn baseline_unknown_echo_uses_normalized_name() {
    assert_eq!(eval("{{X-Y-Z}}"), "{{xyz}}");
    assert_eq!(eval("{{Unk::a::b}}"), "{{unk::a::b}}");
}

#[test]
fn baseline_math_skips_unknown_chars() {
    assert_eq!(eval("{{? abc}}"), "NaN");
    assert_eq!(eval("{{? xyz + 1}}"), "NaN");
}

// ── Group A: arrays ─────────────────────────────────────────────────────────

#[test]
fn array_make_and_length() {
    assert_eq!(eval("{{makearray::a::b::c}}"), r#"["a","b","c"]"#);
    assert_eq!(eval("{{array::x::y}}"), r#"["x","y"]"#);
    assert_eq!(eval(r#"{{arraylength::["a","b","c"]}}"#), "3");
    // Non-JSON input falls back to the § separator.
    assert_eq!(eval("{{arraylength::a\u{00A7}b}}"), "2");
}

#[test]
fn array_element_indexing() {
    assert_eq!(eval(r#"{{arrayelement::["a","b","c"]::1}}"#), "b");
    // Array.at() negative index counts from the end.
    assert_eq!(eval(r#"{{arrayelement::["a","b","c"]::-1}}"#), "c");
    // Out of bounds → "null", not a panic.
    assert_eq!(eval(r#"{{arrayelement::["a"]::5}}"#), "null");
}

#[test]
fn array_push_pop_shift() {
    assert_eq!(eval(r#"{{arraypush::["a","b"]::c}}"#), r#"["a","b","c"]"#);
    assert_eq!(eval(r#"{{arraypop::["a","b","c"]}}"#), r#"["a","b"]"#);
    assert_eq!(eval(r#"{{arrayshift::["a","b","c"]}}"#), r#"["b","c"]"#);
    // Pop/shift on empty array is a no-op, not a panic.
    assert_eq!(eval("{{arraypop::[]}}"), "[]");
    assert_eq!(eval("{{arrayshift::[]}}"), "[]");
}

#[test]
fn array_splice_and_assert() {
    assert_eq!(eval(r#"{{arraysplice::["a","b","c"]::1::1::x}}"#), r#"["a","x","c"]"#);
    // arrayassert only writes when the index is currently out of bounds.
    assert_eq!(eval(r#"{{arrayassert::["a"]::2::z}}"#), r#"["a","","z"]"#);
    assert_eq!(eval(r#"{{arrayassert::["a","b"]::0::z}}"#), r#"["a","b"]"#);
}

#[test]
fn array_filter_modes() {
    assert_eq!(eval(r#"{{filter::["a","","a"]::unique}}"#), r#"["a",""]"#);
    assert_eq!(eval(r#"{{filter::["a","","a"]::nonempty}}"#), r#"["a","a"]"#);
    // default "all" removes both empties and duplicates.
    assert_eq!(eval(r#"{{filter::["a","","a"]}}"#), r#"["a"]"#);
}

#[test]
fn array_spread_split_join() {
    assert_eq!(eval(r#"{{spread::["a","b","c"]}}"#), "a::b::c");
    assert_eq!(eval("{{split::a,b,c::,}}"), r#"["a","b","c"]"#);
    // Tag arguments are trimmed before use (established CBS behavior), so the
    // separator here is "," not ", ".
    assert_eq!(eval(r#"{{join::["a","b"]::,}}"#), "a,b");
}

// ── Group A: dictionaries ───────────────────────────────────────────────────

#[test]
fn dict_make_and_element() {
    // Keys are sorted in serde_json's object output; both keys must be present.
    let d = eval("{{makedict::name=John::age=25}}");
    assert!(d.contains(r#""name":"John""#));
    assert!(d.contains(r#""age":"25""#));
    assert_eq!(eval(r#"{{dictelement::{"name":"John"}::name}}"#), "John");
    // Missing key → "null".
    assert_eq!(eval(r#"{{dictelement::{"a":"1"}::z}}"#), "null");
}

#[test]
fn dict_object_assert() {
    // Only sets when key is absent.
    assert_eq!(eval(r#"{{objectassert::{"a":"1"}::b::2}}"#), r#"{"a":"1","b":"2"}"#);
    assert_eq!(eval(r#"{{objectassert::{"a":"1"}::a::9}}"#), r#"{"a":"1"}"#);
}

#[test]
fn dict_element_single_level() {
    // Single-level traversal works; multi-level inline JSON hits the documented
    // `::` split quirk, so it is not exercised here.
    assert_eq!(eval(r#"{{element::{"name":"John"}::name}}"#), "John");
    assert_eq!(eval(r#"{{element::["x","y"]::1}}"#), "y");
    assert_eq!(eval(r#"{{element::{"a":"1"}::missing}}"#), "null");
}

// ── Group A: booleans / aggregates ──────────────────────────────────────────

#[test]
fn boolean_all_any() {
    assert_eq!(eval("{{all::1::1::1}}"), "1");
    assert_eq!(eval("{{all::1::0::1}}"), "0");
    assert_eq!(eval("{{any::0::1::0}}"), "1");
    assert_eq!(eval("{{any::0::0::0}}"), "0");
    // Array-argument form.
    assert_eq!(eval(r#"{{all::["1","1"]}}"#), "1");
    assert_eq!(eval(r#"{{any::["0","0"]}}"#), "0");
}

#[test]
fn aggregate_sum_average_range() {
    assert_eq!(eval("{{sum::1::2::3}}"), "6");
    assert_eq!(eval("{{average::2::4::6}}"), "4");
    // Non-numeric operands count as 0.
    assert_eq!(eval("{{sum::1::x::2}}"), "3");
    assert_eq!(eval("{{range::[5]}}"), r#"["0","1","2","3","4"]"#);
    assert_eq!(eval("{{range::[2,8,2]}}"), r#"["2","4","6"]"#);
    assert_eq!(eval("{{range::[5,0,-1]}}"), r#"["5","4","3","2","1"]"#);
}

// ── Group A: time (UTC, explicit timestamp) ─────────────────────────────────

#[test]
fn time_format_explicit_timestamp() {
    // 1640995200000 ms = 2022-01-01T00:00:00Z.
    assert_eq!(eval("{{date::YYYY-MM-DD::1640995200000}}"), "2022-01-01");
    assert_eq!(eval("{{time::HH:mm:ss::1640995200000}}"), "00:00:00");
    // Empty format string → empty output (date-format guard).
    assert_eq!(eval("{{date::::1640995200000}}"), "");
}

// ── Group A: escape-output funcs emit ACTUAL chars ─────────────────────────

#[test]
fn escape_output_funcs_actual_chars() {
    assert_eq!(eval("{{debo}}"), "(");
    assert_eq!(eval("{{debc}}"), ")");
    assert_eq!(eval("{{deabo}}"), "<");
    assert_eq!(eval("{{deabc}}"), ">");
    assert_eq!(eval("{{displayescapedsemicolon}}"), ";");
    // Existing brace/colon escapes keep their prior behavior.
    assert_eq!(eval("{{bo}}"), "{{");
    assert_eq!(eval("{{decbo}}"), "{");
    assert_eq!(eval("{{dec}}"), ":");
}

// ── Group A: crypto / encoding ──────────────────────────────────────────────

#[test]
fn crypto_xor_roundtrip() {
    let enc = eval("{{xor::hello}}");
    assert_eq!(enc, "l5qTk5A=");
    let mut c = ctx();
    let dec = evaluate(&format!("{{{{xordecrypt::{}}}}}", enc), &mut c);
    assert_eq!(dec, "hello");
    // Invalid base64 → empty, not a panic.
    assert_eq!(eval("{{xordecrypt::!!!notbase64!!!}}"), "");
}

#[test]
fn crypto_crypt_roundtrip() {
    // Default shift 32768 is self-inverse (mod 65536), so crypt∘crypt = identity.
    let enc = eval("{{crypt::hello}}");
    assert_ne!(enc, "hello");
    let mut c = ctx();
    let dec = evaluate(&format!("{{{{crypt::{}}}}}", enc), &mut c);
    assert_eq!(dec, "hello");
}

#[test]
fn encoding_unicode_and_hex() {
    assert_eq!(eval("{{unicodeencode::A}}"), "65");
    assert_eq!(eval("{{unicodedecode::65}}"), "A");
    assert_eq!(eval("{{u::41}}"), "A");
    assert_eq!(eval("{{ue::41}}"), "A");
    assert_eq!(eval("{{fromhex::FF}}"), "255");
    assert_eq!(eval("{{tohex::255}}"), "ff");
    // Invalid hex → NaN (rejection side, §2.3).
    assert_eq!(eval("{{fromhex::zz}}"), "NaN");
}

// ── Group A: hash / deterministic random ────────────────────────────────────

#[test]
fn hash_is_deterministic_and_7_digits() {
    let a = eval("{{hash::hello}}");
    let b = eval("{{hash::hello}}");
    assert_eq!(a, b);
    assert_eq!(a.len(), 7);
    assert!(a.chars().all(|c| c.is_ascii_digit()));
    // Different input → different hash (witness, not exhaustive).
    assert_ne!(eval("{{hash::world}}"), a);
}

#[test]
fn roll_dice_bounds_and_errors() {
    // 1d1 is deterministic: always 1.
    assert_eq!(eval("{{roll::1d1}}"), "1");
    assert_eq!(eval("{{roll}}"), "1");
    // Non-numeric notation → NaN (rejection side).
    assert_eq!(eval("{{roll::abc}}"), "NaN");
    // In-range for a real die.
    let r: i64 = eval("{{roll::6}}").parse().unwrap();
    assert!((1..=6).contains(&r));
}

#[test]
fn rollp_is_deterministic_for_fixed_seed() {
    let mut c1 = ctx();
    c1.hash_seed = "seed".into();
    let mut c2 = ctx();
    c2.hash_seed = "seed".into();
    assert_eq!(evaluate("{{rollp::1d20}}", &mut c1), evaluate("{{rollp::1d20}}", &mut c2));
}

// ── Group B: context-backed status ──────────────────────────────────────────

#[test]
fn status_role_jbtoggled_maxcontext() {
    let mut c = ctx();
    c.role = "assistant".into();
    c.jb_toggled = true;
    c.max_context = 8192;
    assert_eq!(evaluate("{{role}}", &mut c), "assistant");
    assert_eq!(evaluate("{{jbtoggled}}", &mut c), "1");
    assert_eq!(evaluate("{{maxcontext}}", &mut c), "8192");

    let mut d = ctx();
    assert_eq!(evaluate("{{jbtoggled}}", &mut d), "0");
}

// ── Group C: justified-missing stubs are empty (not raw passthrough) ─────────

#[test]
fn stub_functions_are_empty() {
    assert_eq!(eval("{{metadata::version}}"), "");
    assert_eq!(eval("{{assetlist}}"), "");
    assert_eq!(eval("{{screenwidth}}"), "");
    assert_eq!(eval("{{moduleenabled::x}}"), "");
}

// ── Blocks: #escape (literal interior — the escape-region fix) ──────────────

#[test]
fn escape_block_keeps_interior_literal() {
    // The {{/if_pure}} and {{/0}} inside #escape must survive as literal text,
    // not be parsed as block closes. This is the core preset-breakage fix.
    assert_eq!(
        eval("{{#escape}}{{/if_pure}}{{/0}}literal{{/escape}}"),
        "{{/if_pure}}{{/0}}literal"
    );
}

#[test]
fn escape_block_does_not_evaluate_interior() {
    let mut c = ctx();
    c.char_name = "Alice".into();
    // {{char}} inside escape is NOT expanded.
    assert_eq!(evaluate("{{#escape}}{{char}}{{/escape}}", &mut c), "{{char}}");
    // Mode argument is accepted; interior still literal.
    assert_eq!(evaluate("{{#escape::mode}}{{char}}{{/escape}}", &mut c), "{{char}}");
}

#[test]
fn escape_block_llama_template() {
    // The witnessed use case: emit llama chat-template control tokens.
    let input = "{{#escape}}<|start_header_id|>{{bos_token}}<|end_header_id|>{{/escape}}";
    assert_eq!(
        eval(input),
        "<|start_header_id|>{{bos_token}}<|end_header_id|>"
    );
}

// ── Blocks: #code ───────────────────────────────────────────────────────────

#[test]
fn code_block_strips_newlines_tabs_then_evaluates() {
    let mut c = ctx();
    let input = "{{#code}}\n\t{{setvar::x::5}}\n\t{{getvar::x}}\n{{/code}}";
    assert_eq!(evaluate(input, &mut c), "5");
}

// ── Blocks: #func + call:: ──────────────────────────────────────────────────

#[test]
fn func_define_and_call() {
    let mut c = ctx();
    let input = "{{#func greet::name}}Hi {{getvar::name}}{{/func}}{{call::greet::Bob}}";
    assert_eq!(evaluate(input, &mut c), "Hi Bob");
}

#[test]
fn func_call_unknown_is_empty() {
    assert_eq!(eval("{{call::nonexistent::x}}"), "");
}

#[test]
fn func_call_restores_variable_scope() {
    let mut c = ctx();
    // An outer `name` var must be restored after the call's local binding.
    let input = "{{setvar::name::outer}}{{#func f::name}}{{getvar::name}}{{/func}}{{call::f::inner}}-{{getvar::name}}";
    assert_eq!(evaluate(input, &mut c), "inner-outer");
}
