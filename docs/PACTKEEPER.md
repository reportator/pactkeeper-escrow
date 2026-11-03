# PactKeeper on top of the escrow program

How PactKeeper deals map onto this program (devnet, tPACT test token). Read this before wiring the server (step 4).

## One escrow per deal

| Role | On-chain |
| --- | --- |
| PactKeeper server | escrow `admin` (creates the escrow, sets extensions, then `SetImmutable`) |
| PactKeeper oracle | `Arbiter` extension — signs `Settle` and co-signs refunds |
| Contractor | `Beneficiary` extension — the only account `Settle` can pay |
| Client | depositor of every receipt |

Recommended setup order: `CreatesEscrow` → `AllowMint(tPACT)` → `SetArbiter(oracle)` → `SetBeneficiary(contractor)`
→ `SetArbiterTimeout(t)` → `SetImmutable` (nobody can change the rules after that, the server included).

## Outcomes

| Deal outcome | Instruction |
| --- | --- |
| Report accepted / stage approved | `Settle(payout_bps = 10000)` by the oracle |
| Settlement with a share (`payoutPct`) | `Settle(payout_bps = payoutPct × 100)`; the rest goes back to the client, rounding dust too |
| Cancelled / refund | `Withdraw` by the client, co-signed by the oracle (oracle first in remaining accounts) |
| Oracle gone | after `timeout` seconds from the deposit, `Withdraw` by the client **without** the oracle |

`Settle` closes the receipt, so a settled stage can be neither settled again nor refunded.

## Arbiter timeout (`SetArbiterTimeout`, discriminator 15, extension type 5)

- Data: `extensions_bump: u8`, `timeout: u64` seconds (> 0, ≤ i64::MAX). Admin only, while the escrow is mutable;
  calling it again updates the value; `RemoveExtension(5)` removes it.
- Counted per receipt from `deposited_at`. Before it expires, `Withdraw` needs the arbiter as before.
  From `deposited_at + timeout` on, the arbiter is not needed (an arbiter account passed anyway is skipped).
- A `Timelock`, if set, still applies on top.
- The oracle can still `Settle` after the timeout, until the client withdraws.
- Suggested value: campaign end − deposit time + 72 h review + 72 h dispute window + margin (e.g. 30 days after the campaign end).

## Staged payments: one receipt per stage

No extra instruction: each stage is its own deposit receipt.

- Receipt seed of stage `i` (0-based) = keypair from `sha256(contract_hash ‖ i)` (`Keypair::new_from_array`),
  where `contract_hash` is the 32-byte SHA-256 of the agreed contract text (smart deals: hash of the canonical terms).
  The server can always re-derive it; a stage cannot be funded twice because its receipt PDA would already exist.
- "Fund everything" = one transaction with all stage deposits; "fund the next stage" = one deposit.
- Approve stage `i` = `Settle(10000)` on its receipt; cancel the rest = refund each unreleased receipt.

Tests: `tests/integration-tests/src/test_set_arbiter_timeout.rs`, `test_pactkeeper_stages.rs`, `test_settle.rs`.
