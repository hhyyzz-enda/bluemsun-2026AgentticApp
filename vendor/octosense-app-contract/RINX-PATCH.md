# Pending shared contract release

Exact source from companion App Hub commit `9854614` on
`feat/palpo-miniapp-contract`, based on App Hub 2bcb898. Adds the ADR 0010 exact service grants and their
consent language as version 1.2.0. No admission or integrity rule is relaxed.
Remove the root, standalone miniapp-catalog, system-apps and miniapp-package
Cargo patches when the same shared contract is published. All four consumers
require at least version 1.2 so build, admission and packaging agree.
