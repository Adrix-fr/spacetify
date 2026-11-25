//! What gets injected, built from the active theme and extensions.
//!
//! The same pieces serve both methods:
//! - file method: `spacetify/inject.css` (the theme), `spacetify/inject.js`
//!   (the runtime) and one `spacetify/extensions/<id>.js` per extension;
//! - live method: the runtime, with the theme inside, then one script per
//!   extension.
//!
//! Each extension keeps its own file name (or `sourceURL`), so the runtime
//! can tell which extension an uncaught error comes from.

use serde_json::{Value as Json, json};

use super::{ASSET_DIR, MARKER_PATH, Marker};
use crate::mods::{ActiveMods, Extension};

const RUNTIME: &str = include_str!("../../assets/runtime.js");
const CONFIG_PLACEHOLDER: &str = "/*@spacetify-config*/ null";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    File,
    Live,
}

/// Everything spacetify injects, ready to be written or sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Payload {
    css: String,
    theme: Json,
    extensions: Vec<(String, Json, String)>,
}

impl Payload {
    pub fn new(mods: &ActiveMods) -> Self {
        let theme = mods.theme.as_ref().map_or(Json::Null, |theme| {
            let meta = &theme.manifest.meta;
            json!({ "id": meta.id, "name": meta.name, "version": meta.version.to_string() })
        });
        Self {
            css: mods
                .theme
                .as_ref()
                .map(|theme| theme.css.clone())
                .unwrap_or_default(),
            theme,
            extensions: mods
                .extensions
                .iter()
                .map(|e| (e.manifest.meta.id.clone(), meta_json(e), e.code.clone()))
                .collect(),
        }
    }

    /// Files added to the archive, as `(path, content)`, in a fixed order.
    pub fn files(&self, marker: &Marker) -> Vec<(String, Vec<u8>)> {
        let marker = serde_json::to_vec_pretty(marker).expect("marker is always serializable");
        let mut files = vec![
            (format!("{ASSET_DIR}inject.css"), utf8_with_bom(&self.css)),
            (
                format!("{ASSET_DIR}inject.js"),
                utf8_with_bom(&self.runtime(Mode::File)),
            ),
        ];
        for (id, meta, code) in &self.extensions {
            files.push((
                extension_path(id),
                utf8_with_bom(&wrap_extension(meta, code)),
            ));
        }
        files.push((MARKER_PATH.to_owned(), marker));
        files
    }

    /// Scripts for live injection, to run in this order.
    pub fn live_scripts(&self) -> Vec<String> {
        let mut scripts = vec![format!(
            "{}\n//# sourceURL=spacetify/inject.js\n",
            self.runtime(Mode::Live)
        )];
        for (id, meta, code) in &self.extensions {
            scripts.push(format!(
                "{}\n//# sourceURL={}\n",
                wrap_extension(meta, code),
                extension_path(id)
            ));
        }
        scripts
    }

    fn runtime(&self, mode: Mode) -> String {
        let extensions: Vec<Json> = self
            .extensions
            .iter()
            .map(|(_, meta, _)| meta.clone())
            .collect();
        let config = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "mode": match mode { Mode::File => "file", Mode::Live => "live" },
            "theme": self.theme,
            "css": match mode { Mode::File => Json::Null, Mode::Live => json!(self.css) },
            "extensions": extensions,
        });
        RUNTIME.replacen(CONFIG_PLACEHOLDER, &to_js(&config), 1)
    }
}

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// A BOM makes the browser read the file as UTF-8 whatever the page or the
/// server says, so accents and symbols in mods always show correctly.
fn utf8_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() + 3);
    bytes.extend_from_slice(BOM);
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

fn extension_path(id: &str) -> String {
    format!("{ASSET_DIR}extensions/{id}.js")
}

fn meta_json(extension: &Extension) -> Json {
    let manifest = &extension.manifest;
    let settings: serde_json::Map<String, Json> = manifest
        .settings
        .iter()
        .map(|(name, setting)| (name.clone(), setting.to_json()))
        .collect();
    json!({
        "id": manifest.meta.id,
        "name": manifest.meta.name,
        "version": manifest.meta.version.to_string(),
        "settings": settings,
    })
}

/// The extension's code becomes the body of a function that receives its
/// API. The line break before `});` keeps a trailing `//` comment harmless.
fn wrap_extension(meta: &Json, code: &str) -> String {
    format!(
        "// spacetify extension {id}\n\
         window.__spacetify && window.__spacetify.define({meta}, function (spacetify) {{\n\
         {code}\n\
         }});\n",
        id = meta["id"].as_str().unwrap_or_default(),
        meta = to_js(meta),
    )
}

/// JSON is valid JavaScript; escaping `<` also keeps `</script>` harmless.
fn to_js(value: &Json) -> String {
    value.to_string().replace('<', "\\u003c")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::mods::manifest::{self, ExtensionManifest};

    fn extension(id: &str, code: &str) -> Extension {
        let text = format!(
            "id = \"{id}\"\nname = \"Ext <b>\"\nversion = \"1.2.3\"\nauthor = \"Me\"\nspacetify = \">=0.2.0\"\n"
        );
        let manifest: ExtensionManifest = manifest::parse_extension(&text).unwrap();
        assert_eq!(manifest.settings, BTreeMap::new());
        Extension {
            manifest,
            code: code.into(),
        }
    }

    #[test]
    fn without_mods_there_is_only_the_runtime() {
        let payload = Payload::new(&ActiveMods::default());
        let files = payload.files(&Marker::new("abc"));
        let names: Vec<_> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["spacetify/inject.css", "spacetify/inject.js", MARKER_PATH]
        );
        assert!(
            files
                .iter()
                .take(2)
                .all(|(_, bytes)| bytes.starts_with(BOM))
        );
        let runtime = String::from_utf8(files[1].1.clone()).unwrap();
        assert!(!runtime.contains(CONFIG_PLACEHOLDER));
        assert!(runtime.contains("\"mode\":\"file\""));
        assert_eq!(payload.live_scripts().len(), 1);
    }

    #[test]
    fn each_extension_gets_its_own_file_and_script() {
        let mods = ActiveMods {
            theme: None,
            extensions: vec![extension("one", "a();"), extension("two", "b(); // end")],
        };
        let payload = Payload::new(&mods);
        let files = payload.files(&Marker::new("abc"));
        let two = files
            .iter()
            .find(|(name, _)| name == "spacetify/extensions/two.js")
            .map(|(_, content)| String::from_utf8(content.clone()).unwrap())
            .unwrap();
        assert!(two.contains("b(); // end\n});"));
        assert!(two.contains("\"name\":\"Ext \\u003cb>\""));

        let scripts = payload.live_scripts();
        assert_eq!(scripts.len(), 3);
        assert!(scripts[0].contains("\"mode\":\"live\""));
        assert!(scripts[2].ends_with("//# sourceURL=spacetify/extensions/two.js\n"));
    }

    #[test]
    fn live_runtime_carries_the_theme_css() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("theme.toml"),
            "id = \"t\"\nname = \"T\"\nversion = \"1.0.0\"\nauthor = \"Me\"\nspacetify = \">=0.2.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("theme.css"), ".a { content: \"</style>\" }").unwrap();
        let mods = ActiveMods {
            theme: Some(crate::mods::theme::load(dir.path()).unwrap()),
            extensions: vec![],
        };
        let payload = Payload::new(&mods);
        let live = &payload.live_scripts()[0];
        assert!(live.contains("\\u003c/style>"));
        assert!(live.contains("\"theme\":{\"id\":\"t\""));
        let files = payload.files(&Marker::new("abc"));
        assert!(String::from_utf8_lossy(&files[0].1).contains("</style>"));
    }
}
