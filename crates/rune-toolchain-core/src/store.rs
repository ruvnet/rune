use crate::{Error, Plan, Receipt};
use std::fs;
use std::path::{Path, PathBuf};

/// `.rune/` layout: `plans/<id>.json`, `receipts/<id>.json`, `history` (one
/// approved plan id per line, newest last). The newest history line is current.
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(workspace: &Path) -> Self {
        Store {
            root: workspace.join(".rune"),
        }
    }

    fn write(&self, rel: &str, data: &[u8]) -> Result<(), Error> {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().expect("has parent")).map_err(|e| Error::io(&path, e))?;
        fs::write(&path, data).map_err(|e| Error::io(&path, e))
    }

    fn history(&self) -> Result<Vec<String>, Error> {
        let path = self.root.join("history");
        match fs::read_to_string(&path) {
            Ok(s) => Ok(s.lines().map(str::to_owned).collect()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    fn set_history(&self, ids: &[String]) -> Result<(), Error> {
        self.write("history", format!("{}\n", ids.join("\n")).as_bytes())
    }

    /// Persist the plan and make it current. Returns its id.
    pub fn approve(&self, plan: &Plan) -> Result<String, Error> {
        plan.policy.validate()?;
        let id = plan.id();
        self.write(
            &format!("plans/{id}.json"),
            &serde_json::to_vec_pretty(plan)?,
        )?;
        let mut h = self.history()?;
        h.push(id.clone());
        self.set_history(&h)?;
        Ok(id)
    }

    pub fn load(&self, id: &str) -> Result<Plan, Error> {
        let valid = !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid {
            return Err(Error::IdMismatch {
                claimed: id.into(),
                actual: "not a hex digest".into(),
            });
        }
        let path = self.root.join(format!("plans/{id}.json"));
        let plan: Plan =
            serde_json::from_slice(&fs::read(&path).map_err(|e| Error::io(&path, e))?)?;
        plan.check_schema()?;
        if plan.id() != id {
            return Err(Error::IdMismatch {
                claimed: id.into(),
                actual: plan.id(),
            });
        }
        Ok(plan)
    }

    pub fn current_id(&self) -> Result<Option<String>, Error> {
        Ok(self.history()?.pop())
    }

    pub fn current(&self) -> Result<Plan, Error> {
        let id = self.current_id()?.ok_or(Error::NoPlan("use"))?;
        self.load(&id)
    }

    /// Drop the current plan from history; the previous one becomes current.
    /// Plan files are retained as evidence.
    pub fn rollback(&self) -> Result<String, Error> {
        let mut h = self.history()?;
        if h.len() < 2 {
            return Err(Error::NoPlan("roll back to"));
        }
        h.pop();
        self.set_history(&h)?;
        Ok(h.last().cloned().expect("len>=1"))
    }

    pub fn record(&self, r: &Receipt) -> Result<(), Error> {
        self.write(
            &format!("receipts/{}.json", r.plan_id),
            &serde_json::to_vec_pretty(r)?,
        )
    }

    pub fn receipt(&self, id: &str) -> Result<Option<Receipt>, Error> {
        let path = self.root.join(format!("receipts/{id}.json"));
        match fs::read(&path) {
            Ok(b) => Ok(Some(serde_json::from_slice(&b)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(path, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Policy;

    fn plan(src: &str, parent: Option<String>) -> Plan {
        let pol = Policy {
            max_usd: 2.0,
            max_tokens: 1000,
            max_seconds: 60,
            max_memory_mb: 64,
        };
        Plan::new(
            src.into(),
            "l".into(),
            "rustc 1".into(),
            "x86_64".into(),
            pol,
            parent,
        )
    }

    #[test]
    fn approve_rollback_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path());
        let a = s.approve(&plan("a", None)).unwrap();
        let b = s.approve(&plan("b", Some(a.clone()))).unwrap();
        assert_eq!(s.current_id().unwrap().as_deref(), Some(b.as_str()));
        assert_eq!(s.rollback().unwrap(), a);
        assert_eq!(s.current().unwrap().source_digest, "a");
        assert!(
            s.rollback().is_err(),
            "cannot roll back past the first plan"
        );
    }

    #[test]
    fn tampered_plan_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path());
        let id = s.approve(&plan("a", None)).unwrap();
        let p = d.path().join(format!(".rune/plans/{id}.json"));
        let t = std::fs::read_to_string(&p)
            .unwrap()
            .replace("\"a\"", "\"evil\"");
        std::fs::write(&p, t).unwrap();
        assert!(matches!(s.load(&id), Err(Error::IdMismatch { .. })));
    }

    #[test]
    fn path_traversal_id_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        assert!(Store::open(d.path()).load("../../etc/passwd").is_err());
    }

    #[test]
    fn zero_budget_rejected() {
        let d = tempfile::tempdir().unwrap();
        let mut p = plan("a", None);
        p.policy.max_usd = 0.0;
        assert!(matches!(
            Store::open(d.path()).approve(&p),
            Err(Error::Policy(_))
        ));
    }

    #[test]
    fn stale_source_rejected() {
        let p = plan("a", None);
        assert!(matches!(
            p.check_fresh("b", "l", "rustc 1"),
            Err(Error::Stale {
                field: "source_digest",
                ..
            })
        ));
        assert!(p.check_fresh("a", "l", "rustc 1").is_ok());
    }
}
