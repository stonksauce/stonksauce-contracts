# StonkSauce program security review

Scope: `programs/stonksauce` (Anchor 0.31, Token-2022 aware), the keeper, the shared TS client, and the localnet mocks (`mock_amm`, `mock_switchboard`). Baseline: commit `c9ce389` ("tba"). The program has not been deployed to devnet or mainnet, so every fix ships as a fresh deployment; no account migration is needed.

Every finding marked Fixed has a regression check in `scripts/localnet/e2e.ts`. Each check replays the attack against the fixed program and asserts the specific error code. The e2e output line is quoted under each finding.

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

## Informational and accepted risks

- I-1 Just-in-time deposits. A depositor could deposit just before `next_draw_ts` and withdraw right after settlement, winning with probability proportional to stake for the time the stake is exposed. Mitigated by time-weighting: `deposit` adds only `amount * time_left / round_length` to the draw tree, `withdraw` keeps the same fraction of draw weight as of balance, and `fulfill_draw` rebuilds the tree from full balances for the next round. A deposit after the countdown ends counts nothing toward the pending draw. Full odds now require holding for the whole round.
- I-2 Sybil wallets bypass the per-wallet caps. Both `cap_bps` (share of the jar) and `max_deposit` (absolute amount) limit one position, not one person. A cap does not change anyone's expected value, since odds stay proportional to stake; it limits how concentrated a single position looks and how much one wallet can take in a draw. The admin can change either limit with `set_jar_cap` and `set_jar_max_deposit`. New limits apply to later deposits only, so lowering one never locks or seizes an existing position.
- I-3 StonkFun controls the stake token's transfer fee. The SAUCE transfer-fee config authority (`WLHv2UAZm6z4KyaaELi5pjdbJh6RESMva1Rnn8pJVVh`) can raise the fee up to the Token-2022 maximum. Deposits are credited net of the actual fee received, so accounting stays correct, but withdrawals would pay the higher fee.
- I-4 Position rent is not reclaimed. A fully withdrawn position account stays open. The rent is roughly 0.0015 SOL per user, per jar.
- I-5 Mainnet buyback is not yet exercised. The program side of Jupiter routing is ready: pass-through `swap_data`, the whitelisted program, and post-checks. The keeper only implements the `mock_amm` route. The swap authority PDA cannot pay rent for intermediate accounts, so a Jupiter route must be built with `useSharedAccounts` and a pre-created SAUCE vault. The swap authority must be the route's `userTransferAuthority` and acts as a delegate on the fee vault rather than its owner (M-4), so confirm on a mainnet fork that the chosen route accepts a delegated source account. It also needs a quote-based `min_out`. Until then, keep fees in the fee vault; they are safe there.
- I-6 Keeper liveness and censorship. The keeper can delay a draw by up to one hour (`cancel_draw` timeout) but cannot pick the winner. If the keeper key is lost, rotate it with `set_keeper`. A pending draw committed by the old key has to time out.

## Operational requirements that the fixes depend on

1. Run `initialize` from the upgrade authority before transferring it to a multisig.
2. Set `config.admin` to the multisig with `propose_admin` and `accept_admin`.
3. Use a dedicated hot keeper key. It can only request draws, cancel stale draws and buy back through the whitelisted program.
4. Leave `open_jar_creation` off unless jar squatting is acceptable.
5. Build mainnet without cargo features. Build devnet with `--features devnet`.
