use crate::fsio::{load, reject, write_atomic};
use rune_core::edit::{EditApplier, EditOutcome, EditRequest};
use rune_core::{hex_sha256, Error};

/// Pure-Rust applier: `find` must occur exactly once in the file.
#[derive(Debug, Default, Clone, Copy)]
pub struct ExactReplace;

impl EditApplier for ExactReplace {
    fn name(&self) -> &'static str {
        "exact_replace"
    }

    fn apply(&self, req: &EditRequest) -> Result<EditOutcome, Error> {
        let file = load(req)?;
        if req.find.is_empty() {
            return Err(reject("find text is empty"));
        }
        let n = file.text.matches(req.find.as_str()).count();
        if n != 1 {
            return Err(reject(format!(
                "find text must occur exactly once, found {n} times"
            )));
        }
        let new = file.text.replacen(req.find.as_str(), &req.replace, 1);
        if new.len() as u64 > req.max_bytes {
            return Err(reject("edited file would exceed the size bound"));
        }
        write_atomic(&file.path, &file.sha256, new.as_bytes())?;
        Ok(EditOutcome {
            applier: self.name().into(),
            before_sha256: file.sha256,
            after_sha256: hex_sha256(new.as_bytes()),
            confidence: None,
        })
    }
}
