# ADR 012: Stable product selection and workspace paths

Status: implemented with failing-before, passing-after regression tests.

The Cargo adapter previously picked the last compiler-artifact message. A
two-message counterexample showed that reversing completion order selected a
different binary. Multi crate build scheduling is not part of artifact identity.

Select deterministically from workspace source artifacts: prefer executables,
then use lexicographic path order. Exclude build scripts and dependency packages
outside the workspace source root. This is an explicitly selected single product,
not a digest of the complete output set. Manifests and source members outside the
workspace tree remain outside this validation and need a separate boundary design.

A second regression showed that relative -C paths were applied twice: Rune
derived a relative target path, then Cargo interpreted it beneath its changed
working directory. Canonicalize the workspace once at the command boundary so
store, target and ownership paths are absolute. Caller supplied contract/report
paths retain their existing invocation relative semantics.

Acceptance: permuting Cargo completion messages selects the same product;
dependency binaries and build scripts cannot replace a workspace product;
relative -C builds into that workspace's target/rune directory; normal fixture
replay and authenticated promotion continue to pass. Verify real workspace
selected-product parity against plain Cargo and a fresh rebuild separately.
