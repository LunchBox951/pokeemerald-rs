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
where function.isUnsafe()
select function,
  "Review this unsafe function: document the caller safety contract and keep the unsafe boundary as small as practical."
