/**
 * @name Review unsafe Rust block
 * @description Tracks each unsafe block for deliberate review of its local safety justification and safe alternatives.
 * @kind problem
 * @problem.severity warning
 * @precision high
 * @id pokeemerald-rs/unsafe-block
 * @tags maintainability
 */

import codeql.rust.elements

from BlockExpr block
where
  block.isUnsafe() and
  // Policy: test-only shims are out of scope. Files under a `tests` directory
  // never ship, and their unsafe code (for example the counting global
  // allocator in `rom_build/tests/checkout_reading.rs`) cannot be removed
  // without dropping the test. They still carry `// SAFETY:` comments and
  // `# Safety` docs for human review, but this query does not read comments,
  // so the path decides. Mirrored in UnsafeBlock.ql and UnsafeFunction.ql.
  not block.getLocation().getFile().getRelativePath().regexpMatch("(.*/)?tests/.*")
select block,
  "Review this unsafe block: record the invariant that makes the operation sound and keep the scope minimal."
