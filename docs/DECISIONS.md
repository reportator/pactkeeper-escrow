# Decision log — PactKeeper escrow (public repo: technical decisions only)

Format: one line per decision, newest first. Never delete: mark "❌ superseded <date>, see #N".

| # | Date | Decision | Why / alternatives | Where |
|---|---|---|---|---|
| 3 | 2026-11-03 | Runs on real Solana devnet (not a local validator); repository is public | Real network behaviour for the beta; transparency of the contract | — |
| 2 | 2026-10-31 | Refund goes only to the client and needs the arbiter (oracle) signature; after a timeout a refund is possible without the oracle | Neither side can take funds alone; no lock-up if the oracle disappears | 0fed3b5, 7e25a7e |
| 1 | 2026-10-31 | Extend the audited solana-foundation/escrow fork instead of writing a separate Anchor program | Reuse audited code | 0fed3b5 |
