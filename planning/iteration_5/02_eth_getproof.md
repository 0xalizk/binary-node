## Implement eth_getProof on the binary-node (future)

Deferred workstream — **do after** iteration_4a_ethrex (bootstrap + equiv-daemon) and the status feed
(`01_status_feed.md`). Not needed for the equivalence check (that compares values, not proofs).

### Goal

Make the binary-node serve real `eth_getProof` responses — i.e. actual **EIP-7864 binary-trie
proofs**, so the node can provide verifiable state proofs (the eventual point of running it as
a data source).

### Current state (as of 2026-06-21)

The fork's handler is wired but returns **values without proofs**:
- `crates/networking/rpc/eth/account.rs:192` (`GetProofRequest`) returns correct `balance`,
  `nonce`, `code_hash`, and each storage slot `value` (FKV-backed, within the ~128-block
  window), BUT `account_proof: []`, each storage `proof: []`, and `storage_hash: 0x0`.
- Code comments: *"Binary trie proofs use sibling hashes, not MPT-encoded nodes. Proof
  generation is not yet wired into this RPC handler."*
- (Stock ethrex serves full MPT proofs; the fork does not, by omission.)

### What it requires

- Binary-trie proof generation: produce the sibling-hash proof for a leaf in the EIP-7864
  binary tree (format differs fundamentally from MPT-node proofs). The trie crate
  (`crates/common/binary_trie/`, has `proof.rs`/`merkle.rs`) likely already has, or is the
  place for, the proof primitive — wire it into the RPC handler.
- Populate `account_proof`, per-slot `proof`, and a meaningful `storage_hash` in
  `GetProofRequest::handle`.
- Define/confirm the response encoding for binary-trie proofs (no standard yet — decide a
  serialization; document it so verifiers can consume it).
- Tests: prove a returned proof verifies against the binary-trie root.

### Scope / priority

Moderate; isolated to the RPC handler + trie proof primitive. Independent of the
bootstrap/equiv work. Schedule after the node is shadowing + the status feed is live.
