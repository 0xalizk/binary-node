## Public status feed — live node/equivalence widget on privreads

First workstream after iteration_4a_ethrex. **Prerequisite: iteration_4a_ethrex complete** — binary-node
shadowing mainnet at the tip block-by-block, equiv-daemon running and capturing stats.

Goal: a live indicator on `https://privreads.ethereum.foundation/workstreams/ubt/` showing
the current block, sync status, and MPT-equivalence result — e.g. "block X · synced 🟢 ·
MPT-equivalence 🟢" with a pulsing green dot + updating block number that signal to a reader
this is a live system.

## Constraint that drives the design

The box is locked down: all services bound to `127.0.0.1`, reachable only via authenticated
Teleport, never directly public. A public website therefore **cannot poll the box**. → use a
**push model**: the box pushes a tiny status blob outbound to a public low-latency endpoint;
the privreads page fetches that.

privreads is on **GitHub Pages (static)**, which splits the work:
- the *widget* (client-side JS) is fine on Pages — it can `fetch()` a live feed cross-origin.
- the *feed* cannot be GitHub: receiving a ~10 s push means commit-spam, and raw/Pages are
  CDN-cached ~5 min (ignores cache-busting) → block number would lag ~5 min, killing the
  "live" feel and the freshness gate. So the feed needs one small dynamic endpoint elsewhere.

## Three pieces to build

1. **Box-side status exporter** (outbound only). Small process; reads the few series from the
   local Prometheus (`127.0.0.1:9091`, what the equiv-daemon already exports) and PUTs a tiny
   JSON to the sink every ~10–15 s. Status only, no secrets:
   ```json
   {
     "chain_head": 25340812,
     "binary_head": 25340812,
     "synced": true,
     "equiv_checked_block": 25340811,
     "equiv_ok": true,
     "discrepancies_total": 0,
     "updated_at": 1750000000
   }
   ```
2. **Public low-latency sink — Cloudflare Worker + KV** (recommended; coexists with GitHub
   Pages via CORS). Box does an authenticated `PUT` every ~10 s; Worker serves the JSON with
   `Cache-Control: no-store` + CORS. ~20 lines, no server to run. (GitHub-only fallback works
   but is ~5-min laggy — only if real-time isn't wanted.) **This is the one piece needing EF
   provisioning (Cloudflare account/Worker/KV).**
3. **Widget JS** in the Pages page. Polls the sink every ~10 s; renders block number + two
   indicators (synced, MPT-equivalence) + the pulsing dot.

## Honesty / freshness gate (non-negotiable)

The green/blink must reflect *actual* liveness, not a frozen snapshot. Compute
`fresh = now - updated_at < ~30 s`:
- pulsing green only when `fresh && synced && equiv_ok`
- red when a check fails (`!synced` or `!equiv_ok`)
- grey "stale · last seen N s ago" when the feed goes quiet (box down / crashed / fell behind)

Never show steady-green on a dead feed.

## What iteration_4a_ethrex's equiv-daemon must expose (so this is a small add-on)

The daemon already needs head / equiv / discrepancy state for the Grafana dashboard — surface
the same as: `chain_head`, `binary_head`, `synced`, `equiv_checked_block`, `equiv_ok`,
`discrepancies_total`, `updated_at`. Then exporter + widget are thin.

## Deliverables when built

Box-side pusher; the ~20-line Cloudflare Worker; the widget HTML/JS. Only blocker is
provisioning the Worker/KV (EF infra decision).
