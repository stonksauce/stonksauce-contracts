# Changelog

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
