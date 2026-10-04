//! Versioned appearance data. No Makepad, application, file, network or script dependency.
//! Hosts supply their resolved base; this crate validates and resolves the portable overlay.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub mod makepad;
pub mod css;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_BYTES: usize = 256 * 1024;
pub const EXTENSION: &str = "octotheme";
pub const MIME: &str = "application/vnd.octosense.theme+json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemePackage {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    pub base: String,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default = "empty_object")]
    pub tokens: Value,
    pub variants: Variants,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Variants {
    #[serde(default = "empty_object")]
    pub light: Value,
    #[serde(default = "empty_object")]
    pub dark: Value,
}
fn empty_object() -> Value {
    json!({})
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Token {
    #[serde(rename = "$type")]
    pub kind: String,
    #[serde(rename = "$value")]
    pub value: Value,
    #[serde(
        default,
        rename = "$description",
        skip_serializing_if = "String::is_empty"
    )]
    pub description: String,
}
pub type Tokens = BTreeMap<String, Token>;

pub const COLORS: &[&str] = &[
    "color.surface.page",
    "color.surface.panel",
    "color.surface.field",
    "color.content.primary",
    "color.content.secondary",
    "color.content.disabled",
    "color.action.primary",
    "color.action.on_primary",
    "color.border.default",
    "color.state.hover",
    "color.state.pressed",
    "color.state.selected",
    "color.status.success.foreground",
    "color.status.success.background",
    "color.status.warning.foreground",
    "color.status.warning.background",
    "color.status.danger.foreground",
    "color.status.danger.background",
    "color.status.info.foreground",
    "color.status.info.background",
    "color.chat.incoming",
    "color.chat.outgoing",
    "color.chat.mention",
    "color.code.background",
    "color.code.foreground",
];
pub const NUMBERS: &[(&str, f64, f64)] = &[
    ("typography.scale", 0.8, 2.0),
    ("metrics.spacing", 0.75, 1.5),
];
pub const DIMENSIONS: &[(&str, f64, f64)] = &[
    ("shape.surface.radius", 0., 24.),
    ("metrics.control.height", 36., 64.),
    ("metrics.reading.width", 480., 1000.),
    ("metrics.page.gutter", 8., 48.),
];

impl ThemePackage {
    pub fn blank(name: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: "custom".into(),
            name: name.into(),
            author: String::new(),
            base: "makepad/1".into(),
            required_features: vec![],
            tokens: empty_object(),
            variants: Variants {
                light: empty_object(),
                dark: empty_object(),
            },
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Theme exceeds 256 KB".into());
        }
        let package: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid theme: {e}"))?;
        package.validate_structure()?;
        Ok(package)
    }
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_structure()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("Theme exceeds 256 KB".into());
        }
        Ok(bytes)
    }
    pub fn validate_structure(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION || self.base != "makepad/1" {
            return Err("Unsupported theme schema or base".into());
        }
        if self.id.is_empty()
            || self.id.len() > 80
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(
                "Theme ID must contain 1–80 letters, digits, dots, dashes or underscores".into(),
            );
        }
        if self.name.trim().is_empty()
            || self.name.len() > 160
            || self.author.len() > 160
            || self
                .name
                .chars()
                .chain(self.author.chars())
                .any(char::is_control)
        {
            return Err("Invalid theme name or author".into());
        }
        if !self.required_features.is_empty() {
            return Err("Theme requires unsupported features".into());
        }
        for dark in [false, true] {
            let overrides = self.overrides(dark)?;
            let mut tokens = Tokens::new();
            for name in COLORS {
                tokens.insert((*name).into(), color("000000")?);
            }
            for (name, min, _) in NUMBERS {
                tokens.insert((*name).into(), number(*min));
            }
            for (name, min, _) in DIMENSIONS {
                tokens.insert((*name).into(), dimension(*min));
            }
            tokens.insert(
                "typography.family".into(),
                Token {
                    kind: "fontFamily".into(),
                    value: json!("system"),
                    description: String::new(),
                },
            );
            tokens.insert(
                "motion.duration".into(),
                Token {
                    kind: "duration".into(),
                    value: json!({"value":0,"unit":"ms"}),
                    description: String::new(),
                },
            );
            tokens.extend(overrides.clone());
            for (name, token) in &overrides {
                let resolved = resolve_one(name, &tokens, &mut BTreeSet::new(), 0)?;
                if token.kind != resolved.kind {
                    return Err(format!("Type mismatch in {name}"));
                }
                validate_token(name, &resolved)?;
            }
        }
        Ok(())
    }
    pub fn overrides(&self, dark: bool) -> Result<Tokens, String> {
        let mut result = Tokens::new();
        flatten("", &self.tokens, &mut result, 0)?;
        flatten(
            "",
            if dark {
                &self.variants.dark
            } else {
                &self.variants.light
            },
            &mut result,
            0,
        )?;
        Ok(result)
    }
    /// Bases are supplied by the owning Makepad host, never a copied preset catalog.
    pub fn resolve(&self, dark: bool, base: &Tokens) -> Result<Tokens, String> {
        let mut merged = base.clone();
        merged.extend(self.overrides(dark)?);
        let mut result = Tokens::new();
        for name in merged.keys() {
            let token = resolve_one(name, &merged, &mut BTreeSet::new(), 0)?;
            validate_token(name, &token)?;
            result.insert(name.clone(), token);
        }
        validate_contrast(&result)?;
        Ok(result)
    }
    pub fn set(&mut self, dark: Option<bool>, path: &str, token: Token) {
        let root = match dark {
            None => &mut self.tokens,
            Some(false) => &mut self.variants.light,
            Some(true) => &mut self.variants.dark,
        };
        let mut node = root;
        let parts: Vec<_> = path.split('.').collect();
        for (i, part) in parts.iter().enumerate() {
            if !node.is_object() {
                *node = empty_object()
            }
            if i == parts.len() - 1 {
                node[*part] = serde_json::to_value(&token).unwrap();
                return;
            }
            node = node
                .as_object_mut()
                .unwrap()
                .entry(part.to_string())
                .or_insert_with(empty_object);
        }
    }
}

fn flatten(prefix: &str, node: &Value, into: &mut Tokens, depth: usize) -> Result<(), String> {
    if depth > 12 || into.len() > 128 {
        return Err("Theme token tree is too complex".into());
    }
    let object = node.as_object().ok_or("Theme tokens must be objects")?;
    if object.contains_key("$value") {
        if prefix.is_empty() {
            return Err("A root token requires a name".into());
        }
        let token: Token = serde_json::from_value(node.clone()).map_err(|e| e.to_string())?;
        if token.description.len() > 1000 {
            return Err("Token description is too long".into());
        }
        into.insert(prefix.into(), token);
    } else {
        for (name, value) in object {
            if name.is_empty()
                || name.len() > 80
                || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return Err(format!("Unsupported token group: {name}"));
            }
            flatten(
                &if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                },
                value,
                into,
                depth + 1,
            )?;
        }
    }
    Ok(())
}
fn resolve_one(
    name: &str,
    tokens: &Tokens,
    visited: &mut BTreeSet<String>,
    depth: usize,
) -> Result<Token, String> {
    if depth > 64 || !visited.insert(name.into()) {
        return Err(format!("Cyclic theme reference: {name}"));
    }
    let token = tokens
        .get(name)
        .ok_or_else(|| format!("Missing theme reference: {name}"))?;
    if let Some(reference) = token
        .value
        .as_str()
        .and_then(|s| s.strip_prefix('{'))
        .and_then(|s| s.strip_suffix('}'))
    {
        let target = resolve_one(reference, tokens, visited, depth + 1)?;
        if token.kind != target.kind {
            return Err(format!("Reference type mismatch: {name}"));
        }
        Ok(target)
    } else {
        Ok(token.clone())
    }
}
pub fn color(hex: &str) -> Result<Token, String> {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    if digits.len() != 6 || !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Colors use six hexadecimal digits, for example #09616f".into());
    }
    let n = u32::from_str_radix(digits, 16).map_err(|e| e.to_string())?;
    Ok(Token {
        kind: "color".into(),
        value: json!({"colorSpace":"srgb","components":[((n>>16)&255) as f64/255.,((n>>8)&255) as f64/255.,(n&255) as f64/255.],"alpha":1}),
        description: String::new(),
    })
}
pub fn rgb(token: &Token) -> Result<[f64; 3], String> {
    let v = token
        .value
        .as_object()
        .ok_or("Expected an sRGB color object")?;
    if token.kind != "color"
        || v.get("colorSpace").and_then(Value::as_str) != Some("srgb")
        || v.keys()
            .any(|k| !["colorSpace", "components", "alpha"].contains(&k.as_str()))
        || v.get("alpha").map_or(false, |a| a.as_f64() != Some(1.))
    {
        return Err("Interface colors must be opaque sRGB".into());
    }
    let a = v
        .get("components")
        .and_then(Value::as_array)
        .ok_or("Missing color components")?;
    if a.len() != 3 {
        return Err("Expected three sRGB components".into());
    }
    let mut out = [0.; 3];
    for i in 0..3 {
        out[i] = a[i]
            .as_f64()
            .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
            .ok_or("Invalid sRGB component")?
    }
    Ok(out)
}
pub fn hex(token: &Token) -> Result<String, String> {
    let [r, g, b] = rgb(token)?;
    Ok(format!(
        "{:02x}{:02x}{:02x}",
        (r * 255.).round() as u8,
        (g * 255.).round() as u8,
        (b * 255.).round() as u8
    ))
}
pub fn number(value: f64) -> Token {
    Token {
        kind: "number".into(),
        value: json!(value),
        description: String::new(),
    }
}
pub fn dimension(value: f64) -> Token {
    Token {
        kind: "dimension".into(),
        value: json!({"value":value,"unit":"px"}),
        description: String::new(),
    }
}
fn validate_token(name: &str, token: &Token) -> Result<(), String> {
    if COLORS.contains(&name) {
        rgb(token)?;
        return Ok(());
    }
    if let Some((_, min, max)) = NUMBERS.iter().find(|(key, _, _)| *key == name) {
        if token.kind == "number"
            && token
                .value
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= *min && n <= *max)
        {
            return Ok(());
        }
    } else if let Some((_, min, max)) = DIMENSIONS.iter().find(|(key, _, _)| *key == name) {
        if token.kind == "dimension"
            && token.value["unit"] == "px"
            && token.value["value"]
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= *min && n <= *max)
            && token.value.as_object().is_some_and(|v| v.len() == 2)
        {
            return Ok(());
        }
    } else if name == "typography.family" {
        if token.kind == "fontFamily" && matches!(token.value.as_str(), Some("system" | "mono")) {
            return Ok(());
        }
    } else if name == "motion.duration" {
        if token.kind == "duration"
            && token.value["unit"] == "ms"
            && token.value["value"]
                .as_f64()
                .is_some_and(|n| (0.0..=500.).contains(&n))
            && token.value.as_object().is_some_and(|v| v.len() == 2)
        {
            return Ok(());
        }
    } else {
        return Err(format!("Unsupported theme role: {name}"));
    }
    Err(format!("Invalid type or range for {name}"))
}
pub fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let lum = |v: [f64; 3]| {
        let linear = |c: f64| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(v[0]) + 0.7152 * linear(v[1]) + 0.0722 * linear(v[2])
    };
    let (a, b) = (lum(a), lum(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
pub fn validate_contrast(tokens: &Tokens) -> Result<(), String> {
    let mut pairs = vec![
        ("color.action.on_primary", "color.action.primary", 4.5),
        ("color.action.primary", "color.surface.page", 4.5),
        ("color.action.primary", "color.surface.panel", 4.5),
        ("color.action.primary", "color.state.selected", 4.5),
        ("color.code.foreground", "color.code.background", 4.5),
    ];
    for ink in ["color.content.primary", "color.content.secondary"] {
        for bg in [
            "color.surface.page",
            "color.surface.panel",
            "color.surface.field",
            "color.state.hover",
            "color.state.pressed",
            "color.state.selected",
            "color.chat.incoming",
            "color.chat.outgoing",
            "color.chat.mention",
        ] {
            pairs.push((ink, bg, 4.5))
        }
    }
    for (fg, bg) in [
        (
            "color.status.success.foreground",
            "color.status.success.background",
        ),
        (
            "color.status.warning.foreground",
            "color.status.warning.background",
        ),
        (
            "color.status.danger.foreground",
            "color.status.danger.background",
        ),
        (
            "color.status.info.foreground",
            "color.status.info.background",
        ),
    ] {
        pairs.push((fg, bg, 4.5))
    }
    for (fg, bg, min) in pairs {
        if let (Some(a), Some(b)) = (tokens.get(fg), tokens.get(bg)) {
            let ratio = contrast(rgb(a)?, rgb(b)?);
            if ratio + 0.001 < min {
                return Err(format!(
                    "{fg} on {bg} has {ratio:.2}:1 contrast; {min}:1 is required"
                ));
            }
        }
    }
    Ok(())
}

/// Transport payload for hosts/process adapters; revision covers all values, not just mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedTheme {
    pub schema_version: u32,
    pub identity: String,
    pub dark: bool,
    pub revision: String,
    pub tokens: Tokens,
}
impl ResolvedTheme {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Theme snapshot exceeds 256 KB".into());
        }
        let snapshot: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        snapshot.validate()?;
        Ok(snapshot)
    }
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("Theme snapshot exceeds 256 KB".into());
        }
        Ok(bytes)
    }
    pub fn new(identity: String, dark: bool, tokens: Tokens) -> Self {
        let material = serde_json::to_vec(&(SCHEMA_VERSION, &identity, dark, &tokens)).unwrap();
        Self {
            schema_version: SCHEMA_VERSION,
            identity,
            dark,
            revision: blake3::hash(&material).to_hex().to_string(),
            tokens,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION
            || self.revision
                != Self::new(self.identity.clone(), self.dark, self.tokens.clone()).revision
        {
            return Err("Invalid theme snapshot revision".into());
        }
        if self.identity.is_empty()
            || self.identity.len() > 160
            || self.identity.chars().any(char::is_control)
        {
            return Err("Invalid theme identity".into());
        }
        validate_resolved(&self.tokens)
    }
}

/// Validate a complete snapshot before any renderer emits presentation code.
pub fn validate_resolved(tokens: &Tokens) -> Result<(), String> {
    for role in COLORS
        .iter()
        .copied()
        .chain(NUMBERS.iter().map(|(k, _, _)| *k))
        .chain(DIMENSIONS.iter().map(|(k, _, _)| *k))
        .chain(["typography.family", "motion.duration"])
    {
        if !tokens.contains_key(role) {
            return Err(format!("Missing required theme role: {role}"));
        }
    }
    for (name, token) in tokens {
        validate_token(name, token)?
    }
    validate_contrast(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn complete() -> Tokens {
        let mut tokens = Tokens::new();
        for role in COLORS {
            let hex = if role.ends_with(".foreground") || role.starts_with("color.content.") {
                "000000"
            } else if *role == "color.action.primary" {
                "005555"
            } else {
                "ffffff"
            };
            tokens.insert((*role).into(), color(hex).unwrap());
        }
        for (key, min, _) in NUMBERS {
            tokens.insert((*key).into(), number(*min));
        }
        for (key, min, _) in DIMENSIONS {
            tokens.insert((*key).into(), dimension(*min));
        }
        tokens.insert(
            "typography.family".into(),
            Token {
                kind: "fontFamily".into(),
                value: json!("system"),
                description: String::new(),
            },
        );
        tokens.insert(
            "motion.duration".into(),
            Token {
                kind: "duration".into(),
                value: json!({"value":150,"unit":"ms"}),
                description: String::new(),
            },
        );
        validate_resolved(&tokens).unwrap();
        tokens
    }
    #[test]
    fn renderers_reject_partial_or_untrusted_values_without_partial_output() {
        let mut output = "base".to_string();
        assert!(makepad::append_makepad(&mut output, &Tokens::new()).is_err());
        assert_eq!(output, "base");
        let mut tokens = complete();
        tokens.get_mut("typography.family").unwrap().value = json!("mono; host.request('network')");
        assert!(makepad::append_makepad(&mut output, &tokens).is_err());
        assert!(css::variables(&tokens).is_err());
        assert_eq!(output, "base");
    }
    #[test]
    fn compiler_uses_float_shader_metrics_and_typed_css() {
        let mut tokens = complete();
        tokens.insert("shape.surface.radius".into(), dimension(10.));
        let mut output = String::new();
        makepad::append_makepad(&mut output, &tokens).unwrap();
        assert!(output.contains("corner_radius = 10.0\n"));
        let css = css::variables(&tokens).unwrap();
        assert!(css.contains("--octo-shape-surface-radius:10.0px;"));
        assert!(css.contains("--octo-color-content-primary:#000000;"));
    }
    #[test]
    fn snapshot_roundtrip_revision_and_tamper_detection() {
        let snapshot = ResolvedTheme::new("Customer 海洋".into(), false, complete());
        assert_eq!(
            ResolvedTheme::parse(&snapshot.bytes().unwrap()).unwrap(),
            snapshot
        );
        let mut changed = snapshot.clone();
        changed
            .tokens
            .insert("typography.scale".into(), number(1.4));
        assert!(changed.validate().is_err());
        assert_ne!(
            snapshot.revision,
            ResolvedTheme::new(snapshot.identity, false, changed.tokens).revision
        );
    }
    #[test]
    fn references_can_inherit_base_roles_but_not_change_types() {
        let mut p = ThemePackage::blank("Alias");
        p.set(
            None,
            "color.surface.field",
            Token {
                kind: "color".into(),
                value: json!("{color.surface.page}"),
                description: String::new(),
            },
        );
        ThemePackage::parse(&p.bytes().unwrap()).unwrap();
        let resolved = p.resolve(false, &complete()).unwrap();
        assert_eq!(
            resolved["color.surface.field"],
            resolved["color.surface.page"]
        );
        p.set(
            None,
            "metrics.reading.width",
            Token {
                kind: "dimension".into(),
                value: json!("{color.surface.page}"),
                description: String::new(),
            },
        );
        assert!(p.validate_structure().is_err());
    }
    #[test]
    fn portable_roundtrip_and_reference_resolution() {
        let mut p = ThemePackage::blank("海洋");
        p.set(None, "color.surface.page", color("ffffff").unwrap());
        p.set(
            None,
            "color.surface.panel",
            Token {
                kind: "color".into(),
                value: json!("{color.surface.page}"),
                description: String::new(),
            },
        );
        let p = ThemePackage::parse(&p.bytes().unwrap()).unwrap();
        let r = p.resolve(false, &Tokens::new()).unwrap();
        assert_eq!(hex(&r["color.surface.panel"]).unwrap(), "ffffff");
        let a = ResolvedTheme::new(p.id.clone(), false, r.clone());
        assert!(a.validate().is_err());
        let mut t = r;
        t.insert("typography.scale".into(), number(1.2));
        assert_ne!(a.revision, ResolvedTheme::new(p.id, false, t).revision);
    }
    #[test]
    fn rejects_cycles_unknown_features_scripts_resources_and_bad_ranges() {
        let mut p = ThemePackage::blank("Test");
        p.set(
            None,
            "color.surface.page",
            Token {
                kind: "color".into(),
                value: json!("{color.surface.page}"),
                description: String::new(),
            },
        );
        assert!(p.bytes().unwrap_err().contains("Cyclic"));
        for (name, token) in [
            ("typography.scale", number(100.)),
            ("script", number(1.)),
            (
                "typography.family",
                Token {
                    kind: "fontFamily".into(),
                    value: json!("../../evil.ttf"),
                    description: String::new(),
                },
            ),
        ] {
            let mut p = ThemePackage::blank("Test");
            p.set(None, name, token);
            assert!(p.bytes().is_err());
        }
        let mut p = ThemePackage::blank("Test");
        p.required_features.push("execute".into());
        assert!(p.bytes().is_err());
        assert!(ThemePackage::parse(&vec![0; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn rejects_bad_contrast_and_keeps_variants_separate() {
        let mut p = ThemePackage::blank("Test");
        p.set(Some(false), "color.surface.page", color("ffffff").unwrap());
        p.set(
            Some(false),
            "color.content.primary",
            color("eeeeee").unwrap(),
        );
        assert!(p.resolve(false, &Tokens::new()).is_err());
        assert!(p.resolve(true, &Tokens::new()).unwrap().is_empty());
    }
}
