---
name: UI/UX Pro Max
description: UI/UX design intelligence for web and mobile. Includes 50+ styles, 161 color palettes, 57 font pairings, 161 product types, 99 UX guidelines, and 25 chart types across 10 stacks (React, Next.js, Vue, Svelte, SwiftUI, React Native, Flutter, Tailwind, shadcn/ui, and HTML/CSS). Actions: plan, build, create, design, implement, review, fix, improve, optimize, enhance, refactor, and check UI/UX code. Projects: website, landing page, dashboard, admin panel, e-commerce, SaaS, portfolio, blog, and mobile app. Elements: button, modal, navbar, sidebar, card, table, form, and chart. Styles: glassmorphism, claymorphism, minimalism, brutalism, neumorphism, bento grid, dark mode, responsive, skeuomorphism, and flat design. Topics: color systems, accessibility, animation, layout, typography, font pairing, spacing, interaction states, shadow, and gradient.
type: passive
match: 
companions: frontend-design
companions: frontend-design
---
# UI/UX Pro Max - Design Intelligence

**IMPORTANT: These rules are reference guidelines for your FIRST DRAFT. Do NOT read your output and re-check every rule — that causes infinite loops. Write once, review once (max 1 read-back), then complete_step. The user will ask for adjustments if needed.**

Comprehensive design guide for web and mobile applications. Contains 50+ styles, 161 color palettes, 57 font pairings, 161 product types with reasoning rules, 99 UX guidelines, and 25 chart types across 10 technology stacks.

## When to Apply

This Skill should be used when the task involves **UI structure, visual design decisions, interaction patterns, or user experience quality control**.

### Must Use
- Designing new pages (Landing Page, Dashboard, Admin, SaaS, Mobile App)
- Creating or refactoring UI components (buttons, modals, forms, tables, charts, etc.)
- Choosing color schemes, typography systems, spacing standards, or layout systems
- Reviewing UI code for user experience, accessibility, or visual consistency
- Implementing navigation structures, animations, or responsive behavior
- Making product-level design decisions (style, information hierarchy, brand expression)
- Improving perceived quality, clarity, or usability of interfaces

### Skip
- Pure backend logic development
- Only involving API or database design
- Performance optimization unrelated to the interface
- Infrastructure or DevOps work
- Non-visual scripts or automation tasks

**Decision criteria**: If the task will change how a feature **looks, feels, moves, or is interacted with**, use this Skill.

## Rule Categories by Priority

| Priority | Category | Impact | Key Checks | Anti-Patterns |
|----------|----------|--------|------------|---------------|
| 1 | Accessibility | CRITICAL | Contrast 4.5:1, Alt text, Keyboard nav, Aria-labels | Removing focus rings, Icon-only buttons without labels |
| 2 | Touch & Interaction | CRITICAL | Min size 44×44px, 8px+ spacing, Loading feedback | Reliance on hover only, Instant state changes (0ms) |
| 3 | Performance | HIGH | WebP/AVIF, Lazy loading, Reserve space (CLS < 0.1) | Layout thrashing, Cumulative Layout Shift |
| 4 | Style Selection | HIGH | Match product type, Consistency, SVG icons (no emoji) | Mixing flat & skeuomorphic randomly, Emoji as icons |
| 5 | Layout & Responsive | HIGH | Mobile-first breakpoints, Viewport meta, No horizontal scroll | Horizontal scroll, Fixed px container widths, Disable zoom |
| 6 | Typography & Color | MEDIUM | Base 16px, Line-height 1.5, Semantic color tokens | Text < 12px body, Gray-on-gray, Raw hex in components |
| 7 | Animation | MEDIUM | Duration 150–300ms, Motion conveys meaning, Spatial continuity | Decorative-only animation, Animating width/height, No reduced-motion |
| 8 | Forms & Feedback | MEDIUM | Visible labels, Error near field, Helper text, Progressive disclosure | Placeholder-only label, Errors only at top, Overwhelm upfront |
| 9 | Navigation Patterns | HIGH | Predictable back, Bottom nav ≤5, Deep linking | Overloaded nav, Broken back behavior, No deep links |
| 10 | Charts & Data | LOW | Legends, Tooltips, Accessible colors | Relying on color alone to convey meaning |

## Quick Reference

### 1. Accessibility (CRITICAL)
- `color-contrast` - Minimum 4.5:1 ratio for normal text (large text 3:1)
- `focus-states` - Visible focus rings on interactive elements (2–4px)
- `alt-text` - Descriptive alt text for meaningful images
- `aria-labels` - aria-label for icon-only buttons
- `keyboard-nav` - Tab order matches visual order; full keyboard support
- `form-labels` - Use label with for attribute
- `skip-links` - Skip to main content for keyboard users
- `heading-hierarchy` - Sequential h1→h6, no level skip
- `color-not-only` - Don't convey info by color alone (add icon/text)
- `reduced-motion` - Respect prefers-reduced-motion; reduce/disable animations when requested
- `escape-routes` - Provide cancel/back in modals and multi-step flows

### 2. Touch & Interaction (CRITICAL)
- `touch-target-size` - Min 44×44pt (Apple) / 48×48dp (Material); extend hit area beyond visual bounds if needed
- `touch-spacing` - Minimum 8px/8dp gap between touch targets
- `hover-vs-tap` - Use click/tap for primary interactions; don't rely on hover alone
- `loading-buttons` - Disable button during async operations; show spinner or progress
- `error-feedback` - Clear error messages near problem
- `cursor-pointer` - Add cursor-pointer to clickable elements (Web)
- `tap-delay` - Use touch-action: manipulation to reduce 300ms delay (Web)
- `press-feedback` - Visual feedback on press (ripple/highlight)
- `safe-area-awareness` - Keep primary touch targets away from notch, gesture bar and screen edges
- `drag-threshold` - Use a movement threshold before starting drag to avoid accidental drags

### 3. Performance (HIGH)
- `image-optimization` - Use WebP/AVIF, responsive images (srcset/sizes), lazy load non-critical assets
- `image-dimension` - Declare width/height or use aspect-ratio to prevent layout shift (CLS)
- `font-loading` - Use font-display: swap/optional to avoid invisible text (FOIT)
- `critical-css` - Prioritize above-the-fold CSS
- `lazy-loading` - Lazy load non-hero components via dynamic import / route-level splitting
- `content-jumping` - Reserve space for async content to avoid layout jumps (CLS)
- `virtualize-lists` - Virtualize lists with 50+ items to improve scroll performance
- `progressive-loading` - Use skeleton screens / shimmer instead of long blocking spinners
- `debounce-throttle` - Use debounce/throttle for high-frequency events (scroll, resize, input)

### 4. Style Selection (HIGH)
- `style-match` - Match style to product type
- `consistency` - Use same style across all pages
- `no-emoji-icons` - Use SVG icons (Heroicons, Lucide), not emojis
- `effects-match-style` - Shadows, blur, radius aligned with chosen style
- `state-clarity` - Make hover/pressed/disabled states visually distinct
- `elevation-consistent` - Use a consistent elevation/shadow scale for cards, sheets, modals
- `dark-mode-pairing` - Design light/dark variants together
- `primary-action` - Each screen should have only one primary CTA

### 5. Layout & Responsive (HIGH)
- `viewport-meta` - width=device-width initial-scale=1 (never disable zoom)
- `mobile-first` - Design mobile-first, then scale up to tablet and desktop
- `breakpoint-consistency` - Use systematic breakpoints (e.g. 375 / 768 / 1024 / 1440)
- `readable-font-size` - Minimum 16px body text on mobile (avoids iOS auto-zoom)
- `line-length-control` - Mobile 35–60 chars per line; desktop 60–75 chars
- `horizontal-scroll` - No horizontal scroll on mobile
- `spacing-scale` - Use 4pt/8dp incremental spacing system
- `container-width` - Consistent max-width on desktop (max-w-6xl / 7xl)
- `z-index-management` - Define layered z-index scale
- `viewport-units` - Prefer min-h-dvh over 100vh on mobile

### 6. Typography & Color (MEDIUM)
- `line-height` - Use 1.5-1.75 for body text
- `line-length` - Limit to 65-75 characters per line
- `font-pairing` - Match heading/body font personalities
- `font-scale` - Consistent type scale (e.g. 12 14 16 18 24 32)
- `contrast-readability` - Darker text on light backgrounds
- `weight-hierarchy` - Bold headings (600–700), Regular body (400), Medium labels (500)
- `color-semantic` - Define semantic color tokens (primary, secondary, error, surface)
- `color-dark-mode` - Dark mode uses desaturated / lighter tonal variants, not inverted colors
- `color-accessible-pairs` - Foreground/background pairs must meet 4.5:1 (AA) or 7:1 (AAA)
- `truncation-strategy` - Prefer wrapping over truncation; use ellipsis only when necessary

### 7. Animation (MEDIUM)
- `duration-timing` - Use 150–300ms for micro-interactions; complex transitions ≤400ms
- `transform-performance` - Use transform/opacity only; avoid animating width/height/top/left
- `loading-states` - Show skeleton or progress indicator when loading exceeds 300ms
- `easing` - Use ease-out for entering, ease-in for exiting; avoid linear
- `motion-meaning` - Every animation must express a cause-effect relationship
- `spring-physics` - Prefer spring/physics-based curves over linear or cubic-bezier
- `exit-faster-than-enter` - Exit animations shorter than enter (~60–70% of enter duration)
- `stagger-sequence` - Stagger list/grid item entrance by 30–50ms per item
- `interruptible` - Animations must be interruptible; user tap cancels in-progress animation
- `scale-feedback` - Subtle scale (0.95–1.05) on press for tappable elements

### 8. Forms & Feedback (MEDIUM)
- `input-labels` - Visible label per input (not placeholder-only)
- `error-placement` - Show error below the related field
- `submit-feedback` - Loading then success/error state on submit
- `required-indicators` - Mark required fields (e.g. asterisk)
- `empty-states` - Helpful message and action when no content
- `toast-dismiss` - Auto-dismiss toasts in 3-5s
- `confirmation-dialogs` - Confirm before destructive actions
- `inline-validation` - Validate on blur (not keystroke)
- `error-recovery` - Error messages must include a clear recovery path
- `undo-support` - Allow undo for destructive or bulk actions
- `destructive-emphasis` - Destructive actions use semantic danger color (red)

### 9. Navigation Patterns (HIGH)
- `bottom-nav-limit` - Bottom navigation max 5 items; use labels with icons
- `back-behavior` - Back navigation must be predictable and consistent; preserve scroll/state
- `deep-linking` - All key screens must be reachable via deep link / URL
- `nav-state-active` - Current location must be visually highlighted
- `nav-hierarchy` - Primary nav vs secondary nav must be clearly separated
- `modal-escape` - Modals and sheets must offer a clear close/dismiss affordance
- `breadcrumb-web` - Web: use breadcrumbs for 3+ level deep hierarchies
- `state-preservation` - Navigating back must restore previous scroll position
- `persistent-nav` - Core navigation must remain reachable from deep pages

### 10. Charts & Data (LOW)
- `chart-type` - Match chart type to data type (trend → line, comparison → bar, proportion → pie/donut)
- `color-guidance` - Use accessible color palettes; avoid red/green only pairs
- `legend-visible` - Always show legend; position near the chart
- `tooltip-on-interact` - Provide tooltips/data labels on hover showing exact values
- `axis-labels` - Label axes with units and readable scale
- `responsive-chart` - Charts must reflow or simplify on small screens
- `empty-data-state` - Show meaningful empty state when no data exists
- `no-pie-overuse` - Avoid pie/donut for >5 categories; switch to bar chart
- `gridline-subtle` - Grid lines should be low-contrast
- `number-formatting` - Use locale-aware formatting for numbers, dates, currencies
