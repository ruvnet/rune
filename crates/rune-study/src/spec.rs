//! Workload specifications: element types x operations, their frozen
//! semantics, baseline loop style, family tags and the train/unseen split.
//! Everything here is a pure function of the workload name; no outcome data.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Elem {
    F32,
    F64,
    I32,
    U8,
}

impl Elem {
    pub fn ty(self) -> &'static str {
        match self {
            Elem::F32 => "f32",
            Elem::F64 => "f64",
            Elem::I32 => "i32",
            Elem::U8 => "u8",
        }
    }
    pub fn is_float(self) -> bool {
        matches!(self, Elem::F32 | Elem::F64)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Sum,
    Dot,
    Sqdist,
    L1,
    SumSq,
    SaxpySum,
    MaxAbs,
    Min,
    Max,
    Argmin,
    CountEq,
    Hamming,
    FindFirst,
    CountDigit,
    CountVowel,
    CountSpace,
    Hist,
}

impl Op {
    pub fn name(self) -> &'static str {
        match self {
            Op::Sum => "sum",
            Op::Dot => "dot",
            Op::Sqdist => "sqdist",
            Op::L1 => "l1",
            Op::SumSq => "sumsq",
            Op::SaxpySum => "saxpy_sum",
            Op::MaxAbs => "maxabs",
            Op::Min => "min",
            Op::Max => "max",
            Op::Argmin => "argmin",
            Op::CountEq => "count_eq",
            Op::Hamming => "hamming",
            Op::FindFirst => "find_first",
            Op::CountDigit => "count_digit",
            Op::CountVowel => "count_vowel",
            Op::CountSpace => "count_space",
            Op::Hist => "hist",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Indexed,
    Iter,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Comb {
    Add,
    Max,
    Min,
    Count,
}

/// A scalar reduction `acc = comb(acc, term(x, y))` (or a conditional count).
#[derive(Clone, Debug)]
pub struct Red {
    pub n_in: usize,
    pub acc_ty: &'static str,
    pub init: String,
    /// Rust expression over `x` (and `y`); empty for `Comb::Count`.
    pub term: String,
    pub comb: Comb,
    /// Boolean expression over `x` (and `y`) for `Comb::Count`.
    pub cond: String,
}

#[derive(Clone, Debug)]
pub enum Shape {
    Reduce(Red),
    Argmin,
    Find,
    Hist,
}

#[derive(Clone, Debug)]
pub struct Kernel {
    pub elem: Elem,
    pub op: Op,
    pub style: Style,
}

pub const NEEDLE: u8 = 0xA5;
pub const SPLIT_SALT: &str = "rune-study/split/v1:";
pub const STYLE_SALT: &str = "rune-study/style/v1:";

pub fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn red(n_in: usize, acc_ty: &'static str, init: &str, term: &str, comb: Comb, cond: &str) -> Shape {
    Shape::Reduce(Red {
        n_in,
        acc_ty,
        init: init.into(),
        term: term.into(),
        comb,
        cond: cond.into(),
    })
}

impl Kernel {
    pub fn new(elem: Elem, op: Op) -> Kernel {
        let name = format!("{}_{}", elem.ty(), op.name());
        let style = if fnv1a(&format!("{STYLE_SALT}{name}")) & 1 == 0 {
            Style::Indexed
        } else {
            Style::Iter
        };
        Kernel { elem, op, style }
    }

    pub fn name(&self) -> String {
        format!("{}_{}", self.elem.ty(), self.op.name())
    }

    pub fn fn_name(&self) -> &'static str {
        self.op.name()
    }

    pub fn shape(&self) -> Shape {
        let t = self.elem.ty();
        let inf = format!("{t}::INFINITY");
        match (self.elem, self.op) {
            (Elem::F32 | Elem::F64, op) => match op {
                Op::Sum => red(1, t, "0.0", "x", Comb::Add, ""),
                Op::Dot => red(2, t, "0.0", "x * y", Comb::Add, ""),
                Op::Sqdist => red(2, t, "0.0", "{ let d = x - y; d * d }", Comb::Add, ""),
                Op::L1 => red(2, t, "0.0", "(x - y).abs()", Comb::Add, ""),
                Op::SumSq => red(1, t, "0.0", "x * x", Comb::Add, ""),
                Op::SaxpySum => red(2, t, "0.0", "0.5 * x + y", Comb::Add, ""),
                Op::MaxAbs => red(1, t, "0.0", "x.abs()", Comb::Max, ""),
                Op::Min => red(1, t, &inf, "x", Comb::Min, ""),
                Op::Argmin => Shape::Argmin,
                _ => unreachable!("not a float op"),
            },
            (Elem::I32, op) => match op {
                Op::Sum => red(1, "i64", "0i64", "x as i64", Comb::Add, ""),
                Op::Dot => red(2, "i64", "0i64", "(x as i64) * (y as i64)", Comb::Add, ""),
                Op::L1 => red(
                    2,
                    "u64",
                    "0u64",
                    "(x as i64 - y as i64).unsigned_abs()",
                    Comb::Add,
                    "",
                ),
                Op::Max => red(1, "i32", "i32::MIN", "x", Comb::Max, ""),
                Op::Min => red(1, "i32", "i32::MAX", "x", Comb::Min, ""),
                Op::CountEq => red(2, "u64", "0u64", "", Comb::Count, "x == y"),
                Op::Argmin => Shape::Argmin,
                _ => unreachable!("not an i32 op"),
            },
            (Elem::U8, op) => match op {
                Op::Sum => red(1, "u64", "0u64", "x as u64", Comb::Add, ""),
                Op::CountEq => red(2, "u64", "0u64", "", Comb::Count, "x == y"),
                Op::Hamming => red(2, "u64", "0u64", "(x ^ y).count_ones() as u64", Comb::Add, ""),
                Op::Max => red(1, "u8", "0u8", "x", Comb::Max, ""),
                Op::CountDigit => red(1, "u64", "0u64", "", Comb::Count, "matches!(x, b'0'..=b'9')"),
                Op::CountVowel => red(
                    1,
                    "u64",
                    "0u64",
                    "",
                    Comb::Count,
                    "matches!(x, b'a' | b'e' | b'i' | b'o' | b'u' | b'A' | b'E' | b'I' | b'O' | b'U')",
                ),
                Op::CountSpace => red(
                    1,
                    "u64",
                    "0u64",
                    "",
                    Comb::Count,
                    "matches!(x, b' ' | b'\\t' | b'\\n' | b'\\r')",
                ),
                Op::FindFirst => Shape::Find,
                Op::Hist => Shape::Hist,
                _ => unreachable!("not a u8 op"),
            },
        }
    }

    pub fn n_in(&self) -> usize {
        match self.shape() {
            Shape::Reduce(r) => r.n_in,
            _ => 1,
        }
    }

    /// Source-level tags (element type, structure, algebra, loop style).
    pub fn source_tags(&self) -> Vec<String> {
        let mut t = vec![format!("elem:{}", self.elem.ty())];
        match self.shape() {
            Shape::Reduce(r) => {
                t.push("shape:reduce".into());
                t.push(format!(
                    "combine:{}",
                    match r.comb {
                        Comb::Add => "add",
                        Comb::Max => "max",
                        Comb::Min => "min",
                        Comb::Count => "count",
                    }
                ));
                if self.elem.is_float() && r.comb == Comb::Add {
                    t.push("fp:nonassociative".into());
                } else {
                    t.push("assoc:exact".into());
                }
            }
            Shape::Argmin => {
                t.push("shape:argreduce".into());
                t.push("assoc:exact".into());
            }
            Shape::Find => {
                t.push("shape:search".into());
                t.push("assoc:exact".into());
            }
            Shape::Hist => {
                t.push("shape:histogram".into());
                t.push("assoc:exact".into());
            }
        }
        t.push(match self.style {
            Style::Indexed => "loop:indexed".into(),
            Style::Iter => "loop:iter".into(),
        });
        t
    }

    pub fn workload_tags(&self) -> Vec<String> {
        vec![
            format!("op:{}", self.op.name()),
            format!("inputs:{}", self.n_in()),
        ]
    }
}

/// The fixed workload table: 34 kernels.
pub fn all() -> Vec<Kernel> {
    use Op::*;
    let mut v = Vec::new();
    for e in [Elem::F32, Elem::F64] {
        for op in [Sum, Dot, Sqdist, L1, SumSq, SaxpySum, MaxAbs, Min, Argmin] {
            v.push(Kernel::new(e, op));
        }
    }
    for op in [Sum, Dot, L1, Max, Min, CountEq, Argmin] {
        v.push(Kernel::new(Elem::I32, op));
    }
    for op in [
        Sum, CountEq, Hamming, Max, FindFirst, CountDigit, CountVowel, CountSpace, Hist,
    ] {
        v.push(Kernel::new(Elem::U8, op));
    }
    v
}

/// Training workloads per element type (stratified).
pub fn n_train(e: Elem) -> usize {
    match e {
        Elem::F32 => 3,
        Elem::F64 => 2,
        Elem::I32 => 2,
        Elem::U8 => 3,
    }
}

/// Names of TRAINING workloads: within each element type, the `n_train`
/// workloads with the smallest seeded FNV-1a hash of their name.
pub fn training_set(ks: &[Kernel]) -> Vec<String> {
    let mut out = Vec::new();
    for e in [Elem::F32, Elem::F64, Elem::I32, Elem::U8] {
        let mut names: Vec<(u64, String)> = ks
            .iter()
            .filter(|k| k.elem == e)
            .map(|k| (fnv1a(&format!("{SPLIT_SALT}{}", k.name())), k.name()))
            .collect();
        names.sort();
        out.extend(names.into_iter().take(n_train(e)).map(|(_, n)| n));
    }
    out.sort();
    out
}

/// Print the workload table (name, split, loop style, tags, applicable recipes).
pub fn describe() {
    let ks = all();
    let train = training_set(&ks);
    for k in &ks {
        let rs: Vec<&str> = crate::recipes::ALL
            .iter()
            .filter(|r| r.applies(k))
            .map(|r| r.id())
            .collect();
        println!(
            "{:<18} {:<7} {:<8} src={:?} wl={:?} recipes={}",
            k.name(),
            if train.contains(&k.name()) {
                "TRAIN"
            } else {
                "unseen"
            },
            format!("{:?}", k.style).to_lowercase(),
            k.source_tags(),
            k.workload_tags(),
            rs.join(",")
        );
    }
}
