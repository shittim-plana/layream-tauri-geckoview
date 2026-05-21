/** Basic markdown rendering for char messages.
 *  Handles: ```code blocks```, `inline code`, **bold**, *italic*, line breaks.
 *  Escapes HTML first to prevent XSS, then applies markdown patterns. */
export function renderMarkdown(text) {
  if (!text) return "";
  // Escape HTML entities
  let html = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  // Fenced code blocks: ```lang\n...\n```
  html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, _lang, code) => {
    return `<pre class="md-code-block"><code>${code.replace(/\n$/, "")}</code></pre>`;
  });
  // Inline code: `code`
  html = html.replace(/`([^`\n]+)`/g, '<code class="md-inline-code">$1</code>');
  // Bold: **text**
  html = html.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
  // Italic: *text* (but not inside **)
  html = html.replace(/(?<!\*)\*([^*\n]+)\*(?!\*)/g, "<em>$1</em>");

  return html;
}
