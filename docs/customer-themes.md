# Customer themes and shared appearance

Rinx's standalone appearance is now a device preference shared by native pages,
participating mini apps, URL cards, and plain Markdown readers. Open **Settings →
App appearance → Customize appearance** to duplicate a theme, edit it, preview
light/dark variants, apply, undo, reset, import, export, or share a theme file.

An imported file opens for review. Receiving it does not change appearance.
Preview changes the open application without changing startup settings. Cancel
or closing the editor restores the committed selection. Apply saves it; Undo
restores the previous selection. The editor includes simple accent/type/radius
controls and an advanced JSON editor for the complete contract.

Export and Share include the current edited values, even before Preview. Share
sends an ordinary `.octotheme` Matrix attachment to the chosen joined room,
using the SDK's normal encryption path. Clicking such an attachment opens the
same review flow after download/decryption. Theme files confer no app grants.
An App Hub theme marketplace, account sync and organization policies remain
separate ADR scope.

## File format, version 1

The portable implementation is [`octosense-theme-contract`](../crates/theme-contract).
It depends on Serde and BLAKE3, with no Rinx, Makepad, network or filesystem
dependency. Hosts supply their Makepad base and resolve the overlay. See the
small [Ocean Violet sample](../examples/themes/ocean-violet.octotheme).

Files are UTF-8 JSON, limited to 256 KiB, with extension `.octotheme` and MIME
`application/vnd.octosense.theme+json`. They contain:

```json
{
  "schema_version": 1,
  "id": "my-theme",
  "name": "My theme",
  "author": "",
  "base": "makepad/1",
  "required_features": [],
  "tokens": { "typography": { "scale": { "$type": "number", "$value": 1.15 } } },
  "variants": { "light": {}, "dark": {} }
}
```

The `base` identifies the compatible contract, not a copied preset catalog.
Resolution is base → common tokens → selected variant. Groups contain named
tokens with `$type`, `$value`, and optional `$description`. A value such as
`"{color.surface.page}"` refers to another token, including an inherited base
role. This is an explicitly limited [DTCG format subset](https://www.designtokens.org/tr/2025.10/format/),
not an implementation of the entire format or resolver specification.

| Roles | Type and bounds |
| --- | --- |
| `color.surface.page`, `.panel`, `.field` | Opaque sRGB color |
| `color.content.primary`, `.secondary`, `.disabled` | Opaque sRGB color |
| `color.action.primary`, `.on_primary`; `color.border.default` | Opaque sRGB color |
| `color.state.hover`, `.pressed`, `.selected` | Opaque sRGB color |
| `color.status.{success,warning,danger,info}.{foreground,background}` | Opaque sRGB color |
| `color.chat.incoming`, `.outgoing`, `.mention` | Opaque sRGB color |
| `color.code.background`, `.foreground` | Opaque sRGB color |
| `typography.scale` | Number, 0.8–2.0 |
| `typography.family` | `fontFamily`: `system` or `mono` |
| `metrics.spacing` | Number, 0.75–1.5 |
| `shape.surface.radius` | Dimension, 0–24 px |
| `metrics.control.height` | Dimension, 36–64 px; controls also enforce room for enlarged text |
| `metrics.reading.width` | Dimension, 480–1000 px, constrained by the viewport |
| `metrics.page.gutter` | Dimension, 8–48 px |
| `motion.duration` | Duration, 0–500 ms; shared control transitions |

For example, a color value is
`{"colorSpace":"srgb","components":[0.1,0.2,0.3],"alpha":1}`;
a dimension is `{"value":16,"unit":"px"}`. No font URLs, font binaries,
downloaded SVGs, scripts, shaders, or host requests are admitted by v1. The
host's existing icon resources and installed/bundled font families are used.

Both variants are validated before application. Unknown required features,
roles and types; invalid references/cycles; excessive depth/size; unsupported
resources; and out-of-range values are rejected. Normal text and link/state
pairs have a 4.5:1 contrast gate. Theme names/IDs are bounded. Compilers validate
complete resolved data before emitting any trusted stylesheet or CSS output.

## Runtime, host ownership, and recovery

Rinx uses Makepad's complete `StyleSheet` registration and `ScriptReapply`
mechanism. The revision includes stylesheet data and resolved values, so two
themes of the same mode with different accents/fonts/metrics are distinct.
Rust widgets cache appearance per instance and refresh it during reapply.
Retained secondary windows, reader tabs, link cards, reactions, send status,
room summaries and notification popups refresh their existing presentation.

The OctoSense in-process module remains host-owned: it neither loads nor writes
standalone preferences, and local apply/preview is rejected. A versioned
`ResolvedTheme` contains identity, mode, all resolved roles and a BLAKE3 revision.
`theme::host::receive` validates and deduplicates initial/subscription snapshots
for an initialized hosted instance. It provides a receive boundary, **not a
shipped Android/OpenHarmony IPC bridge**. Existing OctoSense modules continue
using the full stylesheet transport; adopting the portable crate in OctoSense's
own dependency graph is a separate integration step.

The contract's Makepad compiler emits `mod.theme.octo_*` semantic properties;
Rinx aliases these as `RINX_*`/`RBX_*`/legacy `COLOR_*`. Its CSS adapter emits
`--octo-*` variables for controlled HTML consumers. It is not injected into
third-party webpages or authored publication HTML.

Standalone storage uses atomic `theme-state.json` replacement, including the
previous selection. A separately synced `theme-rollback.json` records the last
rendered selection until the current apply transaction has received a draw and
action acknowledgment. A restart after an interrupted apply restores that
selection. Corrupt/unsupported active settings fall back to the previous valid
selection or bundled defaults. Preview does not write either selection. The
old `appearance.json` remains readable for older Rinx versions.

System mode currently has desktop detection and an Android configuration event
adapter. Explicit Light/Dark and custom themes remain available elsewhere.
Automatic System mode on iOS/OpenHarmony/Web needs platform adapters; it is not
advertised on those builds yet.

## Mini apps, typography, and documents

Native mini apps use the same shared components. Splash receives the owning
stylesheet on reapply; its heap/state survives. The existing `rinx_theme.reloading()`
guard lets app startup effects avoid replaying during presentation reload.

Native L0 kits opt in through `$token` references. All recognized semantic colors,
radius, typography body/heading/caption sizes, admitted font resources, and
control/gutter/spacing metrics resolve in memory before standard upstream
lowering. The [reference kit](../examples/miniapps/theme-reference/kit/native/light/kit.json)
demonstrates this. Legacy literal palettes/assets retain their authored
appearance; they must explicitly adopt semantic bindings. No installed package
bytes, digests, permissions or grants are rewritten, and no renderer is forked.

URL card titles and descriptions use the same body role as native labels and
desktop chat: 11 Makepad logical font units at 100%, with the title bold.
Metadata uses the shared 9.5 role. Theme scale affects them together. Reading
documents retain a separate reading scale. Plain Markdown inherits the host's
paper, ink, link and code defaults; its upstream code widget is styled with the
semantic code foreground. Authored article themes, saved content and external
website CSS remain document-owned.

New unexplained interface color literals fail `tools/check_theme_literals.py`.
Reviewed exceptions identify brand/avatar content and media overlays locally
with `theme-content:` comments. Transparent shader values are allowed.

## Validation

See [the validation record](theme-validation.md) for commands, binary hashes,
native evidence and unverified platform/integration boundaries. This delivery
extends the [earlier MVP](theme-mvp.md); a macOS fixture result does not establish
device, IME, accessibility or separate-process host correctness.
