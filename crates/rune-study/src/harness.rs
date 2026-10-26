//! Harness files: bench binary, visible tests and the hidden holdout. These are
//! generated once per workload and shared byte-identically by every candidate.

use crate::spec::{Comb, Elem, Kernel, Op, Red, Shape, NEEDLE};

fn gen_fn(k: &Kernel) -> String {
    let t = k.elem.ty();
    let body = match k.elem {
        Elem::F32 | Elem::F64 => {
            format!("((s >> 40) as {t} / (1u64 << 24) as {t}) * 2.0 - 1.0")
        }
        Elem::I32 => "((s >> 33) as i32) - (1 << 30)".to_string(),
        Elem::U8 => match k.op {
            Op::CountDigit | Op::CountVowel | Op::CountSpace => {
                "b\"aex 7Z\\nok3U.q\\tim\"[(s >> 59) as usize % 16]".to_string()
            }
            _ => "(s >> 56) as u8".to_string(),
        },
    };
    format!(
        "fn gen(n: usize, mut s: u64) -> Vec<{t}> {{\n    (0..n)\n        .map(|_| {{\n            s ^= s << 13;\n            s ^= s >> 7;\n            s ^= s << 17;\n            {body}\n        }})\n        .collect()\n}}\n"
    )
}

pub fn bench_rs(k: &Kernel) -> String {
    let f = k.fn_name();
    let mut prep = String::from(
        "    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1024);\n    let mut a = gen(n, 0x9E37_79B9_7F4A_7C15);\n",
    );
    let mut call = format!("kernel::{f}(black_box(&a))");
    if k.n_in() == 2 {
        prep += "    let mut b = gen(n, 0xD1B5_4A32_D192_ED03);\n";
        call = format!("kernel::{f}(black_box(&a), black_box(&b))");
        if k.op == Op::CountEq {
            // roughly half the positions equal, in an unpredictable pattern
            prep += "    for i in 0..n {\n        if (i.wrapping_mul(0x9E37_79B9) >> 7) & 1 == 0 {\n            b[i] = a[i];\n        }\n    }\n";
        }
    }
    if k.op == Op::FindFirst {
        prep += "    for x in a.iter_mut() {\n        if *x == kernel::NEEDLE {\n            *x ^= 1;\n        }\n    }\n    if let Some(l) = a.last_mut() {\n        *l = kernel::NEEDLE;\n    }\n";
    }
    let _ = NEEDLE;
    BENCH_TEMPLATE
        .replace("@GEN@", &gen_fn(k))
        .replace("@PREP@", &prep)
        .replace("@CALL@", &call)
}

/// Test data: variant 0 random, 1 spike at the end, 2 spike at the start,
/// 3 constant, 4 edge-value cycle.
fn val_fn(k: &Kernel) -> String {
    let t = k.elem.ty();
    let neg = matches!(k.op, Op::Min | Op::Argmin);
    let (rand, spike, constant, edge) = match k.elem {
        Elem::F32 | Elem::F64 => (
            format!("(((next(s) >> 40) as {t}) / (1u64 << 24) as {t}) * 2.0 - 1.0"),
            format!("{}1000.0", if neg { "-" } else { "" }),
            "0.75".to_string(),
            if k.elem == Elem::F32 {
                "[0.0, -0.0, 1.0e15, -1.0e15, f32::from_bits(1), 1.0, -1.0][i % 7]".to_string()
            } else {
                "[0.0, -0.0, 1.0e150, -1.0e150, f64::from_bits(1), 1.0, -1.0][i % 7]".to_string()
            },
        ),
        Elem::I32 => (
            "((next(s) >> 33) as i32) - (1 << 30)".to_string(),
            format!("{}1_000_000", if neg { "-" } else { "" }),
            "7".to_string(),
            "[0, -1, 1, i32::MIN, i32::MAX, 1 << 30, -(1 << 30)][i % 7]".to_string(),
        ),
        Elem::U8 => (
            "(next(s) >> 56) as u8".to_string(),
            if k.op == Op::FindFirst {
                "0xA5".into()
            } else {
                "0xFF".to_string()
            },
            "0xFF".to_string(),
            "[0u8, 255, b'0', b'9', b'a', b'U', b' ', b'\\n', 0xA5][i % 9]".to_string(),
        ),
    };
    let find_fix = if k.op == Op::FindFirst {
        "    if variant == 0 && v == kernel::NEEDLE {\n        return kernel::NEEDLE ^ 1;\n    }\n"
    } else {
        ""
    };
    format!(
        "fn next(s: &mut u64) -> u64 {{\n    *s ^= *s << 13;\n    *s ^= *s >> 7;\n    *s ^= *s << 17;\n    *s\n}}\n\nfn val(variant: u32, i: usize, n: usize, s: &mut u64) -> {t} {{\n    let v: {t} = match variant {{\n        1 if i + 1 == n => {spike},\n        2 if i == 0 => {spike},\n        3 => {constant},\n        4 => {edge},\n        _ => {rand},\n    }};\n{find_fix}    v\n}}\n\nfn make(variant: u32, n: usize, seed: u64) -> Vec<{t}> {{\n    let mut s = seed | 1;\n    (0..n).map(|i| val(variant, i, n, &mut s)).collect()\n}}\n"
    )
}

fn reference_fn(k: &Kernel) -> String {
    let t = k.elem.ty();
    match k.shape() {
        Shape::Reduce(r) => reference_reduce(k, &r),
        Shape::Argmin => format!(
            "fn reference(a: &[{t}]) -> Option<usize> {{\n    let mut best: Option<(usize, {t})> = None;\n    for (i, &v) in a.iter().enumerate() {{\n        match best {{\n            Some((_, m)) if !(v < m) => {{}}\n            _ => best = Some((i, v)),\n        }}\n    }}\n    best.map(|(i, _)| i)\n}}\n"
        ),
        Shape::Find => "fn reference(a: &[u8]) -> Option<usize> {\n    a.iter().position(|&b| b == kernel::NEEDLE)\n}\n".into(),
        Shape::Hist => "fn reference(a: &[u8]) -> Vec<u64> {\n    let mut h = vec![0u64; 256];\n    for &x in a {\n        h[x as usize] += 1;\n    }\n    h\n}\n".into(),
    }
}

fn reference_reduce(k: &Kernel, r: &Red) -> String {
    let t = k.elem.ty();
    let params = if r.n_in == 2 {
        format!("a: &[{t}], b: &[{t}]")
    } else {
        format!("a: &[{t}]")
    };
    let bind = if r.n_in == 2 {
        "let (x, y) = (a[i], b[i]);"
    } else {
        "let x = a[i];"
    };
    if k.elem.is_float() {
        let init = match r.comb {
            Comb::Min => "f64::INFINITY",
            _ => "0.0f64",
        };
        let fbind = if r.n_in == 2 {
            "let (x, y) = (a[i] as f64, b[i] as f64);"
        } else {
            "let x = a[i] as f64;"
        };
        let comb = match r.comb {
            Comb::Max => "v = v.max(t);",
            Comb::Min => "v = v.min(t);",
            _ => "v += t;",
        };
        format!(
            "fn reference({params}) -> (f64, f64) {{\n    let (mut v, mut s) = ({init}, 0.0f64);\n    for i in 0..a.len() {{\n        {fbind}\n        let t: f64 = {};\n        {comb}\n        s += t.abs();\n    }}\n    (v, s)\n}}\n",
            r.term
        )
    } else {
        let step = match r.comb {
            Comb::Add => format!("let t = {};\n        v = v.wrapping_add(t);", r.term),
            Comb::Max => format!("v = v.max({});", r.term),
            Comb::Min => format!("v = v.min({});", r.term),
            Comb::Count => format!("if {} {{\n            v += 1;\n        }}", r.cond),
        };
        format!(
            "fn reference({params}) -> {} {{\n    let mut v: {} = {};\n    for i in 0..a.len() {{\n        {bind}\n        {step}\n    }}\n    v\n}}\n",
            r.acc_ty, r.acc_ty, r.init
        )
    }
}

/// `check(got, a, b)` body lines comparing the kernel output to the reference.
fn check_stmt(k: &Kernel, args: &str, ctx: &str) -> String {
    let f = k.fn_name();
    let call = format!("kernel::{f}({args})");
    match k.shape() {
        Shape::Reduce(r) if k.elem.is_float() => {
            let (tol, floor) = if k.elem == Elem::F32 { ("1e-4", "1.2e-38") } else { ("1e-9", "1e-300") };
            if r.comb == Comb::Add {
                format!("        let got = {call} as f64;\n        let (v, s) = reference({args});\n        assert!(got.is_finite() && (got - v).abs() <= {tol} * s + {floor}, \"{ctx}: got {{got}}, want {{v}} (scale {{s}})\");\n")
            } else {
                format!("        let got = {call} as f64;\n        let (v, _) = reference({args});\n        assert_eq!(got, v, \"{ctx}\");\n")
            }
        }
        Shape::Hist => format!("        let got = {call};\n        assert_eq!(got.to_vec(), reference({args}), \"{ctx}\");\n"),
        _ => format!("        let got = {call};\n        assert_eq!(got, reference({args}), \"{ctx}\");\n"),
    }
}

fn test_body(k: &Kernel, lengths: &str, variants: &str, name: &str) -> String {
    let (decl, args) = if k.op == Op::CountEq {
        ("        let a = make(variant, n, 1000 + n as u64);\n        let mut b = make(variant ^ 8, n, 1000 + n as u64);\n        for i in (1..n).step_by(3) {\n            b[i] = b[i].wrapping_add(1);\n        }\n", "&a, &b")
    } else if k.n_in() == 2 {
        ("        let a = make(variant, n, 1000 + n as u64);\n        let b = make(variant ^ 8, n, 77_000 + n as u64);\n", "&a, &b")
    } else {
        ("        let a = make(variant, n, 1000 + n as u64);\n", "&a")
    };
    format!(
        "#[test]\nfn {name}() {{\n    for variant in {variants} {{\n        for n in {lengths} {{\n{decl}{}        }}\n    }}\n}}\n",
        check_stmt(k, args, "variant {variant} n {n}")
    )
}

pub fn visible_rs(k: &Kernel) -> String {
    format!(
        "//! Visible tests: random data, lengths that are multiples of 8.\n{}\n{}\n{}",
        val_fn(k),
        reference_fn(k),
        test_body(
            k,
            "[8usize, 16, 64, 256, 1024, 4096]",
            "[0u32]",
            "multiples_of_eight_match_reference"
        )
    )
}

pub fn holdout_rs(k: &Kernel) -> String {
    let t = k.elem.ty();
    let empty = {
        let args = if k.n_in() == 2 { "&[], &[]" } else { "&[]" };
        let args2 = if k.n_in() == 2 {
            format!("&Vec::<{t}>::new(), &Vec::<{t}>::new()")
        } else {
            format!("&Vec::<{t}>::new()")
        };
        let _ = args;
        let chk = check_stmt(k, &args2, "empty input");
        format!("#[test]\nfn empty_matches_reference() {{\n    {{\n{chk}    }}\n}}\n")
    };
    let panic = if k.n_in() == 2 {
        format!(
            "\n#[test]\n#[should_panic(expected = \"length mismatch\")]\nfn mismatched_lengths_panic() {{\n    kernel::{}(&vec![{t}::default(); 3], &vec![{t}::default(); 2]);\n}}\n",
            k.fn_name()
        )
    } else {
        String::new()
    };
    let lengths = if k.elem == Elem::F32 || k.elem == Elem::F64 {
        "[1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 33, 63, 65, 1023, 1025, 2040, 2041, 4099]"
    } else {
        "[1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 33, 63, 65, 1023, 1025, 2040, 2041, 4099, 65537]"
    };
    format!(
        "//! HIDDEN holdout (ADR 005). Never shown to the proposer. Lengths 0 (empty), 1, 7, 9,\n//! odd and large; random, spike-at-end, spike-at-start, constant and edge-value data.\n//! Float sums: |got - ref| <= tol * sum|terms| (tol 1e-4 f32, 1e-9 f64), ref in f64.\n//! Everything else is exact.\n{}\n{}\n{}\n{}{}",
        val_fn(k),
        reference_fn(k),
        test_body(k, lengths, "[0u32, 1, 2, 3, 4]", "tail_and_edge_cases_match_reference"),
        empty,
        panic
    )
}

const BENCH_TEMPLATE: &str = r#"//! Prints one JSON line {"ns_per_op": median over batches} for an input length
//! given as the first argument (default 1024).
#![allow(unused_mut)]
use std::hint::black_box;
use std::time::Instant;

@GEN@
fn main() {
@PREP@    let batch = |iters: u64| {
        let t = Instant::now();
        for _ in 0..iters {
            black_box(@CALL@);
        }
        t.elapsed().as_nanos() as f64 / iters as f64
    };
    let mut iters = 1u64;
    while iters < (1 << 28) && batch(iters) * (iters as f64) < 2_000_000.0 {
        iters *= 2;
    }
    let mut per: Vec<f64> = (0..15).map(|_| batch(iters)).collect();
    per.sort_by(|x, y| x.total_cmp(y));
    println!("{{\"ns_per_op\": {:.4}}}", per[per.len() / 2]);
}
"#;
