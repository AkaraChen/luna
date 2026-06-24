# Asahi Web — Design DNA

A short reference for the visual system this app implements. Detailed tokens
live in `src/style.css`; the patterns below say _why_ the values are what they
are.

## North star

**The Studio Notebook.** A tracker should look like an open notebook on a
writer's desk: pure white pages, quiet ink, the work is the subject, the
interface is the margin. The user is already in the task; the surface
shouldn't announce itself.

The system explicitly rejects three reflexes:

- The Linear-clone cockpit (permanent filter rails + toolbars + chips per row).
- The SaaS-cream warmth (paper textures, painterly hues, magazine eyebrows
  where a date stamp would do).
- The dark-dashboard default (dark blue + neon accents the moment someone
  says "tools").

## Colour

- Pure white canvas (`oklch(1 0 0)`), near-black ink (`oklch(0.16 0 0)`).
- Cool-neutral hairlines (`oklch(0.92 0 0)`), one muted-text strength
  (`oklch(0.5 0 0)`). No second mid-gray.
- Colour is **semantic or invisible.** Done is green, In Progress is amber,
  mentions are blue. No decorative tinting on icons, gradients, or buttons.
- See [The Semantic-Only Rule](#) in `style.css`'s comments.

## Typography

- Inter Variable, two weights (400/500). No bold, no light.
- Type scale tops out at 16px (the sidebar wordmark). 15px for page headings,
  14px for list rows, 13.5px for body, 12px for meta, 11px for eyebrows.
- No display scale anywhere in the app — that is reserved for marketing pages.
- Uppercase tracking-loose eyebrow (`asahi-eyebrow` utility) is a divider, not
  a heading.
- JetBrains Mono used for one thing only: issue keys.

## Layout

- Hairline borders, not boxes. Cards exist for comment bubbles only.
- Three structural anchors: fixed left sidebar, sticky right metadata rail,
  sticky bottom composer. Everything between scrolls.
- Pill toggle on rail is the one allowed piece of chrome that feels like a
  "control." Everything else is text or hairline.

## Elevation

Flat by default. The only resting shadow in the system is `0 1px 2px
oklch(0 0 0 / 0.04)`, applied via `.asahi-pill-lift` to the active pill in a
pill toggle. New components don't need a shadow; if they look like they do,
redesign the component.

## Motion

- Custom strong easings: `--ease-out-strong` for entrances, `--ease-in-out-strong`
  for on-screen movement, `--ease-drawer` for sheets.
- Durations: 120–180ms for popovers/dropdowns, 180–220ms for modals/drawers,
  140ms for press feedback. Stay under 300ms.
- Press feedback: `transform: scale(0.97)` on `:active` (utility class
  `.asahi-press`).
- Stagger: 22–28ms per list row, capped at ~220ms total.
- `prefers-reduced-motion` strips transforms, keeps opacity transitions.

## Don'ts

- Don't use display-scale type in app surfaces.
- Don't wrap rows in cards. Hairlines do the work.
- Don't introduce a second muted-gray strength.
- Don't tint icons as decoration. Colour is semantic.
- Don't add a resting shadow to make something "feel like a card."
- Don't reach for a modal when an inline or progressive disclosure works.
- Don't use em dashes in copy. Commas, semicolons, periods, parentheses.

The longer-form version of this document, with the full named-rule catalogue,
lives in the prototype repo (`progressus-web/DESIGN.md`). When values
disagree, this file wins for `apps/asahi-web/`.
