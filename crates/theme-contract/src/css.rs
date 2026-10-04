//! CSS variables for host-owned HTML only. Never inject this into third-party sites.
use super::*;

pub fn variables(tokens: &Tokens) -> Result<String, String> {
    validate_resolved(tokens)?;
    let mut css = String::from(":root{\n");
    for (role, token) in tokens {
        let value = match token.kind.as_str() {
            "color" => format!("#{}", hex(token)?),
            "number" => token.value.to_string(),
            "dimension" => format!("{}px", token.value["value"]),
            "duration" => format!("{}ms", token.value["value"]),
            "fontFamily" if token.value == "mono" => "ui-monospace,monospace".into(),
            "fontFamily" => "system-ui,sans-serif".into(),
            _ => unreachable!("validated token type"),
        };
        css.push_str(&format!("--octo-{}:{value};\n", role.replace('.', "-")));
    }
    css.push('}');
    Ok(css)
}
