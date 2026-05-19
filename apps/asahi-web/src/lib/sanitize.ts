import DOMPurify from "dompurify";

/**
 * Sanitize HTML coming from the server before it lands in a
 * dangerouslySetInnerHTML call. Allowlist matches the Tiptap StarterKit
 * surface: structural prose elements only, no scripts, no inline event
 * handlers, no data-URIs in attributes.
 *
 * Keep the list tight — adding a new tag means auditing every code path
 * that renders sanitized HTML.
 */
const ALLOWED_TAGS = [
  "p",
  "br",
  "strong",
  "em",
  "u",
  "s",
  "code",
  "pre",
  "blockquote",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "ul",
  "ol",
  "li",
  "a",
  "hr",
];

const ALLOWED_ATTR = ["href", "title", "target", "rel"];

export function sanitizeRichText(html: string | null | undefined): string {
  if (!html) return "";
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOW_DATA_ATTR: false,
    KEEP_CONTENT: true,
  });
}
