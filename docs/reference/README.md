## reference

Durable technical specs a replicator/maintainer needs. During the build, the authoritative
copies live in the planning journal (linked below); they'll be inlined here as standalone
reference pages for public release, once stable from the mainnet run.

- **gethdbdump format** — byte-exact format `ethrex migrate` consumes (header, op byte, RLP
  framing; preimage value-length tagging; slim-account encoding; storage value encoding).
  Source of record: `../../planning/iteration_3/06_gethdump_format.md`.
- **ethrex `migrate`** — the command, its three inputs, `--at-block` semantics, the BLAKE3
  key derivation, scale/memory behavior. Source of record:
  `../../planning/iteration_3/03_ethrex_migrate_findings.md`.
- **client schemas / preimage availability** — why reth v2.2.0 has no preimages and geth's
  snap-synced set is ~77%. Source of record:
  `../../planning/iteration_3/04_reth_schema_findings.md`,
  `07_CRITICAL_reth_has_no_preimages.md`, `08_CRITICAL_geth_preimages_incomplete.md`.

Inline (don't just link) before any public release so `docs/` stands alone.
