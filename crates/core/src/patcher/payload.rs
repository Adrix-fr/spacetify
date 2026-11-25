//! What gets injected. For phase 1 it is only a proof that injection works:
//! a badge, a new accent colour and a line in the console.

use super::{ASSET_DIR, MARKER_PATH, Marker};

const INJECT_JS: &str = include_str!("../../assets/inject.js");
const INJECT_CSS: &str = include_str!("../../assets/inject.css");

/// Files added to the archive, as `(path, content)`.
pub(super) fn files(marker: &Marker) -> Vec<(String, Vec<u8>)> {
    let marker = serde_json::to_vec_pretty(marker).expect("marker is always serializable");
    vec![
        (
            format!("{ASSET_DIR}inject.css"),
            INJECT_CSS.as_bytes().to_vec(),
        ),
        (
            format!("{ASSET_DIR}inject.js"),
            INJECT_JS.as_bytes().to_vec(),
        ),
        (MARKER_PATH.to_owned(), marker),
    ]
}

/// One script for live injection: adds the stylesheet itself, then runs the
/// same code as the file-based method.
pub(super) fn live_script() -> String {
    let css = serde_json::to_string(INJECT_CSS).expect("a string is always serializable");
    format!(
        r#"(() => {{
  const addStyle = () => {{
    if (document.getElementById("spacetify-style")) return;
    const style = document.createElement("style");
    style.id = "spacetify-style";
    style.textContent = {css};
    document.head.appendChild(style);
  }};
  if (document.head) addStyle();
  else document.addEventListener("DOMContentLoaded", addStyle, {{ once: true }});
}})();
{INJECT_JS}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_script_embeds_css_and_js() {
        let script = live_script();
        assert!(script.contains("spacetify-badge"));
        assert!(script.contains("--essential-bright-accent"));
    }
}
