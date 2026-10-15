use anchor_lang::prelude::*;

/// One per feed. `amount` is what the vault received, net of transfer tax.
#[event]
pub struct HoleFed {
    pub feeder: Pubkey,
    pub amount: u64,
    pub feeder_total: u64,
    pub total_fed: u64,
    /// STONKSAUCE vault balance after the feed.
    pub locked: u64,
    pub timestamp: i64,
}

/// One per GetSauced: the STONK spent and the STONKSAUCE it bought, which
/// stays in the hole.
#[event]
pub struct GotSauced {
    pub spent: u64,
    pub bought: u64,
    pub total_spent: u64,
    pub total_bought: u64,
    /// STONKSAUCE vault balance after the buy.
    pub locked: u64,
    pub sauced_count: u64,
    /// Jar participants the keeper reported for the Sauce Meter.
    pub participants: u32,
    pub timestamp: i64,
}
