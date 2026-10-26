use rune_memory::control::{ControlStudy, RecipeSpec, Verdict, WorkloadSpec};
use rune_memory::{Fingerprint, HashedFeatureRanker};

fn fp(cluster: usize) -> Fingerprint {
    Fingerprint {
        source_tags: vec![format!("s{cluster}")],
        target_triple: "x86_64".into(),
        workload_tags: vec![format!("w{cluster}")],
        ..Default::default()
    }
}

struct Lcg(u64);
impl Lcg {
    fn chance(&mut self, p: f64) -> bool {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) < p
    }
}

/// `clusters` clusters; `train_per` training and `test_per` unseen workloads
/// each; one library recipe per training workload.
fn study(
    clusters: usize,
    train_per: usize,
    test_per: usize,
    k: usize,
    informative: bool,
) -> ControlStudy {
    let mut workloads = vec![];
    let mut cluster_of = vec![];
    for c in 0..clusters {
        for i in 0..train_per + test_per {
            workloads.push(WorkloadSpec {
                name: format!("c{c}-{i}"),
                training: i < train_per,
                fingerprint: fp(c),
            });
            cluster_of.push(c);
        }
    }
    let mut recipes = vec![];
    let mut recipe_cluster = vec![];
    for (w, spec) in workloads.iter().enumerate().filter(|(_, s)| s.training) {
        recipes.push(RecipeSpec {
            id: format!("r-{}", spec.name),
            origin: spec.name.clone(),
            fingerprint: fp(cluster_of[w]),
        });
        recipe_cluster.push(cluster_of[w]);
    }
    let mut rng = Lcg(7);
    let outcomes = recipe_cluster
        .iter()
        .map(|&rc| {
            cluster_of
                .iter()
                .map(|&wc| {
                    if informative {
                        rc == wc
                    } else {
                        rng.chance(0.25)
                    }
                })
                .collect()
        })
        .collect();
    ControlStudy {
        k,
        trials: 500,
        seed: 42,
        workloads,
        recipes,
        outcomes,
    }
}

#[test]
fn retrieval_genuinely_better_is_detected() {
    let rep = study(10, 2, 4, 2, true).run(&HashedFeatureRanker).unwrap();
    assert_eq!((rep.n_workloads, rep.n_recipes), (40, 20));
    assert_eq!(rep.verdict, Verdict::Better, "{}", rep.render_markdown());
    assert!(rep.retrieval_wilson95.0 > rep.random_wilson95.1);
    assert!(rep.retrieval_mean_evals_to_first_accept < rep.random_mean_evals_to_first_accept);
    assert!(rep.p_value < 0.05);
}

#[test]
fn uninformative_outcomes_give_no_difference() {
    let rep = study(10, 2, 4, 3, false).run(&HashedFeatureRanker).unwrap();
    assert_eq!(
        rep.verdict,
        Verdict::NoDifference,
        "{}",
        rep.render_markdown()
    );
    assert!(rep.render_markdown().contains("do NOT claim"));
}

#[test]
fn tiny_n_is_insufficient_even_when_retrieval_wins_everything() {
    let rep = study(2, 2, 2, 1, true).run(&HashedFeatureRanker).unwrap();
    assert_eq!(rep.n_workloads, 4);
    assert_eq!(rep.retrieval_hit_rate, 1.0);
    assert_eq!(rep.verdict, Verdict::Insufficient);
    assert!(rep.render_markdown().contains("4 unseen workloads"));
}

#[test]
fn library_excludes_non_training_origins_and_runs_are_reproducible() {
    let mut s = study(10, 2, 4, 2, true);
    let extra_origin = s
        .workloads
        .iter()
        .find(|w| !w.training)
        .unwrap()
        .name
        .clone();
    s.recipes.push(RecipeSpec {
        id: "leak".into(),
        origin: extra_origin,
        fingerprint: fp(0),
    });
    s.outcomes.push(vec![true; s.workloads.len()]);
    let a = s.run(&HashedFeatureRanker).unwrap();
    assert_eq!((a.n_recipes, a.n_excluded_recipes), (20, 1));
    assert_eq!(a, s.run(&HashedFeatureRanker).unwrap());
}

#[test]
fn json_round_trip_and_validation() {
    let s = study(10, 2, 4, 2, true);
    let text = serde_json::to_string(&s).unwrap();
    assert_eq!(ControlStudy::from_json(&text).unwrap(), s);
    let mut bad = s.clone();
    bad.outcomes.pop();
    assert!(ControlStudy::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
    let mut zero = s;
    zero.k = 0;
    assert!(ControlStudy::from_json(&serde_json::to_string(&zero).unwrap()).is_err());
}
