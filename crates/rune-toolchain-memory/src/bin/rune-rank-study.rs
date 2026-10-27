//! Offline driver for frozen study fingerprints. Ranking never grants approval.
use rune_memory::{Fingerprint, HashedFeatureRanker, Ranker, Recipe, Snapshot};
use serde::Deserialize;
use std::{
    io::{self, Read},
    process::ExitCode,
};

#[derive(Deserialize)]
struct Input {
    recipes: Vec<Recipe>,
    queries: Vec<Fingerprint>,
}
fn main() -> ExitCode {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        let input: Input = serde_json::from_str(&s)?;
        let snap = Snapshot::from_recipes(
            "transfer-study",
            input.recipes.into_iter().map(Recipe::sealed).collect(),
        );
        let rankings: Vec<Vec<String>> = input
            .queries
            .iter()
            .map(|q| {
                HashedFeatureRanker
                    .rank(q, &snap, snap.recipes().len())
                    .into_iter()
                    .map(|s| {
                        snap.recipes()
                            .iter()
                            .find(|r| r.id == s.id)
                            .unwrap()
                            .descriptor
                            .clone()
                    })
                    .collect()
            })
            .collect();
        println!("{}", serde_json::to_string(&rankings)?);
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
