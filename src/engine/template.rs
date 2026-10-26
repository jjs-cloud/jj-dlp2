//! Splits command strings into arguments and fills in `{placeholders}`.

/// Splits on whitespace, honouring "double" and 'single' quotes; backslashes stay literal.
pub fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else if c == '\\' && q == '"' && chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    in_token = true;
                } else if c.is_whitespace() {
                    if in_token {
                        out.push(std::mem::take(&mut cur));
                        in_token = false;
                    }
                } else {
                    cur.push(c);
                    in_token = true;
                }
            }
        }
    }
    if in_token {
        out.push(cur);
    }
    out
}

/// Builds the argument list; a bare `{key}` token expands to one argument per value, and the values are appended if `{key}` is absent.
pub fn expand(template: &str, key: &str, values: &[String]) -> Vec<String> {
    let bare = format!("{{{key}}}");
    let mut args = Vec::new();
    let mut used = false;
    for tok in tokenize(template) {
        if tok == bare {
            args.extend(values.iter().cloned());
            used = true;
        } else if tok.contains(&bare) {
            args.push(tok.replace(&bare, &values.join(" ")));
            used = true;
        } else {
            args.push(tok);
        }
    }
    if !used {
        args.extend(values.iter().cloned());
    }
    args
}

/// Makes a string safe to use as a file name component.
pub fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quotes() {
        assert_eq!(tokenize(r#"-o "a b/%(id)s" --x 'y z'"#), vec!["-o", "a b/%(id)s", "--x", "y z"]);
    }

    #[test]
    fn expands_and_appends() {
        let urls = vec!["u1".to_string(), "u2".to_string()];
        let one = vec!["U".to_string()];
        assert_eq!(expand("--a {urls}", "urls", &urls), vec!["--a", "u1", "u2"]);
        assert_eq!(expand("--a", "urls", &urls), vec!["--a", "u1", "u2"]);
        assert_eq!(expand("-o x.mp4 {url}", "url", &one), vec!["-o", "x.mp4", "U"]);
        assert_eq!(expand("--referer={url}", "url", &one), vec!["--referer=U"]);
        assert_eq!(expand("-o x.mp4", "url", &one), vec!["-o", "x.mp4", "U"]);
    }
}
