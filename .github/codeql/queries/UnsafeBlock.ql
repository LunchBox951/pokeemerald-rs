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
where block.isUnsafe()
select block,
  "Review this unsafe block: record the invariant that makes the operation sound and keep the scope minimal."
