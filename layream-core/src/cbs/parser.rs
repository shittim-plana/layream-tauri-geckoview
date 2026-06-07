//! CBS template parsing and the public evaluation entry point.
//!
//! Parsing produces a shallow [`Node`] tree. The `{{ }}` boundary scan itself
//! lives in [`crate::cbs::tokenizer`] — the single source of truth shared with
//! the highlighter (§4.1: one fact, one site). [`parse`] is now a thin adapter
//! from [`tokenizer::Segment`] to [`Node`]: tag arguments and block bodies stay
//! raw strings, re-evaluated during evaluation to preserve CBS's string-rewriting
//! macro semantics. The math sub-language is parsed by the LALRPOP grammar in
//! `grammar.lalrpop`.

use crate::cbs::ast::Node;
use crate::cbs::tokenizer::{self, Segment};

pub use crate::cbs::eval::{CbsContext, CbsMessage};

pub(crate) const MAX_DEPTH: usize = 20;

/// Evaluate a CBS template string against the given context.
pub fn evaluate(input: &str, ctx: &mut CbsContext) -> String {
    evaluate_depth(input, ctx, 0)
}

/// Evaluate at a given recursion depth. Beyond [`MAX_DEPTH`] the input is
/// returned verbatim, bounding macro re-evaluation.
pub(crate) fn evaluate_depth(input: &str, ctx: &mut CbsContext, depth: usize) -> String {
    if depth > MAX_DEPTH {
        return input.to_string();
    }
    let nodes = parse(input);
    crate::cbs::eval::eval_nodes(&nodes, ctx, depth)
}

/// Parse a template into its shallow node structure by adapting the shared
/// tokenizer's [`Segment`] stream into [`Node`]s.
///
/// - [`Segment::Text`] → [`Node::Text`] (verbatim; includes escape-region
///   interiors and unclosed-delimiter fallbacks).
/// - [`Segment::Comment`] → dropped (comments produce no output).
/// - [`Segment::Tag`] → [`Node::Tag`] with the trimmed content.
/// - [`Segment::Block`] for `escape` → [`Node::Text`] of the verbatim body: the
///   escape interior is literal and must never be re-evaluated as CBS. This is
///   the single point where escape's literal contract is realized for eval.
/// - any other [`Segment::Block`] → [`Node::Block`] for [`crate::cbs::eval`].
fn parse(input: &str) -> Vec<Node> {
    tokenizer::scan(input)
        .into_iter()
        .filter_map(segment_to_node)
        .collect()
}

fn segment_to_node(seg: Segment) -> Option<Node> {
    match seg {
        Segment::Text(t) => Some(Node::Text(t)),
        Segment::Comment(_) => None,
        Segment::Tag { content, .. } => Some(Node::Tag(content)),
        Segment::Block { name, body, header } => {
            if name == "escape" {
                Some(Node::Text(body))
            } else {
                Some(Node::Block { header, body })
            }
        }
    }
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
        ctx.toggles.insert("dark".into(), "1".into());
        assert_eq!(evaluate("{{#when::toggle::dark}}yes{{/when}}", &mut ctx), "yes");
    }

    #[test]
    fn when_tis_tnotis() {
        let mut ctx = CbsContext::default();
        ctx.toggles.insert("theme".into(), "dark".into());
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
