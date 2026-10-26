//! Append-only cost ledger (ADR 008). Money is tracked in micro-USD so sums
//! are exact. Every campaign reserves before it spends and reconciles after.

use crate::{Error, Policy};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Reserve,
    Release,
    Spend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub campaign: String,
    pub kind: Kind,
    pub usd_micros: u64,
    pub tokens: u64,
    /// For `Release`/`Spend`, the `seq` of the reservation being settled.
    pub reservation: Option<u64>,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    pub open_reserved_usd_micros: u64,
    pub open_reserved_tokens: u64,
    pub spent_usd_micros: u64,
    pub spent_tokens: u64,
    /// Spend above its reservation (provider charged more than reserved).
    pub overrun_usd_micros: u64,
}

pub fn usd_to_micros(usd: f64) -> u64 {
    (usd.max(0.0) * 1_000_000.0).round() as u64
}

pub struct Ledger {
    path: PathBuf,
    campaign: String,
    max_usd_micros: u64,
    max_tokens: u64,
}

impl Ledger {
    /// `.rune/ledger/<campaign>.jsonl`. Campaign ids are `[A-Za-z0-9_-]{1,64}`.
    pub fn open(workspace: &Path, campaign: &str, policy: &Policy) -> Result<Self, Error> {
        policy.validate()?;
        let ok = !campaign.is_empty()
            && campaign.len() <= 64
            && campaign
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err(Error::Policy(
                "campaign id must be 1-64 chars of [A-Za-z0-9_-]".into(),
            ));
        }
        let path = workspace
            .join(".rune/ledger")
            .join(format!("{campaign}.jsonl"));
        Ok(Ledger {
            path,
            campaign: campaign.into(),
            max_usd_micros: usd_to_micros(policy.max_usd),
            max_tokens: policy.max_tokens,
        })
    }

    pub fn entries(&self) -> Result<Vec<Entry>, Error> {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(Error::io(&self.path, e)),
        };
        text.lines()
            .filter(|l| !l.is_empty())
            .map(|l| Ok(serde_json::from_str(l)?))
            .collect()
    }

    pub fn totals(&self) -> Result<Totals, Error> {
        let entries = self.entries()?;
        let mut t = Totals::default();
        let mut open: std::collections::BTreeMap<u64, (u64, u64)> = Default::default();
        for e in &entries {
            match e.kind {
                Kind::Reserve => {
                    open.insert(e.seq, (e.usd_micros, e.tokens));
                }
                Kind::Release => {
                    if let Some(r) = e.reservation {
                        open.remove(&r);
                    }
                }
                Kind::Spend => {
                    t.spent_usd_micros += e.usd_micros;
                    t.spent_tokens += e.tokens;
                    if let Some(r) = e.reservation {
                        if let Some((ru, _)) = open.remove(&r) {
                            t.overrun_usd_micros += e.usd_micros.saturating_sub(ru);
                        }
                    }
                }
            }
        }
        t.open_reserved_usd_micros = open.values().map(|v| v.0).sum();
        t.open_reserved_tokens = open.values().map(|v| v.1).sum();
        Ok(t)
    }

    fn append(
        &self,
        kind: Kind,
        usd_micros: u64,
        tokens: u64,
        reservation: Option<u64>,
        note: &str,
    ) -> Result<u64, Error> {
        let seq = self.entries()?.len() as u64 + 1;
        let entry = Entry {
            seq,
            campaign: self.campaign.clone(),
            kind,
            usd_micros,
            tokens,
            reservation,
            note: note.into(),
        };
        fs::create_dir_all(self.path.parent().expect("has parent"))
            .map_err(|e| Error::io(&self.path, e))?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| Error::io(&self.path, e))?;
        let mut line = serde_json::to_vec(&entry)?;
        line.push(b'\n');
        f.write_all(&line)
            .and_then(|_| f.sync_all())
            .map_err(|e| Error::io(&self.path, e))?;
        Ok(seq)
    }

    /// Reserve budget before any paid call. Fails if spent + open + request would exceed the policy.
    pub fn reserve(&self, usd: f64, tokens: u64, note: &str) -> Result<u64, Error> {
        let t = self.totals()?;
        let want = usd_to_micros(usd);
        if t.spent_usd_micros + t.open_reserved_usd_micros + want > self.max_usd_micros
            || t.spent_tokens + t.open_reserved_tokens + tokens > self.max_tokens
        {
            return Err(Error::Policy(
                "budget exhausted: reservation would exceed policy".into(),
            ));
        }
        self.append(Kind::Reserve, want, tokens, None, note)
    }

    /// Settle a reservation with the provider-reported actuals.
    pub fn settle(&self, reservation: u64, usd: f64, tokens: u64, note: &str) -> Result<(), Error> {
        self.append(
            Kind::Spend,
            usd_to_micros(usd),
            tokens,
            Some(reservation),
            note,
        )?;
        Ok(())
    }

    /// Return a reservation unspent (the call failed before the provider charged anything).
    pub fn release(&self, reservation: u64, note: &str) -> Result<(), Error> {
        self.append(Kind::Release, 0, 0, Some(reservation), note)?;
        Ok(())
    }

    /// The ledger must equal the provider-reported totals exactly (micro-USD, tokens).
    pub fn reconcile(&self, provider_usd: f64, provider_tokens: u64) -> Result<(), Error> {
        let t = self.totals()?;
        if t.open_reserved_usd_micros != 0 {
            return Err(Error::Policy("unsettled reservations remain".into()));
        }
        if t.spent_usd_micros != usd_to_micros(provider_usd) || t.spent_tokens != provider_tokens {
            return Err(Error::Policy(format!(
                "ledger {}µ$/{}tok != provider {}µ$/{}tok",
                t.spent_usd_micros,
                t.spent_tokens,
                usd_to_micros(provider_usd),
                provider_tokens
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(usd: f64, tokens: u64) -> Policy {
        Policy {
            max_usd: usd,
            max_tokens: tokens,
            max_seconds: 60,
            max_memory_mb: 64,
        }
    }

    #[test]
    fn reserve_settle_reconcile() {
        let d = tempfile::tempdir().unwrap();
        let l = Ledger::open(d.path(), "c1", &policy(2.0, 10_000)).unwrap();
        let r = l.reserve(0.50, 1000, "proposal").unwrap();
        l.settle(r, 0.0123, 812, "usage").unwrap();
        l.reconcile(0.0123, 812).unwrap();
        assert!(l.reconcile(0.02, 812).is_err());
    }

    #[test]
    fn over_budget_reservation_rejected() {
        let d = tempfile::tempdir().unwrap();
        let l = Ledger::open(d.path(), "c1", &policy(1.0, 10_000)).unwrap();
        l.reserve(0.6, 100, "a").unwrap();
        assert!(
            l.reserve(0.6, 100, "b").is_err(),
            "open reservations count against the budget"
        );
    }

    #[test]
    fn release_frees_budget_and_open_reservation_blocks_reconcile() {
        let d = tempfile::tempdir().unwrap();
        let l = Ledger::open(d.path(), "c1", &policy(1.0, 10_000)).unwrap();
        let r = l.reserve(0.9, 100, "a").unwrap();
        assert!(l.reconcile(0.0, 0).is_err());
        l.release(r, "provider error").unwrap();
        l.reserve(0.9, 100, "b").unwrap();
    }

    #[test]
    fn overrun_is_recorded_not_hidden() {
        let d = tempfile::tempdir().unwrap();
        let l = Ledger::open(d.path(), "c1", &policy(5.0, 10_000)).unwrap();
        let r = l.reserve(0.1, 100, "a").unwrap();
        l.settle(r, 0.25, 90, "provider charged more").unwrap();
        assert_eq!(l.totals().unwrap().overrun_usd_micros, 150_000);
    }

    #[test]
    fn bad_campaign_id_rejected() {
        let d = tempfile::tempdir().unwrap();
        assert!(Ledger::open(d.path(), "../x", &policy(1.0, 1)).is_err());
    }
}
