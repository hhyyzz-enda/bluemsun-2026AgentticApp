//! The manifest's growth rules within 1.x (ADR 0005 §2): unknown fields are
//! refused at this build's minor and ignored (and reported) in a manifest
//! written for a newer 1.x, a required feature this build does not know
//! refuses the app at every minor, and the two new fields leave older
//! manifests' signing bytes alone.
use octosense_app_contract::*;

fn manifest_with(body: &str) -> String {
    format!(
        r#"{{"schema":1,"id":"forecast","version":"1.0.0","name":"Forecast","integrity":{{"bundle_blake3":"00"}}{}}}"#,
        if body.is_empty() { String::new() } else { format!(",{body}") }
    )
}

#[test]
fn palpo_extension_preserves_schema_and_declares_its_host_feature() {
    assert_eq!(SCHEMA, 1);
    assert_eq!(SCHEMA_MINOR, 0);
    assert_eq!(KNOWN_FEATURES, &["palpo-admin-v1"]);
}

#[test]
fn palpo_uses_exact_service_grants() {
    let services = serde_json::to_string(octosense_app_contract::palpo::SERVICES).unwrap();
    let manifest = parse(&manifest_with(&format!(
        r#""requires":["palpo-admin-v1"],"capabilities":{services}"#
    ))).unwrap();
    let limits = HostLimits::system().with_require_signature(false);
    let resolved = resolve(&manifest, &limits).unwrap();
    for service in octosense_app_contract::palpo::SERVICES {
        assert!(resolved.allows(service));
    }
    assert!(!resolved.allows("palpo.*"));
    assert!(!resolved.allows("palpo.users.delete"));
    let unknown = parse(&manifest_with(r#""capabilities":["palpo.users.delete"]"#)).unwrap();
    assert!(resolve(&unknown, &limits).is_err());
}

#[test]
fn an_unknown_required_feature_needs_a_newer_host() {
    let err = parse(&manifest_with(r#""requires":["storage.encrypted"]"#)).unwrap_err();
    assert_eq!(err, "app forecast needs a newer host: storage.encrypted");
    let err = parse(&manifest_with(r#""requires":["a","b"],"schema_minor":2"#)).unwrap_err();
    assert!(err.ends_with("needs a newer host: a, b"), "{err}");
}

#[test]
fn resolve_checks_requires_even_for_a_manifest_not_read_by_parse() {
    let manifest: AppManifest = serde_json::from_str(&manifest_with(r#""requires":["x"]"#)).unwrap();
    let limits = HostLimits::default().with_require_signature(false);
    let err = resolve(&manifest, &limits).unwrap_err();
    assert!(err.contains("needs a newer host: x"), "{err}");
}

#[test]
fn an_empty_requires_and_a_newer_minor_are_accepted() {
    let manifest = parse(&manifest_with(r#""requires":[],"schema_minor":3"#)).unwrap();
    assert!(manifest.requires.is_empty());
    assert_eq!(manifest.schema_minor, 3);
}

#[test]
fn unknown_fields_are_still_refused() {
    let err = parse(&manifest_with(r#""sandbox":"off""#)).unwrap_err();
    assert!(err.contains("unknown field"), "{err}");
    // At this build's own minor, said or not, the read is strict.
    let err = parse(&manifest_with(r#""schema_minor":0,"sandbox":"off""#)).unwrap_err();
    assert!(err.contains("unknown field"), "{err}");
    let err = parse(&manifest_with(r#""schema_minor":0,"network":{"hosts":[],"proxy":"x"}"#)).unwrap_err();
    assert!(err.contains("unknown field `proxy`"), "{err}");
}

#[test]
fn the_new_fields_do_not_change_an_older_manifests_signing_bytes() {
    let plain = parse(&manifest_with("")).unwrap();
    let explicit = parse(&manifest_with(r#""requires":[],"schema_minor":0"#)).unwrap();
    assert_eq!(plain.signing_bytes().unwrap(), explicit.signing_bytes().unwrap());
    let bytes = String::from_utf8(plain.signing_bytes().unwrap()).unwrap();
    assert!(!bytes.contains("requires") && !bytes.contains("schema_minor"), "{bytes}");
    let minor = parse(&manifest_with(r#""schema_minor":1"#)).unwrap();
    assert!(String::from_utf8(minor.signing_bytes().unwrap()).unwrap().contains(r#""schema_minor":1"#));
}

#[test]
fn a_newer_minor_ignores_an_unknown_optional_field_and_reports_it() {
    let manifest = parse(&manifest_with(r#""schema_minor":1,"capabilities":["storage"],"theme_color":"slate""#)).unwrap();
    assert_eq!(manifest.ignored_fields(), ["theme_color"]);
    assert_eq!(manifest.capabilities, ["storage"]);
    let limits = HostLimits::default().with_require_signature(false);
    assert!(resolve(&manifest, &limits).unwrap().allows("storage"));
}

#[test]
fn a_newer_minor_with_an_unknown_required_feature_is_refused() {
    let err = parse(&manifest_with(r#""schema_minor":2,"requires":["net.proxy"],"network":{"hosts":[],"proxy":"p.example"}"#))
        .unwrap_err();
    assert_eq!(err, "app forecast needs a newer host: net.proxy");
    // Without unknown fields, the strict read reaches the same refusal.
    let err = parse(&manifest_with(r#""schema_minor":2,"requires":["net.proxy"]"#)).unwrap_err();
    assert_eq!(err, "app forecast needs a newer host: net.proxy");
}

#[test]
fn a_newer_minor_ignores_unknown_fields_at_every_level() {
    let manifest = parse(&manifest_with(
        r#""schema_minor":3,
           "capabilities":["net","research"],
           "network":{"hosts":["api.weather.example"],"retry":2},
           "storage":{"max_bytes":4096,"backup":"never"},
           "compute":{"memory_bytes":1024,"gpu":false},
           "research":{"langs":["en"],"safe_search":true},
           "integrity_extra":1,
           "agent":{"profile":"read-only","voice":"calm",
                    "model":{"needs":["tool_calling"],"temperature":0.2,
                             "per_task":{"triage":{"tier":"fast","budget":5}}},
                    "triggers":{"schedule":["0 7 * * *"],"geofence":"home"}}"#,
    ))
    .unwrap();
    assert_eq!(
        manifest.ignored_fields(),
        [
            "agent.model.per_task.triage.budget",
            "agent.model.temperature",
            "agent.triggers.geofence",
            "agent.voice",
            "compute.gpu",
            "integrity_extra",
            "network.retry",
            "research.safe_search",
            "storage.backup",
        ]
    );
    // What the build knows is read as written.
    assert_eq!(manifest.network.hosts, ["api.weather.example"]);
    assert_eq!(manifest.storage.max_bytes, Some(4096));
    assert_eq!(manifest.research.as_ref().unwrap().langs, ["en"]);
    assert_eq!(manifest.agent.as_ref().unwrap().model.as_ref().unwrap().per_task["triage"].tier, ModelTier::Fast);
}

#[test]
fn a_newer_minor_still_refuses_what_it_knows_to_be_wrong() {
    // Ignoring is for unknown fields only: a known field with a bad value,
    // a foreign schema and the old research shape are refused as before.
    let err = parse(&manifest_with(r#""schema_minor":1,"storage":{"agent_workspace":"everything"}"#)).unwrap_err();
    assert!(err.contains("manifest is not valid"), "{err}");
    let err = parse(&manifest_with(r#""schema_minor":1,"x":1"#).replace(r#""schema":1"#, r#""schema":2"#)).unwrap_err();
    assert!(err.contains("manifest is not valid"), "{err}");
    let err = parse(&manifest_with(r#""schema_minor":1,"capabilities":["research"],"research":{"languages":["en"]}"#)).unwrap_err();
    assert!(err.contains("old toolbox shape"), "{err}");
}

#[test]
fn a_newer_manifest_signs_with_the_fields_it_was_signed_with() {
    let manifest = parse(&manifest_with(r#""schema_minor":1,"theme_color":"blue","network":{"hosts":[],"retry":2}"#)).unwrap();
    let bytes = String::from_utf8(manifest.signing_bytes().unwrap()).unwrap();
    assert!(bytes.contains(r#""theme_color":"blue""#), "{bytes}");
    assert!(bytes.contains(r#""network":{"hosts":[],"retry":2}"#), "{bytes}");
}

#[test]
fn hostlimits_builders_set_each_ceiling() {
    let limits = HostLimits::system()
        .with_require_signature(true)
        .with_max_storage_bytes(1)
        .with_max_instruction_budget(2)
        .with_max_memory_bytes(3)
        .with_max_iterations(4)
        .with_max_token_budget(5)
        .with_offered_tools(["net.fetch"]);
    assert!(limits.require_signature);
    assert_eq!(
        (limits.max_storage_bytes, limits.max_instruction_budget, limits.max_memory_bytes, limits.max_iterations, limits.max_token_budget),
        (1, 2, 3, 4, 5)
    );
    assert_eq!(limits.offered_tools, ["net.fetch"]);
}
