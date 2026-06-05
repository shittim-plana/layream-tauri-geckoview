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
