//! A deliberately small Cargo.toml reader: just enough to compare dependency
//! tables and detect build scripts / proc-macro crates without a TOML crate.

use std::collections::BTreeSet;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ManifestFacts {
    /// Normalised `header|key=value` lines of every dependency-like table
    /// (dependencies, build-/dev-dependencies, target-specific, patch, replace).
    pub deps: BTreeSet<String>,
    /// `[package] build = ...` is set.
    pub build_script: bool,
    /// `[lib] proc-macro = true`.
    pub proc_macro: bool,
}

fn strip_comment(line: &str) -> String {
    let (mut out, mut quote) = (String::new(), None::<char>);
    for c in line.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                } else if c == '#' {
                    break;
                }
            }
        }
        out.push(c);
    }
    out
}

fn depth_delta(s: &str) -> i32 {
    let (mut d, mut quote) = (0i32, None::<char>);
    for c in s.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => d += 1,
                ']' | '}' => d -= 1,
                _ => {}
            },
        }
    }
    d
}

fn squash(s: &str) -> String {
    let (mut out, mut quote) = (String::new(), None::<char>);
    for c in s.chars() {
        match quote {
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    out.push(c);
                } else if !c.is_whitespace() {
                    out.push(c);
                }
            }
        }
    }
    out
}

pub fn parse(text: &str) -> ManifestFacts {
    let mut facts = ManifestFacts::default();
    let mut header = String::new();
    let mut lines = text.lines().map(strip_comment);
    while let Some(raw) = lines.next() {
        let line = raw.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            header = squash(line.trim_start_matches('[').trim_end_matches(']'));
            if is_dep_header(&header) {
                facts.deps.insert(format!("[{header}]"));
            }
            continue;
        }
        let Some((key, first)) = line.split_once('=') else {
            continue;
        };
        let mut value = first.to_string();
        let mut depth = depth_delta(&value);
        while depth > 0 {
            let Some(next) = lines.next() else { break };
            depth += depth_delta(&next);
            value.push(' ');
            value.push_str(next.trim());
        }
        let (key, value) = (squash(key), squash(&value));
        if is_dep_header(&header) {
            facts.deps.insert(format!("{header}|{key}={value}"));
        }
        if header == "package" && key == "build" {
            facts.build_script = true;
        }
        if header == "lib" && (key == "proc-macro" || key == "proc_macro") && value == "true" {
            facts.proc_macro = true;
        }
    }
    facts
}

fn is_dep_header(h: &str) -> bool {
    h.contains("dependencies") || h.starts_with("patch") || h.starts_with("replace")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_dependency_tables() {
        let f = parse(
            "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\" # c\nfoo = { path = \"f\",\n  features = [\"a\"] }\n[dev-dependencies.bar]\nversion = \"2\"\n[target.'cfg(unix)'.build-dependencies]\ncc = \"1\"\n",
        );
        assert!(f.deps.contains("dependencies|serde=\"1\""));
        assert!(f.deps.iter().any(|d| d.starts_with("dependencies|foo=")));
        assert!(f.deps.contains("[dev-dependencies.bar]"));
        assert!(f.deps.contains("dev-dependencies.bar|version=\"2\""));
        assert!(f.deps.iter().any(|d| d.contains("build-dependencies|cc")));
        assert!(!f.build_script && !f.proc_macro);
    }

    #[test]
    fn detects_build_and_proc_macro() {
        let f = parse("[package]\nbuild = \"b.rs\"\n[lib]\nproc-macro = true\n");
        assert!(f.build_script && f.proc_macro);
        // comments and package names do not count
        let g = parse("[package]\nname = \"dependencies\" # build = x\n");
        assert!(g.deps.is_empty() && !g.build_script);
    }
}
