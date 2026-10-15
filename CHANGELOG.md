# Changelog

## v1.2: SauceHole

New program `sauce_hole`, `Ho1ezeQ2fb1zEL3aK94e4q5j7g3vwGfi5PguPWGjB2VC`. The `stonksauce` program is unchanged.

- One hole per deployment (PDA `4Z3tMJTnRuQw7XSqLA1JrcHPAj3mKfL2rLAGcCJrGM7h`) owns a STONKSAUCE vault and a STONK reward vault. No instruction moves STONKSAUCE out of the vault.
- `feed`: anyone locks STONKSAUCE in the hole; each feeder gets an on-chain record.
- `get_sauced`: the keeper spends STONK rewards through the whitelisted swap program (Jupiter v6 on mainnet). The swap authority PDA is approved as a delegate for exactly `amount_in`, signs the CPI, and the approval is revoked afterwards. Both vaults must still be owned by the hole with no delegate or close authority, at most `amount_in` may be spent, and at least `min_out` STONKSAUCE must arrive.
- Only the swap authority signs inside the swap CPI; outer signers are never forwarded (SH-1).
- Admin: two-step transfer, keeper, swap program, minimum feed, pause. Pause blocks GetSauced only; feeding always works.

Security: see "2026-10: SauceHole program" in [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md).

Reproducible build: `solana-verify build --library-name sauce_hole --arch v3 --base-image solanafoundation/solana-verifiable-build:4.1.0`, executable hash `7d30fa4355ebb1696ad10ccff3e86516c4d9f1db4bed59c936b73d70c46be2f5`.

## v1.1

Switchboard shut down (announced 2026-09-19, support ended 2026-09-25), so draws could no longer get randomness. No mainnet draw had run under Switchboard, so no pending draw needed migrating. This release moves draws to ORAO VRF (`VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y`).

- `request_draw(nonce)` CPIs ORAO `request_v2` with `seed = sha256("stonksauce-draw" || jar || round_index || nonce)`. The request account must be ORAO's PDA for that seed, and the ORAO program and network state are pinned.
- `fulfill_draw` reads the fulfilled ORAO request: owner, `RandomnessV2` discriminator, seed and a non-zero value are checked. Anyone can settle once ORAO has answered, and nobody can see the value earlier, so the keeper can no longer withhold an unfavourable result.
- `cancel_draw` takes the request account and refuses with the new `DrawFulfilled` (6038) once ORAO has answered, so a fulfilled draw can never be cancelled and re-rolled. A draw ORAO has not answered still unlocks after one hour.
- The jar field and `DrawCancelled` field `commit_slot` are renamed `draw_nonce` (same offset; the `Jar` layout is unchanged). Other error codes are unchanged.
- The `devnet` cargo feature is removed; every cluster runs one binary.
- `mock_orao` replaces `mock_switchboard` for local tests.

Security: see "2026-10: switch to ORAO VRF" in [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md). Randomness now trusts ORAO's quorum of fulfilment authorities.

Reproducible build: `solana-verify build --library-name stonksauce --arch v3 --base-image solanafoundation/solana-verifiable-build:4.1.0`, executable hash `16ef4ac1a68a66b99c4453389d68e84f8205e73dbb5ba5d07d2a8bd77dfdd23e`.

## v1.0

First mainnet release of the `stonksauce` program, `FhGABn7RcdhGc4CrNx4ohDdAsFUFiHz48y4QDMfxe2UF`.

- Jars: one per stake mint, owning the stake, reward, fee and SAUCE vaults.
- Time-weighted draw odds: a deposit counts only for the part of the round left.
- Sharded Fenwick trees: up to 196,224 depositors per jar, O(log n) weighted picks.
- Switchboard On-Demand randomness with commit and reveal-slot checks; stuck draws are cancellable by anyone after one hour.
- 95% of the pot to the winner, 5% to the fee vault for SAUCE buyback and burn.
- Per-jar limits: minimum deposit, absolute maximum per wallet, optional share cap from 5% to 100%, all admin-adjustable for later deposits only.
- Two-step admin transfer; pause never blocks withdrawals or settling a pending draw.
- Hardening: shard PDAs pinned in deposit, withdraw and fulfill_draw; `shard_count` bounded at draw time; checked shard occupancy.

Security: all findings in [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md) (2 critical, 9 high, 5 medium, 7 low) are fixed; I-1 to I-6 are documented accepted risks. No independent audit yet.

Known limitation: the buyback is not run on mainnet until a Jupiter route is tested (I-5). Fees stay in each jar's fee vault.

Reproducible build: `solana-verify build --library-name stonksauce --arch v3 --base-image solanafoundation/solana-verifiable-build:4.1.0`, executable hash `727f6400e139ff57e69d4baeba773b0759e21df0972829b8e239dd44841d6433`.
