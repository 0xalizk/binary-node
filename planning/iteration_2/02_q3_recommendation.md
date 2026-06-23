# Q3 resolved + the equiv-check timing problem (needs your call)

## Q3 (start state / sync strategy) — RESOLVED: genesis full re-execution

Your proposed "option 3" (snap-sync but apply to a binary trie) is **not feasible**,
confirmed by the branch code:

- Snap delivers MPT state keyed by `keccak(address)` / `keccak(slot)` — **hashed**.
- The binary trie keys on the **real** address/slot (`get_tree_key` takes plaintext
  `Address`/`U256`). Reversing keccak needs preimages.
- The branch's only way to get preimages from a *snapshot* is a **Geth** preimage DB
  + patched Geth code-exporter. We run ethrex, not Geth.

So the realistic, ethrex-only path is the one you instinctively wanted: **binary-node
builds its state by re-executing blocks from genesis** (path a). Execution yields
real keys, so no preimage problem. This is the proven branch path (10k Hoodi blocks
from genesis).

**The catch (be eyes-open):** full mainnet re-execution from genesis (~25.3M blocks)
is the *slow* path — it's not comparable to a normal ethrex sync, which is fast only
*because* it's snap (skips execution). On this shared 8-core box, catch-up to tip is
realistically **weeks**, and mainnet state correctness on this branch is **unproven**
(Hoodi-only). This is the dominant risk of the whole project.

## The equiv-check timing problem (NEW — your decision)

You want equiv-daemon to "shadow binary-node since the first block" with **value-level
checks** (compare `eth_getBalance` etc. on both nodes). But:

- binary-node answers state only for the **last ~128 blocks** (not archive).
- **mainnet-node is also NOT archive** — it only retains recent state (~128 blocks).
- During catch-up, binary-node is processing block N where N ≪ mainnet head
  (e.g. binary at 5M while mainnet is at 25.3M). **Neither node can answer a
  value query at block 5M** — mainnet-node pruned it long ago.

So **value-level equivalence at every historical block is impossible without an
archive MPT reference.** There are three ways forward — pick one:

### Option 1 (recommended) — two-phase equivalence
- **Catch-up phase (genesis→tip):** the equivalence signal is the branch's per-block
  **intrinsic validation** that runs for free: `receipts_root`, `gas_used`,
  `requests_hash`, and `block_access_list_hash` are all checked every block (only the
  binary state *root* is skipped). These are **execution-derived** — if binary-node
  read/wrote state wrong, they'd diverge. equiv-daemon records pass/fail + metrics
  (state size, exec speed) per block. This is "shadowing from block 0," just with an
  execution-equivalence check rather than a value-by-value RPC diff.
- **Tip phase (steady state, indefinite):** once binary-node reaches tip, both nodes
  have recent state, so equiv-daemon does the full **value-level** check per new
  block — touched keys → `getBalance`/`getNonce`/`getCode`/`getStorageAt` on both →
  green/red. This is the rich check you described, running forever.
- **Cost:** none beyond the base project. **Limitation:** no value-by-value diff for
  historical blocks (covered instead by receipts/gas/BAL equivalence).

### Option 2 — full historical value-level checks (most rigorous, expensive)
- Stand up an **archive MPT reference** (re-sync mainnet-node, or a 3rd ethrex, in
  archive mode) so we can value-diff every historical block.
- **Cost:** archive mainnet is ~2–3 TB extra disk and a long archive sync; more
  CPU/IO contention on the one box. Likely impractical here, but it's your call.

### Option 3 — lockstep MPT vs binary re-exec
- Run a **second MPT ethrex** that re-executes genesis→tip in lockstep with
  binary-node; compare each block's `AccountUpdate` diffs as they execute (no
  historical queries needed).
- **Cost:** doubles the (already long) catch-up compute — three execution nodes on
  8 cores. Strongest theoretical guarantee, worst resource fit.

## My recommendation

**Option 1.** It satisfies your three "done" criteria, needs no extra hardware, and
the catch-up intrinsic checks (receipts/gas/BAL) are a genuine state-equivalence
signal — not a hand-wave. We can always add Option 2's archive reference later if you
want value-by-value historical coverage.

**Please confirm Option 1 (or pick 2/3)** — it's the last thing I need to finalize
the plan. Everything else is locked.
