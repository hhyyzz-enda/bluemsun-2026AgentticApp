# Shared theme reference

These offline fixtures run in `cargo run --profile fast --example theme_mvp`.
They exercise the same Splash registration and L0 presentation adapter used by
Rinx. They are not installable service bundles: their `fixture.*` requests are
answered only by the example harness.

## Splash

Use `mod.widgets.RinxLabel`, `RinxHint`, `RinxPageTitle`, `RinxInput`,
`RinxButton`, and `RinxPrimaryButton`. The host registers these in the isolate
and propagates its Makepad stylesheet. Use named widget IDs for stateful and
dynamic content so reconciliation retains the correct instance.

Presentation source runs again when a theme changes. Guard model initialization
and startup service calls with `if !mod.rinx_theme.reloading()`. Event handlers
continue to run normally. The pinned Makepad runtime preserves module state,
input edits, and handler timers, and replaces rather than stacks top-level
timers. Arbitrary unguarded top-level effects in old scripts are not guaranteed
to be replay-safe.

## L0 native kit

The existing `theme light` ledger name selects `kit/native/light/kit.json`.
Rinx resolves its semantic token references in memory against the current host
palette; the directory name does not force a light appearance.

Supported host roles are `color.surface.{page,panel,field}`,
`color.content.{primary,secondary}`, `color.action.{primary,on_primary}`,
`color.border.default`, `color.state.{hover,pressed,selected}`, and
`shape.surface.radius`. Declare only the roles the kit uses. Unrecognized
tokens, component contracts, geometry, fonts, and content stay package-owned.

Native measured buttons have a separate hit area, surface and label. This
fixture declares all three, using the standard upstream renderer. Rinx wraps
the measured tree in the upstream scroll viewport to place it inside the host
panel. No generated color strings or package bytes are rewritten.

Classic L0 palettes keep their existing rendering until migrated to the semantic
native-kit contract. See [ADR 0009](../../../docs/adr/0009-shared-reloadable-themes.md).
