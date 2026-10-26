//! Token-aware source scan for the security gates. Operates on the output of
//! [`crate::lexer`], so comments and string contents never count.
//!
//! This is a conservative static heuristic, not a proof: it does not follow
//! macro expansion or renamed imports of whole crates (`use std as s;`).

use crate::lexer::{lex, Tok, Token};
use std::path::{Component, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Unsafe,
    Net,
    Process,
    FsWrite,
    Env,
    ExternC,
    IncludeOutside,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Unsafe => "unsafe",
            Kind::Net => "std::net",
            Kind::Process => "std::process",
            Kind::FsWrite => "filesystem write API",
            Kind::Env => "std::env",
            Kind::ExternC => "extern ABI",
            Kind::IncludeOutside => "include outside tree",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub kind: Kind,
    pub line: usize,
    pub detail: String,
}

const FS_WRITE: &[&str] = &[
    "write",
    "create",
    "create_new",
    "create_dir",
    "create_dir_all",
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "rename",
    "copy",
    "hard_link",
    "symlink",
    "soft_link",
    "set_permissions",
    "set_len",
    "OpenOptions",
    "options",
];
const NET_TYPES: &[&str] = &["TcpStream", "TcpListener", "UdpSocket"];

fn id(t: &[Token], i: usize) -> Option<&str> {
    match t.get(i).map(|x| &x.tok) {
        Some(Tok::Ident(s)) => Some(s),
        _ => None,
    }
}
fn punct(t: &[Token], i: usize, c: char) -> bool {
    matches!(t.get(i).map(|x| &x.tok), Some(Tok::Punct(p)) if *p == c)
}
fn colon2(t: &[Token], i: usize) -> bool {
    punct(t, i, ':') && punct(t, i + 1, ':')
}

/// True if a literal include path, relative to the including file's directory,
/// leaves the tree rooted at the crate checkout.
pub fn escapes_tree(file_rel: &Path, lit: &str) -> bool {
    let p = Path::new(lit);
    if p.is_absolute() {
        return true;
    }
    let mut depth = file_rel.parent().map_or(0, |d| d.components().count()) as i64;
    for c in p.components() {
        match c {
            Component::ParentDir => depth -= 1,
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            _ => return true,
        }
        if depth < 0 {
            return true;
        }
    }
    false
}

pub fn scan_rust(src: &str, file_rel: &Path) -> Vec<Finding> {
    let t = lex(src);
    let mut out = Vec::new();
    let mut add =
        |kind: Kind, line: usize, detail: String| out.push(Finding { kind, line, detail });
    let mut i = 0;
    while i < t.len() {
        let line = t[i].line;
        let Some(name) = id(&t, i) else {
            i += 1;
            continue;
        };
        match name {
            "unsafe" => add(Kind::Unsafe, line, "unsafe".into()),
            "extern" => {
                if let Some(Tok::Str(abi)) = t.get(i + 1).map(|x| &x.tok) {
                    if abi != "Rust" {
                        add(Kind::ExternC, line, format!("extern \"{abi}\""));
                    }
                }
            }
            "include_bytes" | "include_str" | "include"
                if punct(&t, i + 1, '!') && (punct(&t, i + 2, '(') || punct(&t, i + 2, '[')) =>
            {
                match (t.get(i + 3).map(|x| &x.tok), t.get(i + 4).map(|x| &x.tok)) {
                    (Some(Tok::Str(p)), Some(Tok::Punct(')' | ']'))) => {
                        if escapes_tree(file_rel, p) {
                            add(Kind::IncludeOutside, line, format!("{name}!(\"{p}\")"));
                        }
                    }
                    _ => add(
                        Kind::IncludeOutside,
                        line,
                        format!("{name}! with non-literal path"),
                    ),
                }
            }
            "use" if !punct(&t, i.wrapping_sub(1), '.') => {
                let mut j = i + 1;
                let mut idents: Vec<&str> = Vec::new();
                let mut glob = false;
                while j < t.len() && !punct(&t, j, ';') {
                    if let Some(s) = id(&t, j) {
                        idents.push(s);
                    }
                    glob |= punct(&t, j, '*');
                    j += 1;
                }
                if matches!(
                    idents.first(),
                    Some(&"std") | Some(&"core") | Some(&"alloc")
                ) {
                    let has = |n: &str| idents.contains(&n);
                    if has("net") {
                        add(Kind::Net, line, "use std::net".into());
                    }
                    if has("process") {
                        add(Kind::Process, line, "use std::process".into());
                    }
                    if has("env") {
                        add(Kind::Env, line, "use std::env".into());
                    }
                    if has("fs") && (glob || idents.iter().any(|s| FS_WRITE.contains(s))) {
                        add(Kind::FsWrite, line, "use of std::fs write API".into());
                    }
                }
                i = j;
            }
            _ if !colon2(&t, i.wrapping_sub(2)) => {
                // collect a `a::b::c` path starting here
                let mut segs = vec![name];
                let mut j = i + 1;
                while colon2(&t, j) {
                    match id(&t, j + 2) {
                        Some(s) => {
                            segs.push(s);
                            j += 3;
                        }
                        None => break,
                    }
                }
                classify_path(&segs, line, &mut add);
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn classify_path(segs: &[&str], line: usize, add: &mut impl FnMut(Kind, usize, String)) {
    let path = segs.join("::");
    let first = segs[0];
    let rest_has_fs_write = |from: usize| segs.iter().skip(from).any(|s| FS_WRITE.contains(s));
    if matches!(first, "std" | "core" | "alloc") && segs.len() > 1 {
        match segs[1] {
            "net" => add(Kind::Net, line, path),
            "process" => add(Kind::Process, line, path),
            "env" => add(Kind::Env, line, path),
            "fs" if rest_has_fs_write(2) => add(Kind::FsWrite, line, path),
            _ => {}
        }
        return;
    }
    if NET_TYPES.contains(&first) {
        add(Kind::Net, line, path);
    } else if first == "Command" && segs.get(1) == Some(&"new") {
        add(Kind::Process, line, path);
    } else if first == "env" && segs.len() > 1 {
        add(Kind::Env, line, path);
    } else if first == "OpenOptions"
        || (first == "fs" && rest_has_fs_write(1))
        || (first == "File" && segs.get(1).is_some_and(|s| FS_WRITE.contains(s)))
    {
        add(Kind::FsWrite, line, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Kind> {
        scan_rust(src, Path::new("src/lib.rs"))
            .iter()
            .map(|f| f.kind)
            .collect()
    }

    #[test]
    fn unsafe_forms_but_not_text() {
        assert_eq!(kinds("unsafe fn f() {} fn g() { unsafe { } }").len(), 2);
        assert!(kinds("// unsafe\nlet s = \"unsafe\"; /* unsafe */").is_empty());
        assert_eq!(kinds("unsafe impl Send for X {}"), vec![Kind::Unsafe]);
    }

    #[test]
    fn capabilities() {
        assert_eq!(kinds("use std::net::TcpStream;"), vec![Kind::Net]);
        assert_eq!(kinds("use std::{io, net::UdpSocket};"), vec![Kind::Net]);
        assert_eq!(
            kinds("fn f(){ std::process::Command::new(\"x\"); }"),
            vec![Kind::Process]
        );
        assert_eq!(kinds("let v = std::env::var(\"A\");"), vec![Kind::Env]);
        assert_eq!(
            kinds("fn f(){ std::fs::write(\"a\", b\"\"); }"),
            vec![Kind::FsWrite]
        );
        assert_eq!(
            kinds("use std::fs::{self, File}; fn f(){ File::create(\"x\"); }"),
            vec![Kind::FsWrite]
        );
        assert_eq!(kinds("use std::fs::*;"), vec![Kind::FsWrite]);
        assert_eq!(kinds("extern \"C\" fn f() {}"), vec![Kind::ExternC]);
    }

    #[test]
    fn reads_and_text_are_clean() {
        assert!(kinds("fn f(){ std::fs::read_to_string(\"a\").ok(); }").is_empty());
        assert!(kinds("let s = \"std::net::TcpStream\"; // std::process").is_empty());
        assert!(kinds("fn f(){ let _ = a.write(b); }").is_empty());
    }

    #[test]
    fn includes() {
        assert!(kinds("const A: &str = include_str!(\"data.txt\");").is_empty());
        assert!(kinds("const A: &str = include_str!(\"../data.txt\");").is_empty());
        assert_eq!(
            kinds("const A: &[u8] = include_bytes!(\"../../../../etc/passwd\");"),
            vec![Kind::IncludeOutside]
        );
        assert_eq!(
            kinds("const A: &str = include_str!(\"/etc/passwd\");"),
            vec![Kind::IncludeOutside]
        );
        assert_eq!(
            kinds("const A: &str = include_str!(concat!(env!(\"X\"), \"/y\"));"),
            vec![Kind::IncludeOutside]
        );
    }

    #[test]
    fn escape_math() {
        assert!(!escapes_tree(Path::new("src/lib.rs"), "../Cargo.toml"));
        assert!(escapes_tree(Path::new("src/lib.rs"), "../../x"));
        assert!(escapes_tree(Path::new("lib.rs"), "../x"));
    }
}
