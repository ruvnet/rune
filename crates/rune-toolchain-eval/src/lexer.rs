//! A small Rust lexer: enough to find identifiers, punctuation and string
//! literals while ignoring comments, strings and char literals. Used by the
//! security gates so `// unsafe` or `"std::net"` text never triggers (or hides)
//! a finding.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Punct(char),
    /// Decoded-enough string contents (escapes kept verbatim).
    Str(String),
    Other,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

fn is_id_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}
fn is_id_cont(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

pub fn lex(src: &str) -> Vec<Token> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1usize);
    let n = cs.len();
    let push = |out: &mut Vec<Token>, tok: Tok, line: usize| out.push(Token { tok, line });

    while i < n {
        let c = cs[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && cs.get(i + 1) == Some(&'/') {
            while i < n && cs[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && cs.get(i + 1) == Some(&'*') {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if cs[i] == '/' && cs.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if cs[i] == '*' && cs.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    if cs[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if let Some((body, next, lines)) = string_at(&cs, i) {
            push(&mut out, Tok::Str(body), line);
            line += lines;
            i = next;
        } else if c == '\'' {
            // char literal or lifetime
            if cs.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < n && cs[i] != '\'' {
                    i += 1;
                }
                i += 1;
                push(&mut out, Tok::Other, line);
            } else if cs.get(i + 2) == Some(&'\'') {
                i += 3;
                push(&mut out, Tok::Other, line);
            } else {
                i += 1; // lifetime tick; the name lexes as an identifier
                push(&mut out, Tok::Punct('\''), line);
            }
        } else if is_id_start(c) {
            let s = i;
            // raw identifier r#name
            if c == 'r'
                && cs.get(i + 1) == Some(&'#')
                && cs.get(i + 2).is_some_and(|x| is_id_start(*x))
            {
                i += 2;
            }
            let name_start = i;
            while i < n && is_id_cont(cs[i]) {
                i += 1;
            }
            let _ = s;
            push(
                &mut out,
                Tok::Ident(cs[name_start..i].iter().collect()),
                line,
            );
        } else if c.is_ascii_digit() {
            while i < n && (cs[i].is_alphanumeric() || cs[i] == '_' || cs[i] == '.') {
                // stop before `..` and method calls like `1.max(2)`
                if cs[i] == '.' && !cs.get(i + 1).is_some_and(|x| x.is_ascii_digit()) {
                    break;
                }
                i += 1;
            }
            push(&mut out, Tok::Other, line);
        } else {
            push(&mut out, Tok::Punct(c), line);
            i += 1;
        }
    }
    out
}

/// Recognise `"…"`, `b"…"`, `c"…"`, `r#"…"#`, `br"…"` at `i`.
/// Returns (contents, index after literal, newlines inside).
fn string_at(cs: &[char], i: usize) -> Option<(String, usize, usize)> {
    let n = cs.len();
    let mut j = i;
    if matches!(cs[j], 'b' | 'c') {
        j += 1;
    }
    let raw = cs.get(j) == Some(&'r');
    if raw {
        j += 1;
    }
    let mut hashes = 0;
    if raw {
        while cs.get(j) == Some(&'#') {
            hashes += 1;
            j += 1;
        }
    }
    if cs.get(j) != Some(&'"') {
        return None;
    }
    // `b`/`c`/`r` must start a literal, not end a longer identifier
    if j > i && i > 0 && (cs[i - 1] == '_' || cs[i - 1].is_alphanumeric()) {
        return None;
    }
    j += 1;
    let start = j;
    let mut lines = 0;
    while j < n {
        if cs[j] == '\n' {
            lines += 1;
        }
        if raw {
            if cs[j] == '"' && (0..hashes).all(|k| cs.get(j + 1 + k) == Some(&'#')) {
                let body = cs[start..j].iter().collect();
                return Some((body, j + 1 + hashes, lines));
            }
        } else if cs[j] == '\\' {
            j += 2;
            continue;
        } else if cs[j] == '"' {
            let body = cs[start..j].iter().collect();
            return Some((body, j + 1, lines));
        }
        j += 1;
    }
    Some((cs[start.min(n)..].iter().collect(), n, lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idents(s: &str) -> Vec<String> {
        lex(s)
            .into_iter()
            .filter_map(|t| match t.tok {
                Tok::Ident(i) => Some(i),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn ignores_comments_and_strings() {
        let src = r##"
            // unsafe in a line comment
            /* unsafe /* nested unsafe */ still comment */
            let s = "unsafe std::net";
            let r = r#"unsafe " still raw"#;
            let b = b"unsafe";
            let c = 'u'; let e = '\n';
            fn real<'a>() {}
        "##;
        let ids = idents(src);
        assert!(!ids.contains(&"unsafe".to_string()), "{ids:?}");
        assert!(ids.contains(&"real".to_string()));
        assert!(ids.contains(&"a".to_string()));
    }

    #[test]
    fn finds_real_tokens_and_lines() {
        let toks = lex("fn f() {\n  unsafe { x() }\n}");
        let u = toks
            .iter()
            .find(|t| t.tok == Tok::Ident("unsafe".into()))
            .unwrap();
        assert_eq!(u.line, 2);
    }

    #[test]
    fn string_contents_preserved() {
        let toks = lex(r#"extern "C" fn f() {}"#);
        assert!(toks.iter().any(|t| t.tok == Tok::Str("C".into())));
    }

    #[test]
    fn identifier_ending_in_b_is_not_a_string_prefix() {
        let ids = idents(r#"let sub = 1; let x = sub"y";"#);
        assert!(ids.contains(&"sub".to_string()));
    }
}
