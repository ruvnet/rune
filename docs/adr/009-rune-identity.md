# ADR 009: Rune identity and visual language

Status: Accepted for this architecture package; public naming availability unresolved
Date: 2026-10-26

## Decision

Use Rune as the user selected project identity. The tagline is The learning toolchain for Rust. The proposed executable is rune, pending namespace checks. Existing ADR identifiers remain stable. No executable or package reservation is claimed.

Rune owns build orchestration and validated discovery. RVForge retains artifact authoring and staging, RVF remains the artifact format and RVM remains a runtime. Avoid renaming those existing components.

Use an original geometric R mark with graphite backgrounds, copper outlines and amber highlights. Decorative glyphs are abstract geometry, not claims of historical rune meanings. Animated lines represent conceptual flow, never actual execution or verified progress.

## Rendering rules

Repository assets use self contained SVG with no scripts, external fonts, foreignObject or network references. SVG CSS animates strokes and opacity. Reduced motion preference disables animation. Every image has title and description; surrounding README prose carries its substantive content if animation or rendering is unavailable. Stable static compositions remain legible without CSS animation.

## Consequences and alternatives

Rune unifies the product narrative but introduces collision risk with existing software names. RuForge was rejected because of confusion with RVForge. The final package, repository and binary names remain subject to discovery and clearance. Public release must resolve namespace conflicts; an alternate executable name can preserve the product identity.

## Acceptance

All ADR scopes name Rune; README links resolve; SVG XML parses and contains no active content or external references; the static composition is inspected. Each visualization identifies proposed behavior and makes no claim of a completed compiler. Public release additionally requires namespace and trademark review.
