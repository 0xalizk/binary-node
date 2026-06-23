## iteration_4b_geth — alternative route: an EIP-7864-compliant geth

Parallel exploration while iteration_4a_ethrex is blocked on provisioning (the ethrex-fork
migrate is RAM-bound / days-long on the current box).

**Hypothesis:** the go-ethereum team has been adding EIP-7864 (unified binary state tree)
support. If geth can natively sync/produce EIP-7864 state — or convert MPT→binary itself —
that could be a simpler/faster shadow-node path than bootstrapping our ethrex fork via the
snapshot+preimage migrate.

**Plan:**
1. Deep-research `github.com/ethereum/go-ethereum` (+ gballet forks, verkle/binary-tree work):
   what EIP-7864 support exists, where (branch/PR/commit), maturity, how to build/run, and
   whether it can reach mainnet binary-tree state. → `01_geth_eip7864_findings.md`.
2. Decide if 4b is viable; if so, experiment (build, try to sync/convert).

Relation to 4a: 4a (ethrex fork) remains the primary, paused on provisioning. 4b is a hedge.
