//! Light CSS checks. Not a full parser: enough to refuse files that would
//! break the rest of the stylesheet, and to find the parts `doctor` needs.

use super::ModIssue;

/// Refuses unclosed comments, strings or braces, and `@import`.
pub fn validate(file: &str, css: &str) -> Result<(), ModIssue> {
    let mut depth = 0usize;
    let mut line = 1;
    let mut chars = css.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '\n' => line += 1,
            '/' if chars.peek().is_some_and(|&(_, n)| n == '*') => {
                let start = line;
                chars.next();
                let mut closed = false;
                while let Some((_, c)) = chars.next() {
                    if c == '\n' {
                        line += 1;
                    } else if c == '*' && chars.peek().is_some_and(|&(_, n)| n == '/') {
                        chars.next();
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(ModIssue::CssUnclosedComment {
                        file: file.into(),
                        line: start,
                    });
                }
            }
            '"' | '\'' => {
                let mut closed = false;
                while let Some((_, n)) = chars.next() {
                    match n {
                        '\\' => {
                            chars.next();
                        }
                        '\n' => break,
                        _ if n == c => {
                            closed = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !closed {
                    return Err(ModIssue::CssUnclosedString {
                        file: file.into(),
                        line,
                    });
                }
            }
            '\\' => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return Err(ModIssue::CssUnbalanced {
                        file: file.into(),
                        line,
                    });
                }
                depth -= 1;
            }
            '@' if css[at + 1..]
                .get(..6)
                .is_some_and(|word| word.eq_ignore_ascii_case("import")) =>
            {
                return Err(ModIssue::CssImport {
                    file: file.into(),
                    line,
                });
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(ModIssue::CssUnbalanced {
            file: file.into(),
            line,
        });
    }
    Ok(())
}

/// Replaces comments with spaces, keeping strings and line breaks.
pub fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    let mut quote = None;
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                out.push(c);
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                out.push(c);
            }
            None if c == '/' && chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                    }
                    if previous == '*' && n == '/' {
                        break;
                    }
                    previous = n;
                }
                out.push(' ');
            }
            None => out.push(c),
        }
    }
    out
}

/// Selector lists of every style rule, at any nesting level. At-rule
/// preludes such as `@media (…)` are skipped.
pub fn selectors(css: &str) -> Vec<String> {
    let css = strip_comments(css);
    let mut found = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    for (at, c) in css.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, c) {
            (_, '\\') => escaped = true,
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '{') => {
                let prelude = css[start..at].trim();
                if !prelude.is_empty() && !prelude.starts_with('@') {
                    found.push(prelude.to_owned());
                }
                start = at + 1;
            }
            (None, '}' | ';') => start = at + 1,
            _ => {}
        }
    }
    found
}

/// Custom properties read with `var(--name)`.
pub fn variables_used(css: &str) -> Vec<String> {
    let css = strip_comments(css);
    let mut found = Vec::new();
    let mut rest = css.as_str();
    while let Some(at) = rest.find("var(") {
        let after = rest[at + 4..].trim_start();
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if name.starts_with("--") && name.len() > 2 {
            found.push(name);
        }
        rest = &rest[at + 4..];
    }
    found
}

/// Custom properties given a value, such as `--name: red`.
pub fn variables_declared(css: &str) -> Vec<String> {
    let css = strip_comments(css);
    let mut found = Vec::new();
    for (at, _) in css.match_indices("--") {
        let before = css[..at].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        let name: String = css[at..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let after = css[at + name.len()..].trim_start();
        if name.len() > 2 && after.starts_with(':') && !css[..at].trim_end().ends_with('(') {
            found.push(name);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(css: &str) -> ModIssue {
        validate("theme.css", css).unwrap_err()
    }

    #[test]
    fn accepts_ordinary_css() {
        let css = r#"/* a } comment */
            @media (min-width: 10px) { .a::after { content: "}"; } }
            [data-testid="now-playing-bar"] { color: var(--text-base) }
            .b\{ { color: red }"#;
        validate("theme.css", css).unwrap();
    }

    #[test]
    fn refuses_broken_css() {
        assert_eq!(
            issue("a { color: red;\n"),
            ModIssue::CssUnbalanced {
                file: "theme.css".into(),
                line: 2
            }
        );
        assert_eq!(
            issue("a {}\n}"),
            ModIssue::CssUnbalanced {
                file: "theme.css".into(),
                line: 2
            }
        );
        assert_eq!(
            issue("\n/* open"),
            ModIssue::CssUnclosedComment {
                file: "theme.css".into(),
                line: 2
            }
        );
        assert_eq!(
            issue("a { content: \"x }"),
            ModIssue::CssUnclosedString {
                file: "theme.css".into(),
                line: 1
            }
        );
        assert_eq!(
            issue("@IMPORT url(x.css);"),
            ModIssue::CssImport {
                file: "theme.css".into(),
                line: 1
            }
        );
    }

    #[test]
    fn finds_selectors_at_every_level() {
        let css = "/* .no { } */ .a, #b > [data-testid=\"c\"] { color: red }\n\
                   @media (x) { .d:hover { top: 0 } }\n\
                   @keyframes k { from { top: 0 } 50% { top: 1px } }";
        assert_eq!(
            selectors(css),
            [".a, #b > [data-testid=\"c\"]", ".d:hover", "from", "50%"]
        );
    }

    #[test]
    fn finds_variables() {
        let css = ".a { --own: 1px; color: var( --text-base ); margin: var(--own) }\n\
                   /* var(--commented) */ .b { width: calc(var(--x, 2px) * 2) }";
        assert_eq!(variables_used(css), ["--text-base", "--own", "--x"]);
        assert_eq!(variables_declared(css), ["--own"]);
    }
}
