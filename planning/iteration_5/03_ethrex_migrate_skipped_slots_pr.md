## PR to ethrex (eip-7864 branch): make `migrate` count skipped storage slots

Upstream contribution, low effort. Not blocking our bootstrap — file once iteration_4a is done
and we've confirmed the behaviour on a real run.

## The bug

`migrate`'s collect phase tracks a `skipped` counter for **accounts** with a missing address
preimage, and warns (first 10) + reports the total at the end. But the **storage** branch
silently drops slots whose preimage is missing — no counter, no warning:

```rust
// cmd/ethrex/migrate.rs, collect loop (~line 478)
RawEntry::Storage { .. } => {
    if let Some(ProcessedEntry::Storage { tree_key, value_bytes }) = processed {
        inserts.push((tree_key.to_vec(), value_bytes.to_vec()));
        storage_count += 1;
    }
    // ← no else: a missing slot preimage vanishes (no skipped++, no warn!)
}
```

Consequence: storage-side preimage gaps are **invisible**. The reported `"{skipped} entries
skipped"` line and `storage_count` give no way to know how many slots were dropped; you can
only infer it by comparing `storage_count` against the input slot-entry count by hand (which is
exactly what we had to do — see iteration_4a_ethrex run, ~1.7% of *accounts* were skipped and
the storage gap was unknowable from the tool).

## The fix

Mirror the account path in the storage branch: on the `None` arm, `skipped += 1` and emit the
same capped warning (`"Skipped storage slot with keccak <addr>/<slot>, has_preimage=..."`).
Ideally split into `skipped_accounts` / `skipped_slots` so the end-of-collect summary reports
both, e.g. `"{skipped_accounts} accounts + {skipped_slots} slots skipped due to missing
preimages"`. Trivial, contained to `collect_phase` + the final `info!`/`warn!` in
`migrate_with_preimages`.

## Why it matters

For a verification node, every dropped leaf is an account/slot the binary trie won't contain →
a guaranteed binary-vs-MPT mismatch the equiv-daemon will later flag. Silent storage drops make
a structurally incomplete trie look clean at migrate time. The fix turns "skipped ≈ 0" into a
trustworthy completeness signal for both halves of the state.

## Status

Discovered 2026-06-22 during the iteration_4a mainnet migrate (memory-bound resort run). Our
fork carries this; once validated, port the patch upstream as a PR to the ethrex eip-7864
branch.
