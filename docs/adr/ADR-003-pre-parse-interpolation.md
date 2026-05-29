# ADR-003: Pre-Parse Fixture Interpolation

## Status: Accepted

## Context

The runner loaded fixtures.yaml into `RunContext.vars` as JSON values and interpolated `{{fixtures.*}}` patterns at runtime in string fields. But YAML integer fields (`user_id`, `site_id`) are parsed by serde_yaml before interpolation — a template like `"{{fixtures.test_user.id}}"` in an i64 field causes a parse error.

TC authors had to hardcode IDs instead of using fixture refs, breaking the "never hardcode" rule (R7 in tc-authoring-guide.md).

Industry: Mustache/Handlebars interpolation runs on raw template strings before any structured parsing. pytest fixtures are resolved before test function parameters are typed.

## Decision

Two-phase interpolation:
1. **Pre-parse**: Load fixtures.yaml into a flat `HashMap<String, String>`. Replace all `{{fixtures.*}}` in the raw YAML string before `serde_yaml::from_str()`. Integer values replace the quoted pattern including quotes: `"{{fixtures.test_site.id}}"` → `31255` (bare int).
2. **Runtime**: `RunContext::interpolate()` still runs for api_call `save_as` vars that are populated mid-TC.

Pre-parse handles fixtures.yaml. Runtime handles dynamic vars (auth tokens, created resource IDs).

## Consequences

- TC authors can use `{{fixtures.test_site.id}}` in any field type including integers
- Hardcoded IDs in YAML are no longer necessary
- Two interpolation paths remain (pre-parse + runtime) — Phase 4 unifies into a single FixtureResolver
- Missing keys pass through as literal strings (no panic, no empty)
