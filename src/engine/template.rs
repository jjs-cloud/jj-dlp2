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

/// Builds the final argument list; a bare `{urls}` token expands to one argument per URL.
pub fn expand(template: &str, vars: &[(&str, String)], urls: Option<&[String]>) -> Vec<String> {
    let mut args = Vec::new();
    let mut used_urls = false;
    for tok in tokenize(template) {
        if tok == "{urls}" {
            args.extend(urls.unwrap_or(&[]).iter().cloned());
            used_urls = true;
            continue;
        }
        let mut s = tok;
        for (k, v) in vars {
            s = s.replace(&format!("{{{k}}}"), v);
        }
        args.push(s);
    }
    if let (false, Some(urls)) = (used_urls, urls) {
        args.extend(urls.iter().cloned());
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
    fn expands_urls_and_vars() {
        let urls = vec!["u1".to_string(), "u2".to_string()];
        assert_eq!(expand("--a {urls}", &[], Some(&urls)), vec!["--a", "u1", "u2"]);
        assert_eq!(expand("-o {id}.mp4 {url}", &[("id", "X".into()), ("url", "U".into())], None), vec!["-o", "X.mp4", "U"]);
        assert_eq!(expand("--a", &[], Some(&urls)), vec!["--a", "u1", "u2"]);
    }
}
