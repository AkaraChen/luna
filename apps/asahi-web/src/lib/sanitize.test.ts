import { describe, expect, it } from "vitest";

import { sanitizeRichText } from "./sanitize";

describe("sanitizeRichText", () => {
  it("returns an empty string for null input", () => {
    expect(sanitizeRichText(null)).toBe("");
    expect(sanitizeRichText(undefined)).toBe("");
    expect(sanitizeRichText("")).toBe("");
  });

  it("keeps the structural tags used by rich text", () => {
    const cases = [
      ["p", "<p>text</p>"],
      ["strong", "<strong>text</strong>"],
      ["em", "<em>text</em>"],
      ["u", "<u>text</u>"],
      ["s", "<s>text</s>"],
      ["code", "<code>text</code>"],
      ["pre", "<pre>text</pre>"],
      ["blockquote", "<blockquote>text</blockquote>"],
      ["h1", "<h1>text</h1>"],
      ["h2", "<h2>text</h2>"],
      ["h3", "<h3>text</h3>"],
      ["h4", "<h4>text</h4>"],
      ["h5", "<h5>text</h5>"],
      ["h6", "<h6>text</h6>"],
      ["ul", "<ul><li>text</li></ul>"],
      ["ol", "<ol><li>text</li></ol>"],
      ["a", '<a href="https://example.test">text</a>'],
      ["hr", "<hr>"],
      ["br", "<br>"],
    ] as const;

    for (const [tag, html] of cases) {
      expect(sanitizeRichText(html), tag).toContain(`<${tag}`);
    }
  });

  it("script-strip removes script elements", () => {
    const result = sanitizeRichText("<p>safe</p><script>alert(1)</script>");

    expect(result).toContain("<p>safe</p>");
    expect(result).not.toContain("<script");
  });

  it("event-handler-strip removes inline event handlers", () => {
    const result = sanitizeRichText('<p onclick="evil()">safe</p>');

    expect(result).toBe("<p>safe</p>");
    expect(result).not.toContain("onclick");
  });

  it("removes unsafe media and style tags", () => {
    const result = sanitizeRichText(
      '<img src="x" onerror="evil()"><iframe src="https://example.test"></iframe><style>p{color:red}</style><p>safe</p>',
    );

    expect(result).not.toContain("<img");
    expect(result).not.toContain("<iframe");
    expect(result).not.toContain("<style");
    expect(result).toContain("<p>safe</p>");
  });

  it("removes javascript links and data URI attributes", () => {
    const result = sanitizeRichText(
      '<a href="javascript:alert(1)">bad</a><a href="data:text/html,evil">data</a>',
    );

    expect(result).not.toContain("javascript:");
    expect(result).not.toContain("data:text/html");
    expect(result).toContain("<a>bad</a>");
    expect(result).toContain("<a>data</a>");
  });

  it("does not throw on malformed nested markup", () => {
    expect(() => sanitizeRichText("<p><strong>open")).not.toThrow();
    expect(sanitizeRichText("<p><strong>open")).toContain("open");
  });

  it("keeps configured link attributes without inventing rel", () => {
    const result = sanitizeRichText(
      '<a href="https://example.test" title="Example" target="_blank">Example</a>',
    );

    // NOTE: target is allowed, but rel is not synthesized by this sanitizer.
    expect(result).toContain('href="https://example.test"');
    expect(result).toContain('title="Example"');
    expect(result).toContain('target="_blank"');
    expect(result).not.toContain("rel=");
  });
});
