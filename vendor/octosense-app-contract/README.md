# octosense-app-contract

The OctoSense app contract: the one small, versioned interface between App
Hub and every app ([OctoSense ADR 0005](https://github.com/OctoSense-org/OctoSense/blob/main/docs/adr/0005-app-contract.md)).
It holds only what an app or a host needs to read, check and run an app
package:

| Part | Items |
| --- | --- |
| Manifest | `AppManifest` and its parts, `SCHEMA`, `MANIFEST_FILE`, `parse` |
| Policy | `policy::resolve`, `HostLimits`, `AppPolicy` (capabilities, network hosts, the storage block as `StorageGrant`, budgets, research scope) |
| Integrity | `digest_dir`, `bundle_digest`, `admit`, `admit_digest`, `SignatureVerifier`, `RefuseAllSignatures` |
| Running a package | `SCRIPT_ENTRY`, `script_source`, `ASSETS_PLACEHOLDER`, `AssetServer`, `StaticAssets`, `rewrite_assets` |

What an app may do is in the contract; how a host sandboxes it is not. Each
host builds its own sandbox (an isolate's settings, an agent session) from
`AppPolicy`, and **may restrict more than `AppPolicy` says, never less.**
App Hub's store, catalog, agents, listings and host services stay in
`octosense-app-policy` and `octosense-app-hub`, which depend on this crate.

```toml
[dependencies]
octosense-app-contract = "1"
```

```rust
use octosense_app_contract::{admit_digest, digest_dir, parse, policy, HostLimits, RefuseAllSignatures, MANIFEST_FILE};

fn open(package: &std::path::Path) -> Result<octosense_app_contract::AppPolicy, String> {
    let manifest = parse(&std::fs::read_to_string(package.join(MANIFEST_FILE)).map_err(|e| e.to_string())?)?;
    admit_digest(&manifest, &digest_dir(package)?, &RefuseAllSignatures)?;
    policy::resolve(&manifest, &HostLimits::default().with_require_signature(false))
}
```

## Stability

Within `1.x` the contract only grows (ADR 0005 section 2):

- **Additive only.** New types, functions, optional manifest fields and enum
  variants. Nothing is removed or renamed, and no existing field, default
  or rule changes meaning. Anything else is `2.0`, decided in an ADR.
- **Every public struct and enum is `#[non_exhaustive]`** (except the unit
  marker `RefuseAllSignatures`), so adding a field or variant is a minor
  change. Build values with the provided constructors instead of struct
  literals, and match enums with a `_` arm:

  ```rust
  use octosense_app_contract::{HostLimits, Signature};
  let limits = HostLimits::default().with_require_signature(false);
  let signature = Signature::new("release", "aabb");
  ```

  Manifests come from `parse`, policies from `policy::resolve`.
- **Unknown manifest fields are classified, not ignored.** A field added in
  `1.x` is *optional* (a host may run the app without it: it only adds
  information or asks for less) or *required* (it restricts or changes
  what the app gets). A manifest that uses a required field lists its
  feature in `requires`. `parse` applies, in order:

  1. `schema` must be `1`, for the whole `1.x` line.
  2. Every `requires` entry must be in `KNOWN_FEATURES`, at every
     `schema_minor`. Otherwise: `app <id> needs a newer host: <feature>`.
     `1.0.0` knows no features.
  3. `schema_minor` (default 0) at most `SCHEMA_MINOR` (0 in `1.0.0`): the
     manifest is read strictly, and an unknown field at any level refuses
     it.
  4. `schema_minor` above `SCHEMA_MINOR`: the manifest was written for a
     newer `1.x`. Its unknown fields, at any level, are optional by rule 2,
     so they are ignored, and `AppManifest::ignored_fields()` lists them
     (`network.retry`, `agent.model.temperature`) for the host to log.
     Known fields are checked as always.

  ```json
  { "schema": 1, "schema_minor": 2, "requires": ["<feature>"], ... }
  ```

  An older host therefore never runs an app under weaker rules than its
  author wrote, and a newer optional field never breaks an older host.
  Ignored fields stay in the manifest's signing bytes, so a newer signed
  manifest still verifies. A field added in `1.x` must be skipped when it
  holds its default, and must serialise exactly as written.
- **Older manifests are unchanged.** `requires` and `schema_minor` are left
  out of the canonical signing bytes when empty, so a manifest signed before
  they existed signs as it did.
- **Behaviour is pinned by fixtures.** [`tests/fixtures/`](tests/fixtures/README.md)
  holds real manifests and packages with their digest, signing bytes,
  ignored fields and resolved `AppPolicy`. Every `1.x` must reproduce all
  of them; the corpus is append-only.

## Checks and releases

App Hub's CI (`.github/workflows/app-contract.yml`) tests this crate on its
own, as crates.io builds it, and runs `cargo semver-checks` against the
latest published version: a non-additive change fails the pull request. It
skips the API diff, saying so, until a first version is published.

A release is a version bump here and in [CHANGELOG.md](CHANGELOG.md),
reviewed by App Hub and one app owner (Rinx), then the manual workflow
`publish-app-contract` (`.github/workflows/publish-app-contract.yml`), which
runs the tests and publishes with the `CARGO_REGISTRY_TOKEN` secret. It
refuses a version that is already on crates.io.

## License

Apache-2.0.
