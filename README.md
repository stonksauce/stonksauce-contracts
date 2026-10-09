# StonkSauce contracts

On-chain program for [StonkSauce](https://stonksauce.xyz), a no-loss daily prize jar for StonkFun reward tokens on Solana.

StonkFun reward tokens are Token-2022 tokens with a transfer fee that is paid out to holders in a second token. Holders deposit into a jar, the jar collects the rewards as one large holder, and once per round one depositor wins the pot, weighted by deposit size and time held. Deposits are never part of the prize and can be withdrawn at any time outside the few seconds a draw is settling.

| | |
| --- | --- |
| Program id | `FhGABn7RcdhGc4CrNx4ohDdAsFUFiHz48y4QDMfxe2UF` |
| Framework | Anchor 0.31.1, SBPF v3 |
| Randomness | [ORAO VRF](https://github.com/orao-network/solana-vrf) (Switchboard On-Demand until v1.1) |
| License | [BUSL-1.1](LICENSE), converting to GPL-2.0-or-later on 2030-09-28 |

## Layout

| Path | Contents |
| --- | --- |
| `programs/stonksauce` | The production program. |
| `programs/mock_orao` | Test stand-in for ORAO VRF, loaded at the ORAO program id on localnet. Its authority can write any value, so it is never deployed. |
| `programs/mock_amm` | Constant-product pool used as the buyback swap program in tests and on devnet. Never deployed to mainnet. |
| `docs/SECURITY_REVIEW.md` | Internal security review: every finding, its fix and its regression check. |

## How it works

- **Jars.** One jar per stake mint. The jar PDA owns the stake vault (all deposits), the reward vault (the pot, paid by StonkFun), a fee vault and a SAUCE vault.
- **Positions.** Balances and draw weights live in zero-copy shards of up to 511 depositors, each a Fenwick tree, indexed by a per-jar directory. A jar holds up to 196,224 depositors, and a weighted pick is O(log n).
- **Odds.** A deposit's weight is its size times the share of the round it was held, so depositing just before a draw buys almost no odds.
- **Draws.** When a round ends and the pot is non-empty, the keeper calls `request_draw`, which asks ORAO VRF for randomness on a seed derived on-chain from the jar, round and a nonce. Once a quorum of ORAO's fulfilment authorities has written the value, anyone can call `fulfill_draw`, which picks the winner and pays 95% of the pot. Nobody, including the keeper, can see the value before it is on-chain. A draw ORAO has not answered within an hour can be cancelled by anyone; an answered draw can only be settled.
- **Fees.** 5% of every pot goes to the fee vault. `buyback_and_burn` swaps it for SAUCE through the whitelisted swap program and burns the output, with post-conditions on the swap result.
- **Limits.** Each jar has a minimum deposit, an absolute maximum per wallet, and an optional share cap (5% to 100% of the jar).
- **Admin.** Two-step admin transfer. The admin sets the keeper, swap program, round length, creation fee and per-jar limits. Pausing blocks new jars, deposits and new draw requests; withdrawals and settling or cancelling a pending draw always work.

## Build

```bash
cargo-build-sbf --manifest-path programs/stonksauce/Cargo.toml --arch v3
```

ORAO VRF has the same program id on devnet and mainnet, so every cluster runs the same binary; there are no cluster features.

## Tests

```bash
cargo test -p stonksauce --lib
```

Unit tests cover the Fenwick tree, randomness parsing, cap arithmetic and shard pinning. The end-to-end regression suite that replays each security finding against a local validator lives in the private integration repository; its results are quoted in `docs/SECURITY_REVIEW.md`.

## Verify the deployed program

Mainnet binaries are built reproducibly with [solana-verify](https://github.com/Ellipsis-Labs/solana-verifiable-build):

```bash
cargo install solana-verify --version 0.5.2 --locked
solana-verify build --library-name stonksauce --arch v3 \
  --base-image solanafoundation/solana-verifiable-build:4.1.0
solana-verify get-executable-hash target/deploy/stonksauce.so
solana-verify get-program-hash -u https://api.mainnet-beta.solana.com FhGABn7RcdhGc4CrNx4ohDdAsFUFiHz48y4QDMfxe2UF
```

The two hashes match for the commit tagged with the deployed release. Or in one step:

```bash
solana-verify verify-from-repo https://github.com/stonksauce/stonksauce-contracts \
  --program-id FhGABn7RcdhGc4CrNx4ohDdAsFUFiHz48y4QDMfxe2UF --library-name stonksauce \
  --arch v3 --base-image solanafoundation/solana-verifiable-build:4.1.0 --commit-hash <tag commit>
```

## Security

Please report vulnerabilities privately through GitHub's "Report a vulnerability" on this repository, not in public issues.
