use anchor_lang::prelude::*;

pub const HOLE_SEED: &[u8] = b"hole";
pub const FEEDER_SEED: &[u8] = b"feeder";
pub const SWAP_AUTHORITY_SEED: &[u8] = b"swap_authority";

/// The singleton hole. It owns the STONKSAUCE vault and the STONK reward
/// vault, and no instruction moves STONKSAUCE out of the vault.
#[account]
pub struct Hole {
    pub admin: Pubkey,
    /// Set by `propose_admin`; becomes admin once it signs `accept_admin`.
    pub pending_admin: Pubkey,
    /// Hot key allowed to run GetSauced. The admin can too.
    pub keeper: Pubkey,
    /// The only program `get_sauced` may CPI into.
    pub swap_program: Pubkey,
    pub sauce_mint: Pubkey,
    /// STONK, the reward token GetSauced spends.
    pub quote_mint: Pubkey,
    /// STONKSAUCE ATA of the hole.
    pub sauce_vault: Pubkey,
    /// STONK ATA of the hole, where StonkFun pays its rewards.
    pub reward_vault: Pubkey,
    pub paused: bool,
    /// STONKSAUCE credited to feeders, net of transfer tax.
    pub total_fed: u64,
    /// STONKSAUCE bought by GetSauced.
    pub total_bought: u64,
    /// STONK spent by GetSauced.
    pub total_spent: u64,
    pub sauced_count: u64,
    pub feed_count: u64,
    /// Smallest feed accepted, before transfer tax. Keeps dust feeds from
    /// padding the feed count.
    pub min_feed: u64,
    pub feeder_count: u32,
    /// Reset by every GetSauced.
    pub feeds_since_sauced: u32,
    /// Jar participants at the last GetSauced (or at opening), reported by the keeper.
    pub participants_at_sauced: u32,
    pub opened_ts: i64,
    pub last_sauced_ts: i64,
    pub bump: u8,
}

impl Hole {
    pub const LEN: usize = 8 + 32 * 8 + 1 + 8 * 6 + 4 * 3 + 8 * 2 + 1;

    pub fn is_operator(&self, key: &Pubkey) -> bool {
        *key == self.keeper || *key == self.admin
    }
}

/// Per-wallet feed record, so a later program can read contributions.
#[account]
pub struct Feeder {
    pub wallet: Pubkey,
    pub total_fed: u64,
    pub feed_count: u32,
    pub first_fed_ts: i64,
    pub last_fed_ts: i64,
    pub bump: u8,
}

impl Feeder {
    pub const LEN: usize = 8 + 32 + 8 + 4 + 8 + 8 + 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hole() -> Hole {
        Hole {
            admin: Pubkey::new_unique(),
            pending_admin: Pubkey::default(),
            keeper: Pubkey::new_unique(),
            swap_program: Pubkey::new_unique(),
            sauce_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            sauce_vault: Pubkey::new_unique(),
            reward_vault: Pubkey::new_unique(),
            paused: false,
            total_fed: u64::MAX,
            total_bought: u64::MAX,
            total_spent: u64::MAX,
            sauced_count: u64::MAX,
            feed_count: u64::MAX,
            min_feed: u64::MAX,
            feeder_count: u32::MAX,
            feeds_since_sauced: u32::MAX,
            participants_at_sauced: u32::MAX,
            opened_ts: i64::MAX,
            last_sauced_ts: i64::MAX,
            bump: 255,
        }
    }

    #[test]
    fn hole_len_matches_serialized_size() {
        let mut data = Vec::new();
        hole().try_serialize(&mut data).unwrap();
        assert_eq!(data.len(), Hole::LEN);
    }

    #[test]
    fn feeder_len_matches_serialized_size() {
        let feeder = Feeder {
            wallet: Pubkey::new_unique(),
            total_fed: u64::MAX,
            feed_count: u32::MAX,
            first_fed_ts: i64::MAX,
            last_fed_ts: i64::MAX,
            bump: 255,
        };
        let mut data = Vec::new();
        feeder.try_serialize(&mut data).unwrap();
        assert_eq!(data.len(), Feeder::LEN);
    }

    #[test]
    fn operators_are_keeper_and_admin_only() {
        let h = hole();
        assert!(h.is_operator(&h.keeper));
        assert!(h.is_operator(&h.admin));
        assert!(!h.is_operator(&Pubkey::new_unique()));
        assert!(!h.is_operator(&Pubkey::default()));
    }
}
