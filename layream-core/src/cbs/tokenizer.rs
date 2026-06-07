//! Shared `{{ }}` tokenizer for the CBS template language.
//!
//! This is the single source of truth for where `{{ }}` boundaries are — both
//! [`crate::cbs::parser`] (evaluation) and [`crate::cbs::highlighter`] (editor
//! coloring) consume the [`Segment`] vector produced here, eliminating the
//! historical dual-scan where each module located delimiters independently and
//! could disagree.
//!
//! ## Why logos *plus* a grouping pass
//!
//! CBS tags nest: `{{upper::{{char}}}}`. Balanced/nested `{{ }}` is not a
//! regular language (pumping lemma), so a regex lexer cannot pair delimiters on
//! its own. The division of labor is a logos lexer plus a structural grouping
//! pass:
//!
//! - **logos** ([`RawToken`]) does the byte-level scan: it emits a flat stream
//!   of `{{`, `}}`, and literal text — no hand-written byte loop.
//! - **[`scan`]** is a thin structural pass over that flat stream: a depth
//!   counter pairs delimiters into [`Segment`]s. Pairing is parsing, not
//!   lexing, and is necessarily non-regex.
//!
//! ## Escape regions are literal
//!
//! When [`scan`] meets a `{{#escape}}` open, everything up to the matching
//! `{{/escape}}` is captured as a single [`Segment::Text`] — the interior is
//! never interpreted as CBS. This is what lets a preset escape llama chat
//! templates (`{{`/`}}` literals, stray `{{/0}}`…`{{/57}}`, `{{/if_pure}}`)
//! without the block matcher tripping over them.
//!
//! ## Incomplete input falls back to text
//!
//! An unpaired `{{` (no matching `}}`) is emitted as literal [`Segment::Text`]
//! rather than an error, so the highlighter keeps working while the user is
//! mid-keystroke and a tag is not yet closed.

use logos::Logos;

/// Flat tokens produced by the logos lexer. Longest-match makes `{{`/`}}` win
/// over the single-brace and text rules.
#[derive(Logos, Debug, Clone, PartialEq)]
enum RawToken {
    #[token("{{")]
    Open,
    #[token("}}")]
    Close,
    /// A run of characters containing no brace — the common case.
    #[regex(r"[^{}]+")]
    Text,
    /// A single `{` or `}` that is not part of a `{{`/`}}` delimiter. Literal.
    #[token("{")]
    #[token("}")]
    LoneBrace,
}

/// A structural segment of a CBS template.
///
/// Tag arguments and block bodies are kept as raw strings (CBS is a
/// string-rewriting macro language; `::` splitting and `{{slot}}` substitution
/// happen on raw text during evaluation, not on a parsed tree).
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Literal text outside `{{ }}` — including escape-region interiors and
    /// unclosed-delimiter fallbacks.
    Text(String),
    /// A `{{ ... }}` tag. `content` is the trimmed inner text; `raw` is the
    /// exact inner text (untrimmed), needed to reconstruct unknown tags.
    Tag { content: String, raw: String },
    /// A comment `{{// ...}}`.
    Comment(String),
    /// A block: `{{#name ...}} body {{/name}}` (or `{{:each ...}}`).
    Block {
        /// Trimmed opening-tag content, e.g. `#when::1::and::1`.
        header: String,
        /// Lower-cased block name, e.g. `when`, `each`, `escape`.
        name: String,
        /// Body between open and close, left unparsed. For `#escape` this is the
        /// verbatim interior (never re-scanned as CBS).
        body: String,
    },
}

/// Byte span of a flat token within the source, plus its kind.
struct Spanned {
    kind: RawToken,
    start: usize,
    end: usize,
}

/// Tokenize `source` into the flat logos stream, with absolute byte spans.
///
/// Any character that logos fails to classify is treated as literal text — the
/// lexer never errors out, matching the "passthrough on anything unexpected"
/// contract the template layer relies on.
fn lex(source: &str) -> Vec<Spanned> {
    let mut out = Vec::new();
    let mut lexer = RawToken::lexer(source);
    while let Some(result) = lexer.next() {
        let span = lexer.span();
        let kind = match result {
            Ok(tok) => tok,
            // Unclassifiable byte: fold it into the text stream.
            Err(()) => RawToken::Text,
        };
        out.push(Spanned {
            kind,
            start: span.start,
            end: span.end,
        });
    }
    out
}

/// Names that introduce a block (`{{#name}}…{{/name}}`).
fn block_name_of(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    if let Some(rest) = lower.strip_prefix('#') {
        let name = rest
            .split(|c: char| c == ' ' || c == ':')
            .next()
            .unwrap_or(rest);
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    } else if lower.starts_with(":each") {
        // `{{:each ...}}` is the alternate each-block opener.
        Some("each".to_string())
    } else {
        None
    }
}

/// Scan a template into its structural segments. This is the grouped view used
/// by the parser (evaluation).
///
/// It is a thin adapter over [`scan_spanned`]: that function owns the single
/// delimiter-pairing and escape-region implementation, emitting already-paired
/// `{{ … }}` items (with escape interiors collapsed to one literal `Text`).
/// `scan` projects that flat, span-carrying stream into the richer [`Segment`]
/// shape — comments, `Tag { content, raw }`, and grouped `Block`s — by walking
/// the items and pairing block openers with their `{{/name}}` closers. No
/// delimiter pairing or escape detection happens here; only the block-grouping
/// the parser needs (§1.3 rich→poor at the API edge, §4.1 one pairing rule).
pub fn scan(source: &str) -> Vec<Segment> {
    let items = scan_spanned(source);
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        match items[i] {
            SpanItem::Text { start, end } => {
                push_text(&mut out, &source[start..end]);
                i += 1;
            }
            SpanItem::Tag { inner, .. } => {
                let raw = source[inner.0..inner.1].to_string();
                let content = raw.trim().to_string();

                if content.starts_with("//") {
                    out.push(Segment::Comment(content));
                    i += 1;
                    continue;
                }

                if let Some(name) = block_name_of(&content) {
                    // Group the block: capture everything up to the matching
                    // `{{/name}}` item as the body. Pairing is already done by
                    // `scan_spanned`; here we only depth-count block opens/closes.
                    let (body, next) = group_block(&items, source, i, &name);
                    out.push(Segment::Block { header: content, name, body });
                    i = next;
                    continue;
                }

                out.push(Segment::Tag { content, raw });
                i += 1;
            }
        }
    }
    out
}

/// Append literal text to the segment stream, merging with a trailing
/// [`Segment::Text`] so adjacent literal runs stay a single segment (matching
/// the previous accumulate-then-flush behavior).
fn push_text(out: &mut Vec<Segment>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Segment::Text(existing)) = out.last_mut() {
        existing.push_str(text);
    } else {
        out.push(Segment::Text(text.to_string()));
    }
}

/// Capture the body of the block whose opening [`SpanItem::Tag`] is at index
/// `open_i`, returning `(body, index_after_block)`.
///
/// The body is the verbatim source between the opener's `}}` and the matching
/// `{{/name}}`'s `{{` (or end of input if unclosed). Nested same-name blocks
/// increase depth; the generic close `{{/}}` and `{{/name}}` decrease it.
/// `#escape` never nests (its interior is already one literal `Text` item from
/// [`scan_spanned`], so no inner block opener is visible to count).
fn group_block(items: &[SpanItem], source: &str, open_i: usize, name: &str) -> (String, usize) {
    let escape_literal = name == "escape";
    // Body starts just after the opener's closing `}}`.
    let body_start = match items[open_i] {
        SpanItem::Tag { close, .. } => close.1,
        SpanItem::Text { .. } => unreachable!("group_block called on a Text item"),
    };

    let mut depth = 1u32;
    let mut i = open_i + 1;
    while i < items.len() {
        if let SpanItem::Tag { open, inner, .. } = items[i] {
            let tag = source[inner.0..inner.1].trim().to_lowercase();
            if is_close_of(&tag, name) {
                depth -= 1;
                if depth == 0 {
                    // Body runs up to this close tag's opening `{{`.
                    let body = source[body_start..open.0].to_string();
                    return (body, i + 1);
                }
            } else if !escape_literal && opens_block_named(&tag, name) {
                depth += 1;
            }
        }
        i += 1;
    }
    // Unclosed: body runs to end of input.
    (source[body_start..].to_string(), items.len())
}

/// A flat, span-carrying view of a template, for consumers that need byte
/// offsets rather than reconstructed strings (the highlighter's coloring and the
/// diagnostic block-matcher). It shares the *same* `{{ }}` boundary and escape
/// rules as [`scan`] — the single source of truth — but keeps each `{{ … }}` as
/// its own item (rather than grouping blocks), since coloring and diagnostics
/// operate per-delimiter.
///
/// Escape regions are still honored: the `{{#escape}}` / `{{/escape}}` tags are
/// [`SpanItem::Tag`]s, but everything between them is a single
/// [`SpanItem::Text`] — so neither consumer interprets the interior as CBS.
#[derive(Debug, Clone, PartialEq)]
pub enum SpanItem {
    /// Literal text outside `{{ }}` (and escape-region interiors). Byte range
    /// `[start, end)` into the source.
    Text { start: usize, end: usize },
    /// A complete `{{ … }}`. `open`/`close` are the delimiter byte ranges;
    /// `inner` is the byte range of the content between them.
    Tag {
        open: (usize, usize),
        inner: (usize, usize),
        close: (usize, usize),
    },
}

/// Produce the flat, span-carrying view of `source`. See [`SpanItem`].
///
/// An unpaired `{{` degrades to a [`SpanItem::Text`] run (matching [`scan`]'s
/// incomplete-input recovery), so the highlighter keeps working mid-keystroke.
pub fn scan_spanned(source: &str) -> Vec<SpanItem> {
    let tokens = lex(source);
    let mut out = Vec::new();
    let mut text_start: Option<usize> = None;
    let mut pos = 0;

    // Flush any pending literal run that ends at `up_to`.
    let flush = |out: &mut Vec<SpanItem>, text_start: &mut Option<usize>, up_to: usize| {
        if let Some(s) = text_start.take() {
            if up_to > s {
                out.push(SpanItem::Text { start: s, end: up_to });
            }
        }
    };

    while pos < tokens.len() {
        if tokens[pos].kind == RawToken::Open {
            if let Some(close_idx) = find_matching_close_in(&tokens, pos + 1) {
                flush(&mut out, &mut text_start, tokens[pos].start);
                let open = (tokens[pos].start, tokens[pos].end);
                let inner = (tokens[pos].end, tokens[close_idx].start);
                let close = (tokens[close_idx].start, tokens[close_idx].end);
                out.push(SpanItem::Tag { open, inner, close });

                // If this opens a block, skip its body verbatim only for escape
                // (interior literal). For all other blocks we DON'T skip — the
                // highlighter wants to color the interior tags too; its own depth
                // stack handles nesting.
                let content = source[inner.0..inner.1].trim();
                if let Some(name) = block_name_of(content) {
                    if name == "escape" {
                        // Body is the text between the open's `}}` and the
                        // matching `{{/escape}}` open (or end of input).
                        let body_start = tokens.get(close_idx + 1).map(|t| t.start);
                        let close_open =
                            find_block_close_in(&tokens, source, close_idx + 1, &name);
                        match (body_start, close_open) {
                            (Some(bs), Some((open_i, _))) => {
                                let be = tokens[open_i].start;
                                if be > bs {
                                    out.push(SpanItem::Text { start: bs, end: be });
                                }
                                pos = open_i;
                            }
                            (Some(bs), None) => {
                                if source.len() > bs {
                                    out.push(SpanItem::Text { start: bs, end: source.len() });
                                }
                                pos = tokens.len();
                            }
                            (None, _) => {
                                pos = close_idx + 1;
                            }
                        }
                        continue;
                    }
                }
                pos = close_idx + 1;
                continue;
            }
        }
        // Literal token (text, lone brace, or an unpaired `{{`).
        if text_start.is_none() {
            text_start = Some(tokens[pos].start);
        }
        pos += 1;
    }
    flush(&mut out, &mut text_start, source.len());
    out
}

/// Free-function form of [`Scanner::find_matching_close`] over a token slice,
/// shared by [`scan_spanned`]. Finds the `}}` closing the `{{` whose inner
/// content begins at token index `from`, honoring nested `{{ }}`.
fn find_matching_close_in(tokens: &[Spanned], from: usize) -> Option<usize> {
    let mut depth = 1u32;
    let mut i = from;
    while i < tokens.len() {
        match tokens[i].kind {
            RawToken::Open => depth += 1,
            RawToken::Close => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Free-function form of [`Scanner::find_block_close`] for the escape case only
/// (escape bodies are literal, so nested `#escape` opens don't increase depth).
/// Returns `(open_token_index, close_token_index)` of the matching `{{/escape}}`.
fn find_block_close_in(
    tokens: &[Spanned],
    source: &str,
    start: usize,
    name: &str,
) -> Option<(usize, usize)> {
    let mut i = start;
    while i < tokens.len() {
        if tokens[i].kind == RawToken::Open {
            if let Some(close_i) = find_matching_close_in(tokens, i + 1) {
                let inner = source[tokens[i].end..tokens[close_i].start]
                    .trim()
                    .to_lowercase();
                if is_close_of(&inner, name) {
                    return Some((i, close_i));
                }
                i = close_i + 1;
                continue;
            }
        }
        i += 1;
    }
    None
}

/// Does trimmed-lowercased tag inner `inner` close a block named `name`?
/// Accepts `/name` and bare `/` (generic close).
fn is_close_of(inner: &str, name: &str) -> bool {
    if let Some(rest) = inner.strip_prefix('/') {
        let close_name = rest
            .split(|c: char| c == ' ' || c == ':')
            .next()
            .unwrap_or(rest)
            .trim();
        close_name == name || close_name.is_empty()
    } else {
        false
    }
}

/// Does trimmed-lowercased tag inner `inner` open another block named `name`?
fn opens_block_named(inner: &str, name: &str) -> bool {
    block_name_of(inner).as_deref() == Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(segs: &[Segment]) -> Vec<&str> {
        segs.iter()
            .filter_map(|s| match s {
                Segment::Tag { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plain_text_is_one_segment() {
        let segs = scan("hello world");
        assert_eq!(segs, vec![Segment::Text("hello world".into())]);
    }

    #[test]
    fn simple_tag() {
        let segs = scan("{{char}}");
        assert_eq!(tags(&segs), vec!["char"]);
    }

    #[test]
    fn text_around_tag() {
        let segs = scan("a{{char}}b");
        assert_eq!(
            segs,
            vec![
                Segment::Text("a".into()),
                Segment::Tag { content: "char".into(), raw: "char".into() },
                Segment::Text("b".into()),
            ]
        );
    }

    #[test]
    fn nested_tag_is_single_outer_tag() {
        // The outer tag's raw content includes the inner {{ }} verbatim; the
        // parser re-evaluates it. The boundary scan must treat this as ONE tag.
        let segs = scan("{{upper::{{char}}}}");
        assert_eq!(tags(&segs), vec!["upper::{{char}}"]);
    }

    #[test]
    fn single_braces_are_literal() {
        let segs = scan("a{b}c {d");
        assert_eq!(segs, vec![Segment::Text("a{b}c {d".into())]);
    }

    #[test]
    fn unclosed_delim_is_literal() {
        let segs = scan("a {{char b");
        assert_eq!(segs, vec![Segment::Text("a {{char b".into())]);
    }

    #[test]
    fn comment_segment() {
        let segs = scan("x{{// note}}y");
        assert!(matches!(&segs[1], Segment::Comment(c) if c == "// note"));
    }

    #[test]
    fn block_basic() {
        let segs = scan("{{#if 1}}body{{/if}}");
        assert_eq!(segs.len(), 1);
        match &segs[0] {
            Segment::Block { header, name, body } => {
                assert_eq!(header, "#if 1");
                assert_eq!(name, "if");
                assert_eq!(body, "body");
            }
            other => panic!("expected block, got {other:?}"),
        }
    }

    #[test]
    fn block_nested_same_name() {
        let segs = scan("{{#when 1}}a{{#when 1}}b{{/when}}c{{/when}}");
        match &segs[0] {
            Segment::Block { name, body, .. } => {
                assert_eq!(name, "when");
                assert_eq!(body, "a{{#when 1}}b{{/when}}c");
            }
            other => panic!("expected block, got {other:?}"),
        }
    }

    #[test]
    fn each_colon_opener() {
        let segs = scan("{{:each x as v}}{{slot::v}}{{/each}}");
        match &segs[0] {
            Segment::Block { name, .. } => assert_eq!(name, "each"),
            other => panic!("expected each block, got {other:?}"),
        }
    }

    #[test]
    fn escape_interior_is_literal_single_text() {
        // The {{/if_pure}} and {{/0}} inside #escape must NOT be parsed as
        // blocks — they belong to the literal body. This is the escape-region fix.
        let segs = scan("{{#escape}}{{/if_pure}}{{/0}}literal{{/escape}}");
        assert_eq!(segs.len(), 1);
        match &segs[0] {
            Segment::Block { name, body, .. } => {
                assert_eq!(name, "escape");
                assert_eq!(body, "{{/if_pure}}{{/0}}literal");
            }
            other => panic!("expected escape block, got {other:?}"),
        }
    }

    #[test]
    fn escape_does_not_nest_on_inner_escape_open() {
        // Inside an escape body, a nested `{{#escape}}` is literal, so the
        // FIRST `{{/escape}}` closes the outer block.
        let segs = scan("{{#escape}}A{{#escape}}B{{/escape}}C{{/escape}}");
        match &segs[0] {
            Segment::Block { name, body, .. } => {
                assert_eq!(name, "escape");
                assert_eq!(body, "A{{#escape}}B");
            }
            other => panic!("expected escape block, got {other:?}"),
        }
        // Remaining "C" + stray close become literal text.
        let txt: String = segs[1..]
            .iter()
            .map(|s| match s {
                Segment::Text(t) => t.clone(),
                _ => String::new(),
            })
            .collect();
        assert!(txt.contains('C'));
    }

    #[test]
    fn unclosed_block_body_to_end() {
        let segs = scan("{{#if 1}}rest of input");
        match &segs[0] {
            Segment::Block { name, body, .. } => {
                assert_eq!(name, "if");
                assert_eq!(body, "rest of input");
            }
            other => panic!("expected block, got {other:?}"),
        }
    }

    #[test]
    fn generic_close_closes_block() {
        let segs = scan("{{#if 1}}body{{/}}");
        match &segs[0] {
            Segment::Block { name, body, .. } => {
                assert_eq!(name, "if");
                assert_eq!(body, "body");
            }
            other => panic!("expected block, got {other:?}"),
        }
    }

    // ── scan_spanned (flat, span-carrying view for highlighter/diagnostics) ──

    /// Reconstruct the strings each SpanItem covers, for readable assertions.
    fn spanned_strs(source: &str) -> Vec<(&'static str, String)> {
        scan_spanned(source)
            .into_iter()
            .map(|item| match item {
                SpanItem::Text { start, end } => ("text", source[start..end].to_string()),
                SpanItem::Tag { open, inner, close } => {
                    // Verify open/close cover the delimiters exactly.
                    assert_eq!(&source[open.0..open.1], "{{");
                    assert_eq!(&source[close.0..close.1], "}}");
                    ("tag", source[inner.0..inner.1].to_string())
                }
            })
            .collect()
    }

    #[test]
    fn spanned_text_and_tag() {
        assert_eq!(
            spanned_strs("a{{char}}b"),
            vec![
                ("text", "a".into()),
                ("tag", "char".into()),
                ("text", "b".into()),
            ]
        );
    }

    #[test]
    fn spanned_nested_tag_is_single() {
        // The outer tag's inner includes the nested {{ }} verbatim.
        assert_eq!(
            spanned_strs("{{upper::{{char}}}}"),
            vec![("tag", "upper::{{char}}".into())]
        );
    }

    #[test]
    fn spanned_unclosed_is_text() {
        assert_eq!(
            spanned_strs("a {{char b"),
            vec![("text", "a {{char b".into())]
        );
    }

    #[test]
    fn spanned_block_keeps_open_close_as_tags_body_colored() {
        // A non-escape block keeps interior tags visible (highlighter colors them).
        assert_eq!(
            spanned_strs("{{#if 1}}x{{char}}{{/if}}"),
            vec![
                ("tag", "#if 1".into()),
                ("text", "x".into()),
                ("tag", "char".into()),
                ("tag", "/if".into()),
            ]
        );
    }

    #[test]
    fn spanned_escape_interior_is_one_literal_text() {
        // The {{/if_pure}} and {{/0}} inside escape must be ONE literal text run,
        // bracketed by the escape open/close tags — never colored as CBS.
        assert_eq!(
            spanned_strs("{{#escape}}{{/if_pure}}{{/0}}lit{{/escape}}"),
            vec![
                ("tag", "#escape".into()),
                ("text", "{{/if_pure}}{{/0}}lit".into()),
                ("tag", "/escape".into()),
            ]
        );
    }

    #[test]
    fn spanned_unclosed_escape_runs_to_end() {
        assert_eq!(
            spanned_strs("{{#escape}}{{/0}}tail"),
            vec![
                ("tag", "#escape".into()),
                ("text", "{{/0}}tail".into()),
            ]
        );
    }

    #[test]
    fn spanned_comment_is_a_tag() {
        // Comments are tags here (the highlighter classifies them); only escape
        // bodies become text.
        assert_eq!(
            spanned_strs("x{{// note}}y"),
            vec![
                ("text", "x".into()),
                ("tag", "// note".into()),
                ("text", "y".into()),
            ]
        );
    }
}
