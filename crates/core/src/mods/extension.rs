//! Extensions: `extension.toml` and `main.js`.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value as Json;

use super::files::read_text;
use super::manifest::{self, ExtensionManifest};
use super::{ModIssue, css};

pub const MANIFEST: &str = "extension.toml";
pub const SCRIPT: &str = "main.js";

const MAX_MANIFEST: u64 = 64 * 1024;
const MAX_SCRIPT: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Extension {
    pub manifest: ExtensionManifest,
    pub code: String,
    /// Setting values chosen in spacetify (`config.toml`), already checked
    /// against the manifest. Filled by [`super::ModStore::active`].
    pub configured: BTreeMap<String, Json>,
}

pub fn load(dir: &Path) -> Result<Extension, ModIssue> {
    let manifest = manifest::parse_extension(&read_text(dir, MANIFEST, MAX_MANIFEST)?)?;
    for selector in &manifest.selectors {
        // Catches unclosed brackets or quotes, which a browser would reject.
        css::validate("extension.toml", &format!("{selector} {{}}"))
            .map_err(|_| ModIssue::InvalidSelector(selector.clone()))?;
        if selector.matches('[').count() != selector.matches(']').count()
            || selector.matches('(').count() != selector.matches(')').count()
        {
            return Err(ModIssue::InvalidSelector(selector.clone()));
        }
    }
    let code = read_text(dir, SCRIPT, MAX_SCRIPT)?;
    Ok(Extension {
        manifest,
        code,
        configured: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const MANIFEST_TEXT: &str = "id = \"hello\"\nname = \"Hello\"\nversion = \"0.1.0\"\n\
                                 author = \"Me\"\nspacetify = \">=0.2.0\"\n";

    #[test]
    fn loads_an_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), MANIFEST_TEXT).unwrap();
        assert_eq!(
            load(dir.path()).unwrap_err(),
            ModIssue::MissingFile(SCRIPT.into())
        );
        fs::write(dir.path().join(SCRIPT), "spacetify.log.info('hi');").unwrap();
        let extension = load(dir.path()).unwrap();
        assert_eq!(extension.manifest.meta.id, "hello");
        assert!(extension.code.contains("hi"));
    }

    #[test]
    fn refuses_unbalanced_selectors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(SCRIPT), "").unwrap();
        for selector in ["[data-testid='x'", "div:not(.a", "[title=\"x]"] {
            let manifest = format!("{MANIFEST_TEXT}selectors = [{selector:?}]\n");
            fs::write(dir.path().join(MANIFEST), manifest).unwrap();
            assert_eq!(
                load(dir.path()).unwrap_err(),
                ModIssue::InvalidSelector(selector.into()),
                "{selector}"
            );
        }
    }
}
