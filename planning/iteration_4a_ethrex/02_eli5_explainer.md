## ELI5 explainer (for non-technical colleagues)

Plain-language framing of the binary-node project + the current blocker/fix. Reusable for
sharing. Technical detail lives in the other iteration docs.

## What we're doing

Think of Ethereum as a giant shared **ledger** — a list of every account and how much it
holds. That ledger is organized in a particular filing system that lets anyone quickly
verify a balance is real.

There's a proposal to switch Ethereum to a **new, more efficient filing system** for that
same ledger. Before the network can adopt it, someone has to *prove* the new filing system
holds exactly the same balances as the old one — that nothing is lost or changed in the
reorganization.

**That's our job.** We're building a second copy of Ethereum's ledger in the new filing
system, kept up to date from a real Ethereum node, plus an automatic checker that constantly
compares "new copy vs. real ledger," account by account, every block. If they always match,
the new system is proven safe.

## The snag we hit

In the real ledger, accounts aren't filed under their names — they're filed under a
**scrambled code** of each name. The scrambling only goes one way: you can turn a name into
its code, but you can't turn a code back into a name.

To build the *new* filing system, we need the actual **names**, not the codes. We tried a
shortcut — a ready-made copy that was supposed to come with a "code → name" dictionary. But
that dictionary was only **~77% complete**: it was missing the names of millions of
"dormant" accounts that nobody had touched recently, so their names were never written down.
Missing names mean missing accounts, which would make our copy wrong.

## The fix (what we're doing right now)

The data team that hosts these copies pointed us to a different archive that **recorded
every account's real name every time it was ever used, going all the way back to Ethereum's
beginning** — so it has the *complete* list, dormant accounts included.

We're downloading that archive now. Once we have all the names, we can build the full,
correct copy in the new filing system and turn on the checker.

## Glossary (plain term → real term, for bridging to the technical docs)

- ledger / filing system → Ethereum state trie
- new filing system → EIP-7864 binary state tree
- second copy + checker → binary-node + equivalence daemon
- scrambled code of a name → keccak256 hash; "name" → raw address / storage slot key
- code → name dictionary → keccak preimages
- ready-made copy → ethPandaOps geth snapshot (snap-synced, ~77% preimages)
- the complete-history archive → Xatu canonical_execution data
