use anchor_lang::prelude::*;

#[error_code]
pub enum SauceError {
    #[msg("Protocol fee must be 500 bps")]
    InvalidProtocolFee,
    #[msg("Platform is paused")]
    Paused,
    #[msg("Not the admin")]
    NotAdmin,
    #[msg("Stake mint must be Token-2022 with a transfer fee")]
    NotRewardMint,
    #[msg("A jar already exists for this mint")]
    JarExists,
    #[msg("Jar is frozen for the draw")]
    DrawPending,
    #[msg("Round is not ready to draw")]
    TooEarly,
    #[msg("Reward pot is empty")]
    EmptyPot,
    #[msg("No deposits in the jar")]
    NoDeposits,
    #[msg("Deposit would exceed the per-wallet cap")]
    OverCap,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Withdraw exceeds position")]
    InsufficientPosition,
    #[msg("Shard is full; use another shard or open a new one")]
    ShardFull,
    #[msg("Winner token account does not match the selected depositor")]
    WinnerMismatch,
    #[msg("Randomness is not ready")]
    RandomnessUnready,
    #[msg("Randomness commitment is stale or was replaced")]
    RandomnessStale,
    #[msg("Not a Switchboard randomness account on the default queue")]
    BadRandomnessAccount,
    #[msg("Cap bps out of range")]
    InvalidCap,
    #[msg("Round duration must be positive")]
    InvalidRoundDuration,
    #[msg("Jar status does not allow this instruction")]
    BadStatus,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Buyback produced no STONKSAUCE")]
    BuybackEmpty,
    #[msg("Signer is not the upgrade authority")]
    NotUpgradeAuthority,
    #[msg("Only the keeper or admin can do this")]
    NotKeeper,
    #[msg("Signer is not the pending admin")]
    NotPendingAdmin,
    #[msg("Jar creation is admin only")]
    JarCreationClosed,
    #[msg("Mint has an extension that could freeze or seize jar funds")]
    UnsupportedMint,
    #[msg("Position would fall below the jar minimum")]
    BelowMinDeposit,
    #[msg("Draw has not timed out yet")]
    DrawNotTimedOut,
    #[msg("Swap program is not the configured one")]
    BadSwapProgram,
    #[msg("Swap may not be handed the jar, stake vault or reward vault")]
    ForbiddenSwapAccount,
    #[msg("Buyback spent more than amount_in or left a vault delegated")]
    BuybackViolation,
    #[msg("Shard balances do not match jar deposits")]
    TreeMismatch,
    #[msg("Shard does not belong to this jar, position or draw")]
    WrongShard,
    #[msg("Jar has reached its shard limit")]
    TooManyShards,
    #[msg("Shards must be opened in order")]
    ShardOutOfOrder,
    #[msg("Position would exceed the jar's maximum per wallet")]
    OverMaxDeposit,
    #[msg("Maximum per wallet is below the jar minimum")]
    InvalidMaxDeposit,
}
