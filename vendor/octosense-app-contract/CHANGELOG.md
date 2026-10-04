# Changelog

`octosense-app-contract` follows the rules in [README.md](README.md#stability):
within `1.x` it only grows.

## 1.1.0

`RESERVED_NAMES` lists the native apps OctoSense ships (its
`native-apps.json`), so it grows with them; the rule itself (an id or a
namespace on the list is refused) is unchanged. A manifest that 1.0.0
admitted with one of the new names as its id or namespace is refused.

- `RESERVED_NAMES` gains `calculator`, `clock`, `notes`, `reminders` and
  `weather`: OctoSense ships those Makepad apps as native apps, and grants
  their read tools (`notes.search`, …) to its system agent by name. A store
  app with one of those ids, or one as its namespace (`com.example.notes`),
  is now refused as reserved. The tests' example app is `forecast` (it was
  `weather`).
- `RESERVED_NAMES` gains `browser` and `task`: OctoSense's desktop-only
  native Browser and Task Manager.

## 1.0.0

The contract as OctoSense ADR 0005 section 1 defines it, moved out of App
Hub's `octosense-app-policy` with its behaviour unchanged:

- Manifest: `AppManifest` and its parts, `SCHEMA`, `MANIFEST_FILE`, `parse`.
- Policy: `policy::resolve`, `HostLimits`, `AppPolicy` (what the app may
  do: capabilities, hosts, storage, budgets, research scope). The app's
  agent is resolved by hosts that run agents, not here.
- Integrity: `digest_dir`, `bundle_digest`, `admit`, `admit_digest`,
  `SignatureVerifier`, `RefuseAllSignatures`.
- Running a package: `SCRIPT_ENTRY`, `script_source`, `ASSETS_PLACEHOLDER`,
  `AssetServer`, `StaticAssets`, `rewrite_assets`.

New in the contract:

- `requires` and `schema_minor` in the manifest, `KNOWN_FEATURES` (empty)
  and `SCHEMA_MINOR` (0): a manifest requiring an unknown feature is refused
  ("needs a newer host"); a manifest for a newer `1.x` is read with its
  unknown (optional) fields ignored and listed by
  `AppManifest::ignored_fields`.
- Every public struct and enum is `#[non_exhaustive]` (except the unit
  marker `RefuseAllSignatures`); `HostLimits` gains `with_*` builders and
  `Signature` gains `new`.
- `AppPolicy` carries the whole storage block (`StorageGrant`) and
  serialises.
- The fixture corpus (`tests/fixtures/`).
