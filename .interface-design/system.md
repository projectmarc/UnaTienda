# UnaTienda Interface Design Contract

## Product Intent

UnaTienda is a local-first desktop point-of-sale application for a small-shop owner working while standing behind the counter. The interface must feel fast, calm, practical, and trustworthy, with information scannable at standing distance and offline operation treated as normal.

The primary workflow is linear: open a cash session, register sales and cash movements, reconcile the drawer, then close the session. A continuous session timeline is both the primary navigation and the product's visual signature; it shows opening, sales, movements, reconciliation, and closing as one accountable sequence.

## Structure

- Let the current session and its timeline organize the application shell and screen hierarchy.
- Keep the next contextual action obvious without hiding prior session events.
- Use large, scannable monetary totals and clear labels; render money with a monospace face and tabular numerals.
- Production UI copy is Spanish. This contract and implementation documentation remain English.
- Design for efficient keyboard and mouse use on desktop while keeping controls comfortably targetable.

## Visual Language

- Use receipt-paper beige for the canvas, graphite for text and rules, muted cash green for primary or successful action, restrained amber for warnings, and accounting red for destructive actions, errors, or discrepancies.
- Color is semantic, not decorative. Preserve a clear hierarchy of primary, secondary, tertiary, and muted text.
- Use borders-only depth. Separate layers with quiet graphite rules and subtle warm surface shifts; do not use drop shadows.
- Use a 4px primitive spacing unit and an 8px layout rhythm. Prefer symmetrical padding and deliberate density.
- Keep corner radii restrained and consistent: small on controls, modest on larger containers, never pill-like by default.
- Keep motion brief, functional, and ease-out. Respect reduced-motion preferences; avoid bounce, spring, or ornamental transitions.

## Required States

- Model the session states explicitly: closed/opening, active, reconciling, closing, and closed.
- Every control needs default, hover, active, focus-visible, disabled, and in-progress states where applicable.
- Every data view needs loading, empty, populated, stale/offline, validation-error, backend-error, and retry or recovery states where applicable.
- Preserve entered data through recoverable failures. Confirm destructive or irreversible actions and clearly communicate discrepancies before closing a session.

## Accessibility And Input

- Meet WCAG AA contrast for text, controls, status indicators, and focus treatment; never communicate state through color alone.
- Use semantic HTML, explicit labels, meaningful headings, and live announcements for asynchronous results and errors.
- Provide a visible, high-contrast focus indicator and a logical focus order. All workflows, menus, dialogs, and timeline actions must be operable by keyboard without traps.
- Keep monetary values and primary actions legible at standing distance, and provide sufficiently large pointer targets even within the compact desktop layout.

## Implementation Target

Implement with React and Tailwind CSS, using shadcn/ui or Base UI primitives where they improve accessible behavior. Adapt primitives to this contract rather than inheriting generic component styling. Keep design decisions in semantic application tokens; do not import generated token sets.

## Anti-Patterns

- No sidebar, generic SaaS dashboard, metric-card grid, or disconnected card mosaic.
- No decorative gradients, dramatic shadows, glass effects, marketing sections, or ornamental branding.
- No excessive rounding, pill-shaped containers, floating white cards, thick borders, or abrupt surface-color jumps.
- No multiple competing accents, decorative status colors, tiny financial figures, proportional money columns, or mobile-first visual treatment on the desktop POS.
- Do not let component-library defaults replace the session workflow, timeline signature, Spanish copy, or accessibility requirements.

Generated Open Design HTML, manifests, token dumps, scripts, branding, and marketing artifacts are not authoritative. This file is the canonical, tool-neutral design contract for UnaTienda.
