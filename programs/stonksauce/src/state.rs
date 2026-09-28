use crate::error::SauceError;
use crate::fenwick::{self, MAX_INDEX, MAX_LEAVES};
use anchor_lang::prelude::*;

pub const PROTOCOL_FEE_BPS: u16 = 500;
pub const MIN_CAP_BPS: u16 = 500;
/// 100%: no share cap, leaving only the jar's absolute `max_deposit`.
pub const MAX_CAP_BPS: u16 = 10_000;
/// A draw that has not settled this long after `request_draw` can be cancelled
/// by anyone, so a stuck oracle or payout can never lock deposits.
pub const DRAW_TIMEOUT_SECS: i64 = 3600;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum RoundStatus {
    Open,
    DrawPending,
}

impl Default for RoundStatus {
    fn default() -> Self {
        Self::Open
    }
}

#[account]
pub struct Config {
    pub admin: Pubkey,
    pub sauce_mint: Pubkey,
    pub creation_fee: u64,
    pub paused: bool,
    pub protocol_fee_bps: u16,
    pub round_duration_secs: i64,
    pub bump: u8,
    /// Hot key allowed to request draws and run buybacks. The admin can too.
    pub keeper: Pubkey,
    /// Set by `propose_admin`; becomes admin once it signs `accept_admin`.
    pub pending_admin: Pubkey,
    /// Only program `buyback_and_burn` may CPI into.
    pub swap_program: Pubkey,
    /// When false only the admin can create jars.
    pub open_jar_creation: bool,
}

impl Config {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 1 + 2 + 8 + 1 + 32 + 32 + 32 + 1;

    pub fn is_operator(&self, key: &Pubkey) -> bool {
        *key == self.keeper || *key == self.admin
    }
}

#[account]
pub struct Jar {
    pub stake_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub stake_vault: Pubkey,
    pub reward_vault: Pubkey,
    pub fee_vault: Pubkey,
    /// The jar's shard directory.
    pub directory: Pubkey,
    pub creator: Pubkey,
    pub total_deposits: u64,
    pub participant_count: u32,
    pub cap_bps: u16,
    pub status: RoundStatus,
    pub last_draw_ts: i64,
    pub next_draw_ts: i64,
    pub pending_pot: u64,
    pub last_winner: Pubkey,
    pub last_pot: u64,
    pub last_prize: u64,
    pub last_fee: u64,
    pub last_randomness: u128,
    pub pending_winner: Pubkey,
    pub bump: u8,
    /// Number of rounds settled so far; also the index of the latest draw.
    pub round_index: u64,
    /// Smallest non-zero position. Makes filling every shard slot with dust
    /// cost real capital.
    pub min_deposit: u64,
    /// Switchboard randomness account committed by `request_draw`.
    pub randomness_account: Pubkey,
    /// `seed_slot` of that commitment. A re-commit changes it, which voids the draw.
    pub commit_slot: u64,
    pub draw_requested_ts: i64,
    /// Largest position one wallet may hold, in raw stake units; 0 means no
    /// limit. Jars created before this field read 0 from their spare bytes.
    pub max_deposit: u64,
}

/// Why a position would break a jar's limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapBreach {
    Share,
    Absolute,
}

/// `position` and `total` are after the deposit. The first deposit into an
/// empty jar is exempt from the share cap, since it is 100% of the jar.
pub fn check_caps(
    position: u64,
    total: u64,
    prior_total: u64,
    cap_bps: u16,
    max_deposit: u64,
) -> core::result::Result<(), CapBreach> {
    if max_deposit > 0 && position > max_deposit {
        return Err(CapBreach::Absolute);
    }
    if prior_total > 0 && (position as u128) * 10_000 > (cap_bps as u128) * (total as u128) {
        return Err(CapBreach::Share);
    }
    Ok(())
}

impl Jar {
    pub const LEN: usize = 512;

    /// Part of a deposit made at `now` that counts toward this round's draw:
    /// the fraction of the round still to run. A deposit seconds before the
    /// draw buys seconds' worth of odds, so sniping the pot does not pay.
    pub fn round_weight(&self, amount: u64, now: i64) -> u64 {
        round_weight(amount, self.last_draw_ts, self.next_draw_ts, now)
    }
}

pub fn round_weight(amount: u64, round_start: i64, round_end: i64, now: i64) -> u64 {
    let length = round_end.saturating_sub(round_start);
    if length <= 0 {
        return amount;
    }
    let left = round_end.saturating_sub(now).clamp(0, length);
    ((amount as u128) * (left as u128) / (length as u128)) as u64
}

#[cfg(test)]
mod tests {
    use super::round_weight;

    #[test]
    fn deposit_counts_for_the_part_of_the_round_left() {
        assert_eq!(round_weight(1_000, 0, 100, 0), 1_000);
        assert_eq!(round_weight(1_000, 0, 100, 25), 750);
        assert_eq!(round_weight(1_000, 0, 100, 99), 10);
        assert_eq!(round_weight(1_000, 0, 100, 100), 0);
    }

    #[test]
    fn overdue_and_early_deposits_are_clamped() {
        assert_eq!(round_weight(1_000, 0, 100, 5_000), 0);
        assert_eq!(round_weight(1_000, 50, 150, 0), 1_000);
    }

    #[test]
    fn large_amounts_do_not_overflow() {
        assert_eq!(round_weight(u64::MAX, 0, 86_400, 43_200), u64::MAX / 2);
    }

    #[test]
    fn zero_length_round_counts_in_full() {
        assert_eq!(round_weight(1_000, 100, 100, 100), 1_000);
    }
}

#[cfg(test)]
mod cap_tests {
    use super::*;
    use anchor_lang::AnchorSerialize;

    #[test]
    fn absolute_cap_binds_even_the_first_deposit() {
        assert_eq!(check_caps(5_000, 5_000, 0, 5_000, 5_000), Ok(()));
        assert_eq!(check_caps(5_001, 5_001, 0, 5_000, 5_000), Err(CapBreach::Absolute));
    }

    #[test]
    fn zero_max_means_unlimited() {
        assert_eq!(check_caps(u64::MAX, u64::MAX, 0, MAX_CAP_BPS, 0), Ok(()));
    }

    #[test]
    fn share_cap_exempts_the_first_deposit_only() {
        assert_eq!(check_caps(100, 100, 0, 5_000, 0), Ok(()));
        // A top-up by the only depositor would be 100% of the jar.
        assert_eq!(check_caps(150, 150, 100, 5_000, 0), Err(CapBreach::Share));
        assert_eq!(check_caps(100, 200, 100, 5_000, 0), Ok(()));
        assert_eq!(check_caps(101, 201, 100, 5_000, 0), Err(CapBreach::Share));
    }

    #[test]
    fn full_share_cap_leaves_only_the_absolute_limit() {
        assert_eq!(check_caps(150, 150, 100, MAX_CAP_BPS, 0), Ok(()));
        assert_eq!(check_caps(150, 150, 100, MAX_CAP_BPS, 120), Err(CapBreach::Absolute));
    }

    #[test]
    fn max_deposit_fits_the_existing_jar_allocation() {
        let jar = Jar {
            stake_mint: Pubkey::default(),
            quote_mint: Pubkey::default(),
            stake_vault: Pubkey::default(),
            reward_vault: Pubkey::default(),
            fee_vault: Pubkey::default(),
            directory: Pubkey::default(),
            creator: Pubkey::default(),
            total_deposits: 0,
            participant_count: 0,
            cap_bps: 0,
            status: RoundStatus::Open,
            last_draw_ts: 0,
            next_draw_ts: 0,
            pending_pot: 0,
            last_winner: Pubkey::default(),
            last_pot: 0,
            last_prize: 0,
            last_fee: 0,
            last_randomness: 0,
            pending_winner: Pubkey::default(),
            bump: 0,
            round_index: 0,
            min_deposit: 0,
            randomness_account: Pubkey::default(),
            commit_slot: 0,
            draw_requested_ts: 0,
            max_deposit: u64::MAX,
        };
        assert!(8 + jar.try_to_vec().unwrap().len() <= Jar::LEN);
    }
}

#[cfg(test)]
mod shard_tests {
    use super::*;
    use crate::fenwick::{add, prefix_sum};

    const CPI_CREATE_LIMIT: usize = 10_240;

    fn empty_shard() -> Box<Shard> {
        let mut shard: Box<Shard> = Box::new(unsafe { std::mem::zeroed() });
        shard.next_slot = 1;
        shard
    }

    #[test]
    fn accounts_fit_a_cpi_create() {
        assert!(8 + Directory::LEN <= CPI_CREATE_LIMIT);
        assert!(8 + Shard::LEN <= CPI_CREATE_LIMIT);
        assert_eq!(Directory::LEN, std::mem::size_of::<Directory>());
        assert_eq!(Shard::LEN, std::mem::size_of::<Shard>());
    }

    #[test]
    fn index_zero_means_empty_in_every_shard() {
        for shard in [0, 1, 383] {
            let index = position_index(shard, 1);
            assert_ne!(index, 0);
            assert_eq!(shard_of(index), shard);
            assert_eq!(slot_of(index), 1);
            assert_eq!(slot_of(position_index(shard, MAX_INDEX)), MAX_INDEX);
        }
    }

    #[test]
    fn only_the_canonical_shard_pda_is_accepted() {
        let stake_mint = Pubkey::new_unique();
        let (pda, _) = Pubkey::find_program_address(
            &[b"shard", stake_mint.as_ref(), &3u32.to_le_bytes()],
            &crate::ID,
        );
        assert!(require_shard_address(&pda, &stake_mint, 3).is_ok());
        assert!(require_shard_address(&pda, &stake_mint, 4).is_err());
        assert!(require_shard_address(&pda, &Pubkey::new_unique(), 3).is_err());
        assert!(require_shard_address(&Pubkey::new_unique(), &stake_mint, 3).is_err());
    }

    #[test]
    fn shard_fills_then_recycles_freed_slots() {
        let mut shard = empty_shard();
        for expected in 1..=MAX_INDEX {
            assert_eq!(shard.allocate_slot(), Some(expected));
        }
        assert_eq!(shard.allocate_slot(), None);
        shard.release_slot(7);
        shard.release_slot(300);
        assert_eq!(shard.allocate_slot(), Some(300));
        assert_eq!(shard.allocate_slot(), Some(7));
        assert_eq!(shard.allocate_slot(), None);
    }

    #[test]
    fn stale_shard_counts_full_balances_until_refreshed() {
        let mut directory: Box<Directory> = Box::new(unsafe { std::mem::zeroed() });
        let mut shard = empty_shard();
        shard.balances[1] = 100;
        add(&mut shard.tree, 1, 40);
        directory.balances[0] = 100;
        directory.round_weights[0] = 40;

        assert_eq!(directory.round_weight_of(0, 0), 40);
        assert_eq!(directory.round_weight_of(0, 1), 100);

        refresh_shard(&mut directory, &mut shard, 1);
        assert_eq!(shard.round, 1);
        assert_eq!(prefix_sum(&shard.tree, MAX_INDEX), 100);
        assert_eq!(directory.round_weight_of(0, 1), 100);

        add(&mut shard.tree, 1, 5);
        directory.round_weights[0] += 5;
        refresh_shard(&mut directory, &mut shard, 1);
        assert_eq!(prefix_sum(&shard.tree, MAX_INDEX), 105);
    }
}

#[account]
pub struct Position {
    pub jar: Pubkey,
    pub depositor: Pubkey,
    pub amount: u64,
    /// `shard * MAX_LEAVES + slot`, or 0 while the position holds nothing.
    /// Slots are recycled on full withdraw.
    pub fenwick_index: u32,
    pub bump: u8,
}

impl Position {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 4 + 1;
}

/// Shards per jar, bounded by the 10 KiB directory.
pub const MAX_SHARDS: usize = 384;

pub fn position_index(shard: u32, slot: u32) -> u32 {
    shard * MAX_LEAVES as u32 + slot
}

pub fn shard_of(index: u32) -> u32 {
    index / MAX_LEAVES as u32
}

pub fn slot_of(index: u32) -> u32 {
    index % MAX_LEAVES as u32
}

/// A jar's shards are exactly the PDAs of its stake mint and their index, so
/// once this passes, `shard.index` can be trusted for directory lookups.
pub fn require_shard_address(shard: &Pubkey, stake_mint: &Pubkey, index: u32) -> Result<()> {
    let (expected, _) = Pubkey::find_program_address(
        &[b"shard", stake_mint.as_ref(), &index.to_le_bytes()],
        &crate::ID,
    );
    require_keys_eq!(*shard, expected, SauceError::WrongShard);
    Ok(())
}

/// Per-shard totals, so a draw can weigh every shard without loading them.
#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct Directory {
    pub jar: Pubkey,
    pub shard_count: u32,
    pub _pad: u32,
    pub _reserved: [u8; 8],
    /// Sum of each shard's balances.
    pub balances: [u64; MAX_SHARDS],
    /// Sum of each shard's draw weights, valid only when `rounds` matches the jar.
    pub round_weights: [u64; MAX_SHARDS],
    /// Round whose draw weights each shard's tree holds.
    pub rounds: [u64; MAX_SHARDS],
    /// Slots in use per shard.
    pub occupied: [u16; MAX_SHARDS],
}

impl Directory {
    pub const LEN: usize = 32 + 4 + 4 + 8 + 8 * MAX_SHARDS * 3 + 2 * MAX_SHARDS;

    /// A shard untouched since an earlier round held every balance all round.
    pub fn round_weight_of(&self, shard: usize, round: u64) -> u64 {
        if self.rounds[shard] == round {
            self.round_weights[shard]
        } else {
            self.balances[shard]
        }
    }
}

/// Up to 511 depositors. Owners are not stored: a slot's holder is the
/// Position whose `fenwick_index` points at it, which keeps a shard under 10 KiB.
#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct Shard {
    pub jar: Pubkey,
    pub index: u32,
    /// Next never-used slot.
    pub next_slot: u32,
    /// Round whose draw weights `tree` holds. Older means stale.
    pub round: u64,
    pub free_count: u32,
    pub _pad: u32,
    /// Each slot's full position balance.
    pub balances: [u64; MAX_LEAVES],
    /// Fenwick tree of the draw weights: balances pro-rated by how much of
    /// the round they were held.
    pub tree: [u64; MAX_LEAVES],
    /// Stack of slots freed by full withdrawals.
    pub free: [u16; MAX_LEAVES],
}

impl Shard {
    pub const LEN: usize = 32 + 4 + 4 + 8 + 4 + 4 + 8 * MAX_LEAVES * 2 + 2 * MAX_LEAVES;

    /// A freed slot if any, else the next never-used one.
    pub fn allocate_slot(&mut self) -> Option<u32> {
        if self.free_count > 0 {
            self.free_count -= 1;
            return Some(self.free[self.free_count as usize] as u32);
        }
        if self.next_slot <= MAX_INDEX {
            let slot = self.next_slot;
            self.next_slot += 1;
            return Some(slot);
        }
        None
    }

    pub fn release_slot(&mut self, slot: u32) {
        self.free[self.free_count as usize] = slot as u16;
        self.free_count += 1;
    }
}

/// Resets a shard left over from an earlier round so its tree holds this
/// round's weights. Settlement never touches shards; each one catches up
/// the first time it is used in a new round, so no instruction loops over
/// the whole jar.
pub fn refresh_shard(directory: &mut Directory, shard: &mut Shard, round: u64) {
    if shard.round == round {
        return;
    }
    fenwick::rebuild(&mut shard.tree, &shard.balances);
    shard.round = round;
    let i = shard.index as usize;
    directory.rounds[i] = round;
    directory.round_weights[i] = directory.balances[i];
}
