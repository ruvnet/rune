//! Source text for kernel libraries: the baseline and the shared snippets the
//! recipe generators reuse. Generated crates are dependency-free and unsafe-free.

use crate::spec::{Comb, Kernel, Red, Shape, Style, NEEDLE};

pub const CARGO_TOML: &str = "[package]\nname = \"kernel\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[workspace]\n\n[lib]\nname = \"kernel\"\n\n[[bin]]\nname = \"bench\"\npath = \"src/bin/bench.rs\"\n\n[dependencies]\n\n[profile.release]\nopt-level = 3\ndebug = false\n";

pub fn params(k: &Kernel) -> String {
    let t = k.elem.ty();
    if k.n_in() == 2 {
        format!("a: &[{t}], b: &[{t}]")
    } else {
        format!("a: &[{t}]")
    }
}

pub fn ret_ty(k: &Kernel) -> String {
    match k.shape() {
        Shape::Reduce(r) => r.acc_ty.to_string(),
        Shape::Argmin | Shape::Find => "Option<usize>".into(),
        Shape::Hist => "[u64; 256]".into(),
    }
}

pub fn header(k: &Kernel, what: &str) -> String {
    format!(
        "//! {what}: {}. Frozen semantics are in the workload contract.\n\n",
        k.name()
    )
}

pub fn assert_len(k: &Kernel) -> String {
    if k.n_in() == 2 {
        format!(
            "    assert_eq!(a.len(), b.len(), \"{}: length mismatch\");\n",
            k.fn_name()
        )
    } else {
        String::new()
    }
}

fn is_float_acc(r: &Red) -> bool {
    r.acc_ty == "f32" || r.acc_ty == "f64"
}

/// One accumulator update of `place` with element variables `x` (and `y`).
/// `branchless` selects the select/min/max form for conditional updates.
pub fn upd(r: &Red, place: &str, branchless: bool, indent: &str) -> String {
    match r.comb {
        Comb::Add => {
            let t = &r.term;
            if is_float_acc(r) {
                format!("{indent}{place} += {t};\n")
            } else {
                format!("{indent}{place} = {place}.wrapping_add({t});\n")
            }
        }
        Comb::Max | Comb::Min => {
            let (op, f) = if r.comb == Comb::Max {
                (">", "max")
            } else {
                ("<", "min")
            };
            let t = &r.term;
            if branchless {
                format!("{indent}{place} = {place}.{f}({t});\n")
            } else {
                format!("{indent}let t = {t};\n{indent}if t {op} {place} {{\n{indent}    {place} = t;\n{indent}}}\n")
            }
        }
        Comb::Count => {
            let c = &r.cond;
            if branchless {
                format!("{indent}{place} += ({c}) as u64;\n")
            } else {
                format!("{indent}if {c} {{\n{indent}    {place} += 1;\n{indent}}}\n")
            }
        }
    }
}

/// Merge two partial accumulators.
pub fn merge(r: &Red, a: &str, b: &str) -> String {
    match r.comb {
        Comb::Add | Comb::Count => {
            if is_float_acc(r) {
                format!("{a} + {b}")
            } else {
                format!("{a}.wrapping_add({b})")
            }
        }
        Comb::Max => format!("{a}.max({b})"),
        Comb::Min => format!("{a}.min({b})"),
    }
}

fn open_fn(k: &Kernel) -> String {
    format!(
        "pub fn {}({}) -> {} {{\n{}",
        k.fn_name(),
        params(k),
        ret_ty(k),
        assert_len(k)
    )
}

/// Baseline loops, in the workload's fixed style.
fn reduce_baseline(k: &Kernel, r: &Red) -> String {
    let mut s = open_fn(k);
    s += &format!("    let mut acc: {} = {};\n", r.acc_ty, r.init);
    match (k.style, r.n_in) {
        (Style::Indexed, 2) => {
            s += "    for i in 0..a.len() {\n        let (x, y) = (a[i], b[i]);\n";
        }
        (Style::Indexed, _) => s += "    for i in 0..a.len() {\n        let x = a[i];\n",
        (Style::Iter, 2) => s += "    for (&x, &y) in a.iter().zip(b) {\n",
        (Style::Iter, _) => s += "    for &x in a {\n",
    }
    s += &upd(r, "acc", false, "        ");
    s += "    }\n    acc\n}\n";
    s
}

pub fn pred_table_cond(k: &Kernel) -> Option<String> {
    match k.shape() {
        Shape::Reduce(r) if r.comb == Comb::Count && r.n_in == 1 => Some(r.cond),
        _ => None,
    }
}

pub fn baseline_lib(k: &Kernel) -> String {
    let mut s = header(k, "Baseline kernel");
    let t = k.elem.ty();
    match k.shape() {
        Shape::Reduce(r) => s += &reduce_baseline(k, &r),
        Shape::Argmin => {
            s += &open_fn(k);
            s += &format!(
                "    if a.is_empty() {{\n        return None;\n    }}\n    let mut m: {t} = a[0];\n    for &x in a {{\n        if x < m {{\n            m = x;\n        }}\n    }}\n    a.iter().position(|&x| x == m)\n}}\n"
            );
        }
        Shape::Find => {
            s += &format!("pub const NEEDLE: u8 = {NEEDLE:#04X};\n\n");
            s += &open_fn(k);
            s += "    for i in 0..a.len() {\n        if a[i] == NEEDLE {\n            return Some(i);\n        }\n    }\n    None\n}\n";
        }
        Shape::Hist => {
            s += &open_fn(k);
            s += "    let mut h = [0u64; 256];\n    for &x in a {\n        h[x as usize] += 1;\n    }\n    h\n}\n";
        }
    }
    s
}
