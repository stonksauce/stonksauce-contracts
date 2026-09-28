use anchor_lang::prelude::*;

/// Emitted once per settled round. The jar account only keeps the most recent
/// result, so this log is the record clients replay to build draw history.
#[event]
pub struct DrawSettled {
    pub jar: Pubkey,
    pub stake_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub winner: Pubkey,
    /// Monotonic per jar, starting at 1 for the first settled round.
    pub round_index: u64,
    pub pot: u64,
    pub prize: u64,
    pub fee: u64,
    pub randomness: u128,
    pub participant_count: u32,
    pub total_deposits: u64,
    pub timestamp: i64,
}

/// Emitted when a jar is opened, so clients can list jars without scanning
/// every program account.
#[event]
pub struct JarCreated {
    pub jar: Pubkey,
    pub stake_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub creator: Pubkey,
    pub cap_bps: u16,
    pub timestamp: i64,
    /// 0 means no per-wallet maximum.
    pub max_deposit: u64,
}

/// A pending draw that timed out. Repeated cancels on one jar are worth
/// investigating: they are the only way to re-roll a draw.
#[event]
pub struct DrawCancelled {
    pub jar: Pubkey,
    pub randomness_account: Pubkey,
    pub commit_slot: u64,
    pub timestamp: i64,
}

#[event]
pub struct BuybackBurned {
    pub jar: Pubkey,
    pub spent: u64,
    pub burned: u64,
    pub timestamp: i64,
}
