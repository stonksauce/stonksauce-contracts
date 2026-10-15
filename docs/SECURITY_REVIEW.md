# StonkSauce program security review

Scope: `programs/stonksauce` (Anchor 0.31, Token-2022 aware), the keeper, the shared TS client, and the localnet mocks (`mock_amm`, `mock_switchboard`). Baseline: commit `c9ce389` ("tba"). The program has not been deployed to devnet or mainnet, so every fix ships as a fresh deployment; no account migration is needed.

Every finding marked Fixed has a regression check in `scripts/localnet/e2e.ts`. Each check replays the attack against the fixed program and asserts the specific error code. The e2e output line is quoted under each finding.

2026-10 update: randomness moved from Switchboard On-Demand to ORAO VRF, and `mock_switchboard` was replaced by `mock_orao`. C-2 and H-3 below describe the Switchboard integration as it was reviewed and are kept as history. What replaced each of their checks, and the current regression lines, are in "2026-10: switch to ORAO VRF".

## Summary

| ID | Severity | Title | Status |
| --- | --- | --- | --- |
| C-1 | Critical | Buyback CPI to an arbitrary program, signed by the jar PDA | Fixed |
| C-2 | Critical | Randomness read from an unbound account with a wrong Switchboard id | Fixed |
| H-1 | High | `initialize` can be front-run | Fixed |
| H-2 | High | Permissionless buyback with caller-chosen `min_out` | Fixed |
| H-3 | High | Randomness re-roll by the committer | Fixed |
| H-4 | High | Fenwick slots never freed: dust deposits permanently DoS a jar | Fixed |
| H-5 | High | Pause blocks withdrawals | Fixed |
| H-6 | High | A stuck `DrawPending` locks all deposits forever | Fixed |
| H-7 | High | SAUCE-staked jar: buyback vault is the stake vault | Fixed |
| H-8 | High | Jar squatting with a bogus quote mint | Fixed |
| H-9 | High | Hostile Token-2022 extensions or freeze authority | Fixed |
| M-1 | Medium | `u64` to `i64` cast corrupts the Fenwick tree | Fixed |
| M-2 | Medium | Single-step admin transfer | Fixed |
| M-3 | Medium | No tree and `total_deposits` consistency check at draw time | Fixed |
| M-4 | Medium | Buyback swap signed by the jar PDA | Fixed |
| M-5 | Medium | Quote mint freeze authority not checked | Fixed |
| L-1 | Low | `fulfill_draw_local` backdoor compiled into localnet builds | Fixed (removed) |
| L-2 | Low | `mock_amm` accepts any vault accounts | Fixed |
| L-3 | Low | `fulfill_draw` does not re-assert vault authority | Fixed (hardening) |
| L-4 | Low | Token accounts not bound to the CPI token program | Fixed (hardening) |
| L-5 | Low | Shard account not pinned to its PDA | Fixed (hardening) |
| L-6 | Low | `shard_count` trusted when slicing the directory | Fixed (hardening) |
| L-7 | Low | Unchecked `occupied` increment | Fixed (hardening) |
| I-1 to I-6 | Info | Economic and operational risks | Documented |
| 2026-10 | Change | Switchboard On-Demand replaced by ORAO VRF (supersedes the C-2 and H-3 checks) | Done, localnet regressions pass |
| M-6 | Medium | Buyback forwards the crank's signature into the swap CPI (found in the SauceHole review, SH-1) | Open; fix in the next stonksauce upgrade |
| SH | Review | `programs/sauce_hole` (the SauceHole), see "2026-10: SauceHole program" | SH-1 fixed; localnet and devnet regressions pass |

## Critical

### C-1 Buyback CPI to an arbitrary program, signed by the jar PDA

Original `buyback_and_burn` took `swap_program` as an unchecked `AccountInfo` and called it with `invoke_signed` using the jar PDA seeds. It marked the jar as a signer in the forwarded account list, and anyone could call it.

Attack: deploy a program with a `swap` entry point, pass it as `swap_program`, and include the stake vault, reward vault and fee vault in the remaining accounts. Signer privileges propagate through the attacker's program, which can then CPI into the token program as the jar. One transaction drains every vault of every jar.

Fix:
- `swap_program` must equal `config.swap_program` (admin-set: Jupiter v6 on mainnet, `mock_amm` elsewhere) and be executable.
- The jar, its stake vault and its reward vault may not appear in the swap accounts (`ForbiddenSwapAccount`). A CPI can only touch accounts it is handed. Program-owned state (jar, Fenwick, positions) cannot be written by another program anyway.
- The swap is no longer signed by the jar at all (M-4).
- After the CPI, the fee vault and SAUCE vault are reloaded and must still be owned by the jar, with no delegate and no close authority. The fee vault may not lose more than `amount_in`, and the SAUCE vault must gain at least `min_out` (`BuybackViolation`, `BuybackEmpty`).
- `buyback_and_burn` is keeper-only (see H-2).

Regression:

```
rejected as expected: buyback CPI into Token-2022 to drain the stake vault (original critical bug) (6029)
rejected as expected: buyback swap accounts that include the stake vault (6030)
rejected as expected: buyback swap accounts that include the reward vault (6030)
```

See M-4 for the checks that run with a hostile program whitelisted.

### C-2 Randomness read from an unbound account with a wrong Switchboard id

Original `fulfill_draw` accepted any account as the first remaining account if it was owned by `SWITCHBOARD_ON_DEMAND`. It then used the last 16 bytes of that account's data as the random number. There were three independent problems:

1. The constant `SBondMDrcV3K4kx4pzdDE7sVuJcQnqLyTmB6YqusmFD` is not the Switchboard On-Demand program. The real id is `SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv`. On mainnet every `fulfill_draw` would fail. `request_draw` was permissionless and blocks deposits and withdrawals while pending, so the first draw would lock every deposit permanently.
2. With the correct id, the last 16 bytes of a Switchboard randomness account are not the random value. They are fixed layout data, such as signer and expiry fields, which are public before the draw.
3. Nothing bound the draw to a commitment made before the outcome was known. The fulfiller could choose any Switchboard-owned account, including one whose revealed value was already public, and so choose the winner.

Fix (`randomness.rs`, `request_draw`, `fulfill_draw`):
- The Switchboard program id and default queue are pinned constants. A `devnet` cargo feature switches them to the devnet deployment.
- The randomness account is parsed at the documented offsets (discriminator, queue, `seed_slot`, `reveal_slot`, `value`), and its queue must be the pinned queue.
- `request_draw` requires a commit made in the same transaction (`seed_slot == clock.slot - 1`, not yet revealed). It stores the account key and `commit_slot` on the jar.
- `fulfill_draw` requires that exact account (`address = jar.randomness_account`). The account must still carry the recorded `seed_slot` and must have been revealed in the current slot, so the reveal and the fulfill share one transaction. The winner is picked from the first 16 bytes of the revealed value.

Regression:

```
rejected as expected: request_draw with a commitment from an earlier slot (6015)
rejected as expected: request_draw with an account not owned by Switchboard (6016)
rejected as expected: request_draw with randomness from a non-default queue (6016)
rejected as expected: fulfill before the reveal (6014)
rejected as expected: fulfill reading a different randomness account (6016)
rejected as expected: fulfill paying someone other than the drawn winner (6013)
```

Localnet now loads a mock Switchboard program at the real program id (`solana-test-validator --bpf-program`) with the same account layout. Localnet therefore runs the same binary as mainnet, and the randomness checks are exercised end to end.

Superseded 2026-10: the Switchboard checks and regression lines above no longer exist in the code. See "2026-10: switch to ORAO VRF" for their replacements.

## High

### H-1 `initialize` can be front-run

Anyone could call `initialize` first and become admin, choosing the SAUCE mint and fees.

Fix: `initialize` requires the program and its ProgramData account, and the signer must be the program's upgrade authority (`NotUpgradeAuthority`). Deploy, then initialize, then move the upgrade authority to a multisig.

Regression: `scripts/localnet/bootstrap.ts` asserts a non-authority `initialize` fails with 6022 before the real one runs.

### H-2 Permissionless buyback with caller-chosen `min_out`

Anyone could trigger a buyback of the whole fee vault with `min_out = 1` and sandwich it.

Fix: `buyback_and_burn` requires `config.keeper` or `config.admin` (`NotKeeper`). The keeper must still set a sensible `min_out` from a quote (see I-5).

Regression: `rejected as expected: buyback by a non-keeper (could sandwich with min_out = 1) (6023)`

### H-3 Randomness re-roll by the committer

With Switchboard On-Demand the randomness authority signs the reveal. If anyone could commit, they could withhold unfavourable reveals, or commit again, until they liked the result.

Fix: `request_draw` is keeper-only. `fulfill_draw` rejects a randomness account whose `seed_slot` no longer matches the recorded commit, so a second commit invalidates the draw instead of re-rolling it. The keeper can still stall a draw, but only until the one-hour `cancel_draw` timeout (H-6), and cannot choose the winner.

Regression:

```
rejected as expected: request_draw by a non-keeper (could withhold unfavourable reveals) (6023)
rejected as expected: fulfill after the randomness account was re-committed (6015)
```

Superseded 2026-10: under ORAO VRF the requester has no reveal to withhold, and a second request while a draw is pending fails with `BadStatus`. See "2026-10: switch to ORAO VRF".

### H-4 Fenwick slots never freed: dust deposits permanently DoS a jar

The tree has 191 usable slots. Slots were never returned on full withdrawal, and there was no minimum deposit. 191 one-unit deposits from throwaway wallets closed a jar to new depositors forever.

Fix:
- Per-jar `min_deposit`, set at creation and adjustable by the admin. A position must be 0 or at least `min_deposit` after every deposit and withdraw (`BelowMinDeposit`).
- A full withdraw returns the slot, and new positions reuse freed slots first.
- Later, sharding raised capacity from 191 slots to 384 shards of 511 (196,224). Freed slots go on a per-shard stack, and anyone can open the next shard when every open one is full.

Regression:

```
rejected as expected: position below min_deposit (6027)
rejected as expected: partial withdraw leaving dust below min_deposit (6027)
withdraw works while paused; C's slot 3 freed
rejected as expected: deposit into a full shard (6012)                      (E2E_STRESS=1)
shard 0 filled with 511; the next depositor opened shard 1                  (E2E_STRESS=1)
slot 54 freed and reused before shard 1 grew                                (E2E_STRESS=1)
```

Squatting every slot now costs 196,224 times `min_deposit` plus the rent of 383 shards, and the squatter's capital stays at risk of losing draws.

### H-5 Pause blocks withdrawals

`withdraw` checked `config.paused`, so pausing trapped user funds.

Fix: pause stops deposits, draws and buybacks. Withdrawals always work while the jar is `Open`.

Regression: `rejected as expected: deposit while paused (6001)`, followed by a successful withdraw while paused.

### H-6 A stuck `DrawPending` locks all deposits forever

If the randomness never arrived (oracle outage, keeper loss, the C-2 id bug), the jar stayed in `DrawPending`. Deposits and withdrawals are blocked in that state.

Fix: `cancel_draw` is permissionless once `DRAW_TIMEOUT_SECS` (3600) have passed since `request_draw`. It returns the jar to `Open` without paying out and emits `DrawCancelled`. The keeper calls it automatically.

Regression: `rejected as expected: cancel_draw before the timeout (6028)`. The success path needs an hour of wall-clock time; see TESTING.md, "Manual checks".

### H-7 SAUCE-staked jar: buyback vault is the stake vault

The buyback output account was the jar's associated token account for the SAUCE mint. For a jar that stakes SAUCE (the first planned mainnet jar), that is the stake vault holding depositors' funds. The stake vault was therefore handed to the swap program as a jar-signed writable account.

Fix: buyback output goes to a dedicated PDA token account (`seeds = ["sauce_vault", stake_mint]`, authority jar). The stake vault is in the forbidden list for swap accounts.

Regression: `stake vault untouched by buyback` assertion after the successful buyback.

### H-8 Jar squatting with a bogus quote mint

Jars are one per stake mint (PDA of the stake mint). Anyone could create the jar for a popular reward token with a worthless or wrong quote mint, blocking the real jar forever.

Fix: jar creation is admin-only until the admin calls `set_open_jar_creation(true)` (`JarCreationClosed`). Creation also burns `creation_fee` SAUCE.

Regression: `rejected as expected: non-admin create_jar while creation is closed (mint squatting) (6025)`

### H-9 Hostile Token-2022 extensions or freeze authority

A stake or quote mint with a freeze authority, permanent delegate, transfer hook, non-transferable flag, pausable extension or default-frozen state lets the mint owner freeze, seize or block the vaults.

Fix (`token_utils::require_safe_mint`, `create_jar`):
- Stake and quote mints are rejected if they carry any of those Token-2022 extensions (`UnsupportedMint`).
- Stake and quote mints must have no freeze authority, on either token program. The original fix only checked the stake mint (M-5).
- The stake mint must be Token-2022 with a transfer-fee extension, as before.

Regression:

```
rejected as expected: stake mint with a freeze authority (6026)
rejected as expected: stake mint with a permanent delegate (6026)
rejected as expected: classic quote mint with a freeze authority (could freeze the reward and fee vaults) (6026)
```

The mainnet SAUCE mint (`1JZKShMYfMCH4SqUwHZNxUhQxbxbrZPwQB9HDy1Eua7`) was checked. It has no freeze or mint authority, and its only extensions are MetadataPointer, TokenMetadata and TransferFeeConfig (300 bps), so it passes. STONK is a classic SPL mint with no freeze authority.

## Medium

### M-1 `u64` to `i64` cast corrupts the Fenwick tree

The tree update took a signed delta cast from `u64`. Amounts above `i64::MAX` wrapped and corrupted sums. Fixed with separate `add` and `sub` on `u64` with checked arithmetic, covered by the unit test `fenwick::tests::amounts_above_i64_max_stay_consistent`.

### M-2 Single-step admin transfer

`set_admin` moved control in one call, so a typo lost the protocol. Replaced by `propose_admin` and `accept_admin` (`NotPendingAdmin`).

Regression: `rejected as expected: accept_admin by someone else (6024)`, then a full handover and hand-back.

### M-3 No tree and `total_deposits` consistency check

`fulfill_draw` trusted `total_deposits` when mapping the random ticket into the tree. It now requires the tree's root prefix sum to equal `total_deposits` (`TreeMismatch`), so accounting drift halts the draw instead of paying the wrong person.

### M-4 Buyback swap signed by the jar PDA

Source: Almanax scan, finding `972a5be6`. After C-1, `buyback_and_burn` still called the whitelisted swap program with the jar PDA's signature. The jar is the authority of every vault of that jar, and Jupiter v6 is upgradeable, so a compromised or malicious swap program held a master key for the duration of the CPI. The deny-list and post-checks bounded the damage, but did not close it completely:
- The swap could move up to `amount_in` out of the fee vault to itself and satisfy `min_out` with SAUCE bought elsewhere. This is the same exposure any swap venue has, and `min_out` bounds it.
- The post-checks did not catch other state the owner can change. For a Token-2022 quote mint, the swap could reallocate the fee vault and turn on required memos for incoming transfers. The fee transfer in `fulfill_draw` has no memo, so every draw of that jar would then fail.

Fix: the jar never signs for an external program. `buyback_and_burn` has the jar approve a per-jar swap authority PDA (`["swap_authority", stake_mint]`) as delegate on the fee vault for exactly `amount_in`. The swap is signed by that PDA only, and the jar revokes the delegation after the CPI. A delegate can only transfer or burn up to the approved amount. It cannot change the owner, delegate, close authority or extensions, and it has no authority over any other account. The jar itself is now on the forbidden list for swap accounts, and the post-checks still run.

Residual risk: a hostile swap program can still take up to `amount_in` of protocol fees in one buyback, paying `min_out` in SAUCE. That is inherent to trading through an external venue. Deposits and pots are not exposed.

Regression (the last four run with the SPL Token program whitelisted as a hostile "swap program"):

```
rejected as expected: buyback swap accounts that include the jar (would hand the swap its signature) (6030)
rejected as expected: swap that tries to approve its own delegate on the fee vault (4)
rejected as expected: swap that tries to take ownership of the fee vault (4)
rejected as expected: swap that spends more than amount_in (1)
rejected as expected: swap that pockets amount_in and buys no SAUCE (6021)
```

Codes 4 and 1 are the SPL Token program's `OwnerMismatch` and `InsufficientFunds`: the delegate cannot act as owner or exceed its allowance. After the successful buyback, e2e also asserts the fee vault has no delegate and is still owned by the jar.

### M-5 Quote mint freeze authority not checked

Source: Almanax scan, finding `d4eb1c8d`. H-9 intended to reject freeze authorities, but only the stake mint was checked. The quote mint went through `require_safe_mint`, which returned early for classic SPL mints. A quote mint whose freeze authority is held by a third party could freeze the reward vault or the fee vault. Draws would then fail, and every draw attempt would lock deposits for up to the one-hour timeout.

Fix: `require_safe_mint` rejects any mint with a freeze authority, on either token program, before the Token-2022 extension checks. `create_jar` calls it for both mints. This rules out quote tokens that keep a freeze authority, such as USDC. STONK and SAUCE have none.

Regression: `rejected as expected: classic quote mint with a freeze authority (could freeze the reward and fee vaults) (6026)`

## Low

### L-1 `fulfill_draw_local` backdoor

A `localnet` feature compiled in an instruction that settled a draw with a caller-supplied random number. It was not in mainnet builds, but a wrong build flag would have shipped it. The instruction and the feature were removed. `scripts/mainnet/deploy.sh` also refuses any binary containing the string `FulfillDrawLocal`.

### L-2 `mock_amm` accepts any vault accounts

The mock swap did not check that the passed vaults belonged to the pool. That is test-only code, but it made buyback tests meaningless. The mock now requires the pool's own vaults (`WrongVault`).

### L-3 `fulfill_draw` does not re-assert vault authority

Source: Almanax scan, finding `80309c4f`. `FulfillDraw` pinned the reward and fee vaults by address only. That is sufficient today: `create_jar` creates them with the jar as authority, and only the jar can change that. But the instruction relied on that invariant instead of stating it.

Fix: `reward_vault` and `fee_vault` in `FulfillDraw`, and `fee_vault` in `BuybackAndBurn`, now also require `token::authority = jar`, the quote mint, and the passed quote token program.

### L-4 Token accounts not bound to the CPI token program

Source: Almanax scan, finding `1263598d`. In `Deposit` and `Withdraw`, the stake mint and token accounts were not tied to the passed `stake_token_program`. This was not exploitable. Anchor's `Interface<TokenInterface>` and `InterfaceAccount` only accept the SPL Token and Token-2022 program ids, so no look-alike token program can be passed. A mismatch between the two real programs only makes the caller's own CPI fail.

Fix: `mint::token_program` and `token::token_program` constraints on the stake mint, the user's stake account and the stake vault, and on the quote accounts in `FulfillDraw`. The stake vault also requires `token::authority = jar`. A mismatch now fails with Anchor's `ConstraintTokenTokenProgram` or `ConstraintMintTokenProgram` before any CPI.

### L-5 Shard account not pinned to its PDA

Source: Almanax scan, finding `7bc7a8eb` (reported as Medium). `Deposit`, `Withdraw` and `FulfillDraw` took the shard as a plain `AccountLoader<Shard>`, then checked `shard.jar` and the position's shard index. The shard's address was never checked. This was not exploitable. Anyone can create an account owned by the program at any address, but it starts zeroed. `AccountLoader` requires the Shard discriminator, and only `create_jar` and `open_shard` write one, both at the seed-derived PDA.

Fix: `state::require_shard_address` re-derives `["shard", stake_mint, shard.index]` and requires the passed account to match (`WrongShard`). All three instructions call it, so `shard.index` is trusted for directory lookups because of the address, not only because of how shards are created. Unit test: `state::shard_tests::only_the_canonical_shard_pda_is_accepted`.

### L-6 `shard_count` trusted when slicing the directory

Source: Almanax scan, finding `b1e0aa97`. `fulfill_draw` sliced `directory.balances[..shard_count]`. `open_shard` already enforces `shard_count <= 384`, so the slice cannot go out of bounds. `fulfill_draw` now also requires `shard_count <= MAX_SHARDS` (`TooManyShards`), so a broken invariant would fail with a clear error instead of a panic. The directory is already pinned by `address = jar.directory`.

### L-7 Unchecked `occupied` increment

Source: Almanax scan, finding `a090668b`. `deposit` incremented `directory.occupied[s]` with `+= 1`. A shard hands out at most 511 slots, so a `u16` cannot overflow. The finding also assumed release builds wrap on overflow. This workspace builds with `overflow-checks = true`, so an overflow would abort the transaction instead. The increment now uses `checked_add` (`MathOverflow`), which makes the bound explicit.

## 2026-10: switch to ORAO VRF

Why: Switchboard shut down. Its foundation announced the closure on 2026-09-19 and support ended on 2026-09-25. Its Crossbar host (`crossbar.switchboard.xyz`) no longer resolves in DNS and its oracle gateways return errors, so the keeper's `commitIx` failed with "No eligible randomness oracle candidates were found". When this was found on 2026-10-08, all 10 mainnet jars were still in round 0 with status `Open`, so no mainnet draw ever ran under Switchboard and no pending draw needed migrating.

Draws now use ORAO VRF (`VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y`, the same id on devnet and mainnet; network state `5ER1oENnV4srxYdAynUfRzWeQCPQaqMiAp4VqyMbSqnK`). The ORAO crate (`orao-solana-vrf` 0.7.0) needs Anchor 0.32, so `randomness.rs` hand-rolls the `request_v2` CPI and the `RandomnessV2` parser, as it did for Switchboard. ARCHITECTURE.md section 5.7 describes the design.

What replaced each Switchboard check:

| Switchboard check (C-2, H-3) | ORAO VRF replacement |
| --- | --- |
| Pinned Switchboard program id and default queue; a `devnet` cargo feature switched them | Pinned ORAO VRF id and network state PDA, the same on every cluster. `request_draw` requires `vrf_program` (executable) and `network_state` to equal them (`BadRandomnessAccount`). The `devnet` feature was removed, so every cluster runs one binary. |
| Account owned by Switchboard, parsed at fixed offsets, queue equal to the pinned queue | `fulfill_draw` requires the account to be owned by ORAO and carry the `RandomnessV2` discriminator (`BadRandomnessAccount`), and parses it at fixed offsets (state at 8, seed at 41, value at 73). |
| Commit in the same transaction (`seed_slot == slot - 1`, not yet revealed) | The seed is derived on-chain: `draw_seed = sha256("stonksauce-draw" \|\| jar \|\| round_index \|\| nonce)`. The request account must be ORAO's PDA for that seed (`BadRandomnessAccount`) and is created by the CPI. ORAO creates each seed's account only once, so a seed whose value is already known can never be reused. |
| `address = jar.randomness_account` in `fulfill_draw` | Unchanged; it now holds the ORAO request PDA. |
| `seed_slot == jar.commit_slot`, so a re-commit voids the draw (H-3) | The account's seed must equal `draw_seed(jar, round_index, jar.draw_nonce)` (`RandomnessStale`). A second `request_draw` while one is pending fails with `BadStatus`. |
| Revealed in the current slot, value non-zero (`RandomnessUnready`) | Fulfilled (state 1) and value not all zero (`RandomnessUnready`). The value is final once ORAO writes it, so no same-slot rule is needed. |
| `request_draw` keeper-only, because the committer controlled the reveal | `request_draw` is still keeper or admin only, because the requester pays ORAO's fee and the request rent. `fulfill_draw` is now permissionless in practice: anyone can settle once ORAO has answered. |

New risk, seed squatting: anyone can call ORAO `request_v2` for the next draw's seed first. That only makes `request_draw` fail for that nonce. The keeper uses a fresh random nonce for every request (`newDrawNonce`) and retries, and the squatter learns nothing about any seed the draw will use.

State and interface: the jar field `commit_slot` was renamed `draw_nonce` (same `u64` at the same offset, so the 512-byte `Jar` layout is unchanged and existing jars need no migration). The `DrawCancelled` field `commit_slot` was renamed `draw_nonce`. `request_draw` takes `nonce: u64`, and its crank is writable because it pays. Error codes are unchanged; their messages now read: 6014 `RandomnessUnready` "Randomness is not ready", 6015 `RandomnessStale` "Randomness request is for a different draw", 6016 `BadRandomnessAccount` "Not the ORAO VRF request for this draw".

`cancel_draw` now takes the jar's request account (`address = jar.randomness_account`, `BadRandomnessAccount`) and refuses with the new `DrawFulfilled` (6038) when the request is one `fulfill_draw` would accept. Without this, a fulfilled draw that nobody settled for an hour could be cancelled and requested again, which is a re-roll. The check runs before the timeout check, and a pre-switch Switchboard account never counts as fulfilled, so an ORAO outage still unlocks after an hour. The keeper now tries to settle before it considers cancelling. Deployed to mainnet on 2026-10-08, executable hash `16ef4ac1a68a66b99c4453389d68e84f8205e73dbb5ba5d07d2a8bd77dfdd23e`.

Localnet: `mock_orao` replaces `mock_switchboard` at the real ORAO id. Its `mock_fulfill` lets the network authority (the localnet admin and keeper) write any value, which is only acceptable locally.

Regression (all passing on localnet on 2026-10-08):

```
rejected as expected: request_draw by a non-keeper (6023)
rejected as expected: request_draw naming a request that is not this draw's seed (6016)
rejected as expected: request_draw through a program other than ORAO VRF (6016)
rejected as expected: request_draw with a network state other than ORAO's (6016)
rejected as expected: request_draw on a seed someone already requested (0)
draw requested under nonce 107n after the squatted one failed
rejected as expected: second request_draw while one is pending (re-roll) (6019)
rejected as expected: cancel_draw naming a request that is not the jar's (6016)
rejected as expected: fulfill before ORAO answers (6014)
rejected as expected: fulfill reading a fulfilled request for another seed (6016)
rejected as expected: cancel_draw once ORAO has answered (6038)
rejected as expected: fulfill while ORAO has not answered (6014)
```

Code 0 is the System Program's `AccountAlreadyInUse`, raised when `request_v2` tries to create the squatted request account again; the check accepts any error. The `WinnerMismatch` and `WrongShard` fulfill guards are unchanged. The "settle (permissionless once ORAO answers)" section has a non-keeper user settle the draw. The Switchboard-only regressions (commit from an earlier slot, non-default queue, re-commit after request) were removed.

Unit tests in `randomness.rs` replace the Switchboard ones: `discriminators_and_network_state_match_orao`, `reads_the_first_16_bytes_of_a_fulfilled_request`, `pending_request_is_not_ready`, `other_seed_or_account_type_is_rejected`, `only_an_answered_orao_request_counts_as_fulfilled` and `seeds_differ_per_jar_round_and_nonce`. `cargo test -p stonksauce --lib` passes 28 tests.

Trust model: under Switchboard the keeper could simulate the reveal, learn the winner first and withhold an unfavourable result until the one-hour cancel (old I-6). Under ORAO the keeper cannot see the value before it is on-chain, once it is on-chain anyone can settle, and `cancel_draw` refuses a fulfilled draw, so withholding no longer works. The keeper can still delay requesting a draw. The randomness now relies on ORAO's quorum of fulfilment authorities; a colluding quorum could bias results, similar to trusting Switchboard's oracle TEEs before. The one-hour `cancel_draw` still protects against an ORAO outage (H-6).

## Informational and accepted risks

- I-1 Just-in-time deposits. A depositor could deposit just before `next_draw_ts` and withdraw right after settlement, winning with probability proportional to stake for the time the stake is exposed. Mitigated by time-weighting: `deposit` adds only `amount * time_left / round_length` to the draw tree, `withdraw` keeps the same fraction of draw weight as of balance, and `fulfill_draw` rebuilds the tree from full balances for the next round. A deposit after the countdown ends counts nothing toward the pending draw. Full odds now require holding for the whole round.
- I-2 Sybil wallets bypass the per-wallet caps. Both `cap_bps` (share of the jar) and `max_deposit` (absolute amount) limit one position, not one person. A cap does not change anyone's expected value, since odds stay proportional to stake; it limits how concentrated a single position looks and how much one wallet can take in a draw. The admin can change either limit with `set_jar_cap` and `set_jar_max_deposit`. New limits apply to later deposits only, so lowering one never locks or seizes an existing position.
- I-3 StonkFun controls the stake token's transfer fee. The SAUCE transfer-fee config authority (`WLHv2UAZm6z4KyaaELi5pjdbJh6RESMva1Rnn8pJVVh`) can raise the fee up to the Token-2022 maximum. Deposits are credited net of the actual fee received, so accounting stays correct, but withdrawals would pay the higher fee.
- I-4 Position rent is not reclaimed. A fully withdrawn position account stays open. The rent is roughly 0.0015 SOL per user, per jar.
- I-5 Mainnet buyback is not yet exercised. The program side of Jupiter routing is ready: pass-through `swap_data`, the whitelisted program, and post-checks. The keeper only implements the `mock_amm` route. The swap authority PDA cannot pay rent for intermediate accounts, so a Jupiter route must be built with `useSharedAccounts` and a pre-created SAUCE vault. The swap authority must be the route's `userTransferAuthority` and acts as a delegate on the fee vault rather than its owner (M-4), so confirm on a mainnet fork that the chosen route accepts a delegated source account (the SauceHole fork rehearsal showed Jupiter `shared_accounts_route` accepts one, SH-I11). It also needs a quote-based `min_out`. Until then, keep fees in the fee vault; they are safe there.
- I-6 Keeper liveness and randomness trust. Updated 2026-10 for ORAO VRF. The keeper can delay requesting a draw but cannot pick the winner or see the value before ORAO writes it on-chain. Once it is on-chain anyone can call `fulfill_draw`, and `cancel_draw` refuses a fulfilled draw (`DrawFulfilled`), so an unfavourable result cannot be withheld or re-rolled. Randomness relies on ORAO's quorum of fulfilment authorities, and a colluding quorum could bias results. If the keeper key is lost, rotate it with `set_keeper`; a draw the old key requested still settles once ORAO answers. Under Switchboard (before 2026-10) the keeper could simulate the reveal and withhold until the one-hour cancel, and a pending draw committed by an old key had to time out.

## Operational requirements that the fixes depend on

1. Run `initialize` from the upgrade authority before transferring it to a multisig.
2. Set `config.admin` to the multisig with `propose_admin` and `accept_admin`.
3. Use a dedicated hot keeper key. It can only request draws, cancel stale draws and buy back through the whitelisted program. Keep it funded: each `request_draw` pays ORAO's fee and the request rent.
4. Leave `open_jar_creation` off unless jar squatting is acceptable.
5. Build every cluster without cargo features. The `devnet` feature was removed in 2026-10; localnet, devnet and mainnet run one identical binary.

## 2026-10: SauceHole program

Scope: `programs/sauce_hole` (program id `Ho1ezeQ2fb1zEL3aK94e4q5j7g3vwGfi5PguPWGjB2VC`), its client `scripts/lib/hole.ts`, the keeper step `keeper/src/hole.ts` and the Sauce Meter rules in `shared/getSauced.ts`. Not yet on mainnet. Reviewed binary: sha256 `bb7bba6ec945431c4f82ed1faba2b40105bfb5ef37ea6c7763e58f58e5217189` (the `cargo-build-sbf` build; the mainnet `solana-verify` build has its own hash).

The promise under review: STONKSAUCE that enters the hole never leaves, and STONK rewards sent to the hole can only be spent buying more STONKSAUCE into it.

Design: one singleton `Hole` PDA (seed `hole`, `4Z3tMJTnRuQw7XSqLA1JrcHPAj3mKfL2rLAGcCJrGM7h`) owns two ATAs, the STONKSAUCE vault and the STONK reward vault. `feed` moves the feeder's STONKSAUCE into the vault and credits what actually arrived, net of the 3% transfer tax. `get_sauced` (keeper or admin only) approves the swap authority PDA (seed `swap_authority`) as delegate on the reward vault for exactly `amount_in`, invokes the configured swap program with that PDA as the only signer, revokes, and then checks balances and authorities on both vaults. No instruction transfers, burns, approves, freezes or closes the STONKSAUCE vault, and the hole PDA never signs for the swap.

### SH-1 (Medium) Crank signature forwarded into the swap CPI. Fixed.

`get_sauced` built the swap's account metas with `is_signer: key == swap_authority || account.is_signer`, so any outer signer passed in the swap accounts, including the keeper's own wallet, kept its signature inside the swap program. A hostile or compromised swap program could pair a real buy (passing every balance check) with a transfer out of the keeper's wallet. Fix: only the swap authority is ever marked as a signer. The mock AMM and Jupiter shared-accounts routes need no other signer.

The live stonksauce `buyback_and_burn` (`buyback.rs`, line 46) has the same pattern (M-6). It is exploitable only through a swap program the admin whitelisted, and only against the crank's wallet, never the vaults. Apply the same one-line fix in the next stonksauce upgrade.

Regression: the test sets the System program as the swap program and passes a transfer from the crank. On the pre-fix binary the drain executed and the transaction only reverted at settlement (`SwapViolation`). The test now requires the failure to be the System program's missing signature, before settlement, and checks the crank's SOL is unchanged.

### Attacks replayed against the program

| Attack | Protection | Regression line |
| --- | --- | --- |
| Front-run `initialize` | Signer must be the program's upgrade authority | `hole initialize front-run by a non upgrade authority (6003)` |
| Grief `initialize` by pre-creating the hole's ATAs | Vaults use `init_if_needed`, and Anchor checks an existing ATA's mint, owner and token program | not replayed |
| STONKSAUCE mint with a permanent delegate, transfer hook, pausable, non-transferable or default-frozen state | `require_safe_mint` rejects these extensions | `hole over a STONKSAUCE mint with a permanent delegate (6007)` |
| Reward mint with a freeze authority | `require_safe_mint` | `hole over a reward mint with a freeze authority (6007)` |
| STONKSAUCE as its own reward mint | `SameMint` | `hole with STONKSAUCE as its own reward mint (6008)` |
| Second `initialize` | Hole PDA already exists | `second initialize of the singleton hole (0)` |
| Feed from another wallet's account, into a foreign vault, or of another mint | Anchor constraints on authority, vault address and mint | `(2015)`, `(2012)`, `(2012)` |
| Dust feeds to pad the Sauce Meter | `min_feed` (admin-set, default 1 STONKSAUCE) | `feed below min_feed (6006)` |
| Permissionless GetSauced with `min_out = 1` (sandwich) | Keeper or admin only | `get_sauced by a non-keeper (6004)` |
| Unconfigured swap program | `address = hole.swap_program` | `(6010)` |
| Hole PDA passed into the swap (would hand it the hole's signature) | `ForbiddenSwapAccount` | `(6011)` |
| Swap that approves its own delegate or takes ownership of the reward vault | The swap authority is only a delegate, so the token program refuses | `(4)`, `(4)` |
| Swap that spends more than `amount_in` | Delegation is capped at `amount_in` | `(1)` |
| Swap that pockets `amount_in` and buys nothing, or below `min_out` | `settle`: bought must be above 0 and at least `min_out` | `(6013)` |
| Swap that moves or closes the STONKSAUCE vault with the swap authority | The swap authority has no authority over it | `(4)`, `(11)` |
| Swap that spends the crank's wallet | SH-1 | missing required signature |
| Direct transfer out of, or close of, the STONKSAUCE vault | Only the hole owns it, and it never signs for it | `(4)`, `(11)` |
| Admin actions by others; hijacked handover | `has_one = admin`; two-step `propose_admin` and `accept_admin` | `(6001)`, `(6002)` |

After every step the suite asserts the STONKSAUCE vault never shrank and equals `total_fed + total_bought` plus anything sent to it directly. After a GetSauced it also checks the reward vault has no delegate, is still owned by the hole, and that the `GotSauced` event matches the balance changes.

Results: `cargo test -p sauce_hole --lib` passes 14 tests (`settle`, account sizes, mint extension rules). `pnpm localnet:hole-e2e` passes with a 3% transfer tax on the localnet STONKSAUCE, like mainnet. On 2026-10-14 the program was deployed to devnet and `pnpm devnet:hole-e2e` passed against it. The testnet keeper worker then fired GetSauced unattended on a full Sauce Meter (`get_sauced reason=meter`, 35,179 compute units) and logged `meter 10.0/100, floor not met` on the next tick. The devnet STONKSAUCE stand-in has no transfer tax, so the tax path is proven on localnet only.

### Trust model and accepted risks

- SH-I1 Upgrade authority. Whoever holds the upgrade authority can replace the program and empty both vaults. "Locked forever" is only as strong as that key. Move it to the Squads vault right after `initialize`. Once the program has run on mainnet for a while, consider making it immutable (`solana program set-upgrade-authority --final`), which makes the lock unconditional.
- SH-I2 Admin and keeper control the STONK rewards, never the STONKSAUCE. The admin picks the swap program and the keeper picks `min_out`. Together they could whitelist a swap that keeps `amount_in` and pays 1 raw STONKSAUCE into the vault, which passes `settle` with `min_out = 1`. Repeating that diverts the rewards. Neither role can reduce the STONKSAUCE vault. Mitigations: admin on the multisig; whitelist only Jupiter v6; announce any `set_swap_program`. A timelock on `set_swap_program` is an option before mainnet.
- SH-I3 Price manipulation around the keeper's read. On localnet and devnet the keeper derives `min_out` from the pool reserves at tick time, minus `HOLE_SLIPPAGE_BPS` (default 1%). Someone who moves the pool just before that read gets a worse `min_out` accepted. On mainnet the keeper takes `min_out` from Jupiter's `otherAmountThreshold` and refuses a buy in three cases:
  - Price impact: the buy is halved (up to 7 times) until its rate is within `HOLE_MAX_IMPACT_BPS` (default 1%) of a quote 1/100 of its size. Jupiter's own `priceImpactPct` was found unreliable for this pool.
  - Deviation: the quote, with the 3% tax added back, is more than `HOLE_MAX_DEVIATION_BPS` (default 15%) below Jupiter's Price API. The two sources differed by 2.5% to 3.2% on 2026-10-15, so the default leaves room for a thin pool while catching a pool pushed well off price.
  - Missing price: the Price API has no price for either mint.
  The deviation check is not independent of Jupiter, and the STONK/STONKSAUCE pool held about $7k on 2026-10-15, so `HOLE_CHUNK` should stay small. Transactions are not sent through Jito yet.
- SH-I4 `participants` is reported by the keeper. It only feeds the Sauce Meter's "new participants" score and the `GotSauced` event. No funds depend on it.
- SH-I5 The Sauce Meter and the 48-hour backstop are enforced by the keeper, not the program. An operator can GetSauced at any time. This is by design: the program guarantees where value goes, not when.
- SH-I6 Pause blocks both `feed` and `get_sauced` (the plan had feeds staying open). During an incident the hole stops accepting new tokens. Pausing never moves tokens already locked.
- SH-I7 StonkFun controls the STONKSAUCE transfer fee. A higher fee lowers what a feed credits and what a buy delivers. Feeds are credited net, and the keeper's `min_out` subtracts the current epoch fee, so accounting stays exact.
- SH-I8 A feeder whose STONKSAUCE account has CPI Guard enabled cannot feed, because `feed` moves tokens by CPI with the owner's signature. The wallet must disable CPI Guard first.
- SH-I9 STONKSAUCE sent straight to the vault, without `feed`, is locked forever but not credited to any feeder.
- SH-I10 `Hole` has no spare bytes (342). A field added later needs a `realloc` in that upgrade.
- SH-I11 Mainnet GetSauced buys through Jupiter v6 `shared_accounts_route` (`keeper/src/jupiter.ts`). The keeper asks Jupiter for a route with the swap authority as `userPublicKey` and the hole's STONKSAUCE vault as destination. `patchSharedRoute` then enforces the route's shape and points its source at the reward vault, where the swap authority is a delegate:
  - The instruction must be Jupiter v6 `shared_accounts_route`.
  - Authority, source, destination and both mints must sit at the expected positions.
  - No other account may sign.
  - The swap authority's own token account may appear only in the source slot.
  - Every outer signer flag is cleared, since only the hole signs, inside its CPI.
  `tests/jupiter-route.test.ts` covers each rejection. `pnpm mainnet:hole:fork` rehearsed it on a mainnet fork on 2026-10-15:
  - Jupiter accepts the delegated source.
  - The hole received exactly the quoted `outAmount`, so Jupiter quotes STONKSAUCE net of the 3% tax and `otherAmountThreshold` is a correct `min_out`.
  - A 100 STONK buy used 64,144 compute units with one address lookup table.
  - The keeper's own `maybeGetSauced` landed a second buy through a different Jupiter program authority.
  The route trusts Jupiter v6 itself, which is upgradeable (SH-I2).
- SH-I12 Automated scan, 2026-10-15. Seven findings, triaged:

  | Finding | Verdict | Action |
  | --- | --- | --- |
  | Keeper `allJarMints` trusts spoofed program accounts (Medium) | Not exploitable: anyone can create an account owned by the program, but its data starts zeroed and only the program can write it, so the jar discriminator cannot be forged | Added a `jarPda(stake_mint)` check anyway |
  | `withdraw` token program not pinned (Medium) | False positive: `Interface<TokenInterface>` checks the id against SPL Token and Token-2022 before the handler runs, and the jar PDA fixes the mint and vault | None |
  | Keeper logs the RPC URL with its API key (Low) | Valid; `hole/e2e.ts` did the same | Logs show only the host (`rpcHost` in `scripts/lib/node.ts`) |
  | `jarActivity` downloads every jar on each tick (Low) | Valid as a cost issue; jar creation is admin-only on mainnet (`open_jar_creation` false, checked on-chain) | Fetches a 144-byte slice instead of 512 bytes; totals match the full decode on devnet |
  | Unbounded `HOLE_SLIPPAGE_BPS` collapses `min_out` to 1 (Low) | Valid | Must be an integer from 0 to 500; the Node keeper refuses to start, the Worker turns off only the hole step; a quote that rounds to zero no longer swaps |
  | Permissionless jar creation allows squatting (Low) | Known (H-8); `open_jar_creation` stays off | None |
  | `withdraw` uses `saturating_sub` on totals (Low) | Deliberate: a withdrawal is bounded by `position.amount` (checked) and the vault balance (token program), and reverting on an inconsistent total would lock every other depositor; draws already refuse an inconsistent tree (M-3) | None |