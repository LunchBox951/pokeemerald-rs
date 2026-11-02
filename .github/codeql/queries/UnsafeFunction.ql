/**
 * @name Review unsafe Rust function
 * @description Tracks each unsafe function declaration for deliberate review of its caller contract and safe alternatives.
 * @kind problem
 * @problem.severity warning
 * @precision high
 * @id pokeemerald-rs/unsafe-function
 * @tags maintainability
 */

import codeql.rust.elements

from Function function
where
  function.isUnsafe() and
  // Policy: test-only shims are out of scope. Files under a `tests` directory
  // never ship, and their unsafe code (for example the counting global
  // allocator in `rom_build/tests/checkout_reading.rs`) cannot be removed
  // without dropping the test. They still carry `// SAFETY:` comments and
  // `# Safety` docs for human review, but this query does not read comments,
  // so the path decides. Mirrored in UnsafeBlock.ql and UnsafeFunction.ql.
  not function.getLocation().getFile().getRelativePath().regexpMatch("(.*/)?tests/.*")
select function,
  "Review this unsafe function: document the caller safety contract and keep the unsafe boundary as small as practical."
