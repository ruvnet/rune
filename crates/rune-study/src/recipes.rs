//! The recipe library: eight generic, hand-authored transformations, each a
//! concrete candidate generator with a mechanical applicability rule.

use crate::code::*;
use crate::spec::{Comb, Elem, Kernel, Op, Red, Shape, Style};

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Recipe {
    Unroll4,
    Unroll8,
    IterFold,
    BoundsHoist,
    Branchless,
    SwarU64,
    Table,
    FusedPass,
}

pub const ALL: [Recipe; 8] = [
    Recipe::Unroll4,
    Recipe::Unroll8,
    Recipe::IterFold,
    Recipe::BoundsHoist,
    Recipe::Branchless,
    Recipe::SwarU64,
    Recipe::Table,
    Recipe::FusedPass,
];

impl Recipe {
    pub fn id(self) -> &'static str {
        match self {
            Recipe::Unroll4 => "unroll4",
            Recipe::Unroll8 => "unroll8",
            Recipe::IterFold => "iter-fold",
            Recipe::BoundsHoist => "bounds-hoist",
            Recipe::Branchless => "branchless",
            Recipe::SwarU64 => "swar-u64",
            Recipe::Table => "table",
            Recipe::FusedPass => "fused-pass",
        }
    }

    pub fn descriptor(self) -> &'static str {
        match self {
            Recipe::Unroll4 => "multi-accumulator unrolling, 4 lanes via chunks_exact with scalar tail",
            Recipe::Unroll8 => "multi-accumulator unrolling, 8 lanes via chunks_exact with scalar tail",
            Recipe::IterFold => "rewrite an indexed loop as an iterator zip/fold",
            Recipe::BoundsHoist => "hoist slice bounds: re-slice inputs to one length before an indexed loop",
            Recipe::Branchless => "replace a data-dependent branch with a select (count: += cond as u64; extremum: .max/.min)",
            Recipe::SwarU64 => "word-at-a-time (u64 SWAR) processing of byte slices with scalar tail",
            Recipe::Table => "lookup table: 256-entry const class table (predicate counts) or 4 sub-histograms (byte histogram)",
            Recipe::FusedPass => "fuse a two-pass min-then-position scan into one pass tracking value and index",
        }
    }

    pub fn applicability(self) -> &'static str {
        match self {
            Recipe::Unroll4 | Recipe::Unroll8 => "kernel is a scalar reduction (Shape::Reduce)",
            Recipe::IterFold | Recipe::BoundsHoist => {
                "kernel is a scalar reduction whose baseline loop style is indexed"
            }
            Recipe::Branchless => "scalar reduction whose combine is max, min or conditional count",
            Recipe::SwarU64 => "u8 kernel with op in {sum, count_eq, hamming, find_first}",
            Recipe::Table => "u8 kernel with op in {count_digit, count_vowel, count_space, hist}",
            Recipe::FusedPass => "op is argmin",
        }
    }

    pub fn applies(self, k: &Kernel) -> bool {
        let sh = k.shape();
        let reduce = matches!(sh, Shape::Reduce(_));
        match self {
            Recipe::Unroll4 | Recipe::Unroll8 => reduce,
            Recipe::IterFold | Recipe::BoundsHoist => reduce && k.style == Style::Indexed,
            Recipe::Branchless => matches!(&sh, Shape::Reduce(r) if r.comb != Comb::Add),
            Recipe::SwarU64 => {
                k.elem == Elem::U8
                    && matches!(k.op, Op::Sum | Op::CountEq | Op::Hamming | Op::FindFirst)
            }
            Recipe::Table => {
                k.elem == Elem::U8
                    && matches!(
                        k.op,
                        Op::CountDigit | Op::CountVowel | Op::CountSpace | Op::Hist
                    )
            }
            Recipe::FusedPass => k.op == Op::Argmin,
        }
    }
}

fn unrolled(k: &Kernel, r: &Red, lanes: usize) -> String {
    let t = k.elem.ty();
    let two = r.n_in == 2;
    let mut s = String::new();
    s += &format!(
        "pub fn {}({}) -> {} {{\n{}",
        k.fn_name(),
        params(k),
        r.acc_ty,
        assert_len(k)
    );
    s += &format!(
        "    let mut acc: [{}; {lanes}] = [{}; {lanes}];\n",
        r.acc_ty, r.init
    );
    if two {
        s += &format!("    let (ca, cb) = (a.chunks_exact({lanes}), b.chunks_exact({lanes}));\n");
        s += "    let (ra, rb) = (ca.remainder(), cb.remainder());\n";
        s += "    for (xs, ys) in ca.zip(cb) {\n";
        s += &format!("        for l in 0..{lanes} {{\n            let (x, y) = (xs[l], ys[l]);\n");
    } else {
        s += &format!("    let ca = a.chunks_exact({lanes});\n    let ra = ca.remainder();\n");
        s += "    for xs in ca {\n";
        s += &format!("        for l in 0..{lanes} {{\n            let x = xs[l];\n");
    }
    s += &upd(r, "acc[l]", false, "            ");
    s += "        }\n    }\n";
    s += &format!("    let mut s: {} = {};\n", r.acc_ty, r.init);
    s += &format!(
        "    for l in 0..{lanes} {{\n        s = {};\n    }}\n",
        merge(r, "s", "acc[l]")
    );
    if two {
        s += "    for (&x, &y) in ra.iter().zip(rb) {\n";
    } else {
        s += "    for &x in ra {\n";
    }
    s += &upd(r, "s", false, "        ");
    s += "    }\n    s\n}\n";
    let _ = t;
    s
}

fn iter_fold(k: &Kernel, r: &Red) -> String {
    let mut s = format!(
        "pub fn {}({}) -> {} {{\n{}",
        k.fn_name(),
        params(k),
        r.acc_ty,
        assert_len(k)
    );
    if r.n_in == 2 {
        s += &format!(
            "    a.iter().zip(b).fold({}, |mut acc: {}, (&x, &y)| {{\n",
            r.init, r.acc_ty
        );
    } else {
        s += &format!(
            "    a.iter().fold({}, |mut acc: {}, &x| {{\n",
            r.init, r.acc_ty
        );
    }
    s += &upd(r, "acc", false, "        ");
    s += "        acc\n    })\n}\n";
    s
}

fn bounds_hoist(k: &Kernel, r: &Red) -> String {
    let mut s = format!(
        "pub fn {}({}) -> {} {{\n{}",
        k.fn_name(),
        params(k),
        r.acc_ty,
        assert_len(k)
    );
    s += "    let n = a.len();\n    let a = &a[..n];\n";
    if r.n_in == 2 {
        s += "    let b = &b[..n];\n";
    }
    s += &format!(
        "    let mut acc: {} = {};\n    for i in 0..n {{\n",
        r.acc_ty, r.init
    );
    s += if r.n_in == 2 {
        "        let (x, y) = (a[i], b[i]);\n"
    } else {
        "        let x = a[i];\n"
    };
    s += &upd(r, "acc", false, "        ");
    s += "    }\n    acc\n}\n";
    s
}

fn branchless(k: &Kernel, r: &Red) -> String {
    let mut s = format!(
        "pub fn {}({}) -> {} {{\n{}",
        k.fn_name(),
        params(k),
        r.acc_ty,
        assert_len(k)
    );
    s += &format!("    let mut acc: {} = {};\n", r.acc_ty, r.init);
    s += match (k.style, r.n_in) {
        (Style::Indexed, 2) => "    for i in 0..a.len() {\n        let (x, y) = (a[i], b[i]);\n",
        (Style::Indexed, _) => "    for i in 0..a.len() {\n        let x = a[i];\n",
        (Style::Iter, 2) => "    for (&x, &y) in a.iter().zip(b) {\n",
        (Style::Iter, _) => "    for &x in a {\n",
    };
    s += &upd(r, "acc", true, "        ");
    s += "    }\n    acc\n}\n";
    s
}

const CHUNK8: &str = "u64::from_le_bytes(c.try_into().unwrap())";

fn swar(k: &Kernel) -> String {
    let mut s = String::new();
    match k.op {
        Op::Sum => {
            s += "pub fn sum(a: &[u8]) -> u64 {\n    const M: u64 = 0x00FF_00FF_00FF_00FF;\n    let fold = |v: u64| (v & 0xFFFF) + ((v >> 16) & 0xFFFF) + ((v >> 32) & 0xFFFF) + (v >> 48);\n    let mut total = 0u64;\n    for blk in a.chunks(8 * 255) {\n        let words = blk.chunks_exact(8);\n        let tail = words.remainder();\n        let (mut lo, mut hi) = (0u64, 0u64);\n        for c in words {\n            let v = ";
            s += CHUNK8;
            s += ";\n            lo += v & M;\n            hi += (v >> 8) & M;\n        }\n        total += fold(lo) + fold(hi);\n        for &b in tail {\n            total += b as u64;\n        }\n    }\n    total\n}\n";
        }
        Op::CountEq => {
            s += "pub fn count_eq(a: &[u8], b: &[u8]) -> u64 {\n    assert_eq!(a.len(), b.len(), \"count_eq: length mismatch\");\n    const L7: u64 = 0x7F7F_7F7F_7F7F_7F7F;\n    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));\n    let (ra, rb) = (ca.remainder(), cb.remainder());\n    let mut n = 0u64;\n    for (x, y) in ca.zip(cb) {\n        let d = u64::from_le_bytes(x.try_into().unwrap()) ^ u64::from_le_bytes(y.try_into().unwrap());\n        let z = !(((d & L7).wrapping_add(L7)) | d | L7);\n        n += z.count_ones() as u64;\n    }\n    for (x, y) in ra.iter().zip(rb) {\n        if x == y {\n            n += 1;\n        }\n    }\n    n\n}\n";
        }
        Op::Hamming => {
            s += "pub fn hamming(a: &[u8], b: &[u8]) -> u64 {\n    assert_eq!(a.len(), b.len(), \"hamming: length mismatch\");\n    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));\n    let (ra, rb) = (ca.remainder(), cb.remainder());\n    let mut n = 0u64;\n    for (x, y) in ca.zip(cb) {\n        n += (u64::from_le_bytes(x.try_into().unwrap()) ^ u64::from_le_bytes(y.try_into().unwrap())).count_ones() as u64;\n    }\n    for (&x, &y) in ra.iter().zip(rb) {\n        n += (x ^ y).count_ones() as u64;\n    }\n    n\n}\n";
        }
        _ => {
            s += "pub fn find_first(a: &[u8]) -> Option<usize> {\n    const ONES: u64 = 0x0101_0101_0101_0101;\n    const HIGH: u64 = 0x8080_8080_8080_8080;\n    let pat = ONES * NEEDLE as u64;\n    let chunks = a.chunks_exact(8);\n    let rem = chunks.remainder();\n    let mut base = 0usize;\n    for c in chunks {\n        let x = ";
            s += CHUNK8;
            s += " ^ pat;\n        let m = x.wrapping_sub(ONES) & !x & HIGH;\n        if m != 0 {\n            return Some(base + (m.trailing_zeros() / 8) as usize);\n        }\n        base += 8;\n    }\n    for (i, &b) in rem.iter().enumerate() {\n        if b == NEEDLE {\n            return Some(base + i);\n        }\n    }\n    None\n}\n";
        }
    }
    s
}

fn table(k: &Kernel) -> String {
    if k.op == Op::Hist {
        return "pub fn hist(a: &[u8]) -> [u64; 256] {\n    let mut h = [[0u64; 256]; 4];\n    let c = a.chunks_exact(4);\n    let r = c.remainder();\n    for ch in c {\n        h[0][ch[0] as usize] += 1;\n        h[1][ch[1] as usize] += 1;\n        h[2][ch[2] as usize] += 1;\n        h[3][ch[3] as usize] += 1;\n    }\n    for &x in r {\n        h[0][x as usize] += 1;\n    }\n    let mut out = [0u64; 256];\n    for i in 0..256 {\n        out[i] = h[0][i] + h[1][i] + h[2][i] + h[3][i];\n    }\n    out\n}\n".into();
    }
    let cond = pred_table_cond(k).expect("predicate kernel");
    format!(
        "const TABLE: [u8; 256] = {{\n    let mut t = [0u8; 256];\n    let mut i = 0usize;\n    while i < 256 {{\n        let x = i as u8;\n        t[i] = ({cond}) as u8;\n        i += 1;\n    }}\n    t\n}};\n\npub fn {}(a: &[u8]) -> u64 {{\n    let mut acc = 0u64;\n    for &x in a {{\n        acc += TABLE[x as usize] as u64;\n    }}\n    acc\n}}\n",
        k.fn_name()
    )
}

fn fused(k: &Kernel) -> String {
    format!(
        "pub fn argmin(a: &[{}]) -> Option<usize> {{\n    if a.is_empty() {{\n        return None;\n    }}\n    let mut best = a[0];\n    let mut idx = 0usize;\n    for (i, &x) in a.iter().enumerate() {{\n        if x < best {{\n            best = x;\n            idx = i;\n        }}\n    }}\n    Some(idx)\n}}\n",
        k.elem.ty()
    )
}

/// Candidate `src/lib.rs` for `(recipe, kernel)`; `None` if not applicable.
pub fn candidate_lib(recipe: Recipe, k: &Kernel) -> Option<String> {
    if !recipe.applies(k) {
        return None;
    }
    let mut s = header(k, &format!("Candidate [{}]", recipe.id()));
    if k.op == Op::FindFirst {
        s += &format!("pub const NEEDLE: u8 = {:#04X};\n\n", crate::spec::NEEDLE);
    }
    let red = match k.shape() {
        Shape::Reduce(r) => Some(r),
        _ => None,
    };
    s += &match (recipe, red) {
        (Recipe::Unroll4, Some(r)) => unrolled(k, &r, 4),
        (Recipe::Unroll8, Some(r)) => unrolled(k, &r, 8),
        (Recipe::IterFold, Some(r)) => iter_fold(k, &r),
        (Recipe::BoundsHoist, Some(r)) => bounds_hoist(k, &r),
        (Recipe::Branchless, Some(r)) => branchless(k, &r),
        (Recipe::SwarU64, _) => swar(k),
        (Recipe::Table, _) => table(k),
        (Recipe::FusedPass, _) => fused(k),
        _ => return None,
    };
    Some(s)
}
