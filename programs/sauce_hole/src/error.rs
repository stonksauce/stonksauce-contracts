use anchor_lang::prelude::*;

#[error_code]
pub enum HoleError {
    #[msg("The SauceHole is paused")]
    Paused,
    #[msg("Not the admin")]
    NotAdmin,
    #[msg("Signer is not the pending admin")]
    NotPendingAdmin,
    #[msg("Signer is not the upgrade authority")]
    NotUpgradeAuthority,
    #[msg("Only the keeper or admin can do this")]
    NotKeeper,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Feed is below the minimum")]
    BelowMinFeed,
    #[msg("Mint has a freeze authority or an extension that could freeze or seize hole funds")]
    UnsupportedMint,
    #[msg("STONKSAUCE and the reward token must be different mints")]
    SameMint,
    #[msg("Reward vault holds less than amount_in")]
    InsufficientRewards,
    #[msg("Swap program is not the configured one")]
    BadSwapProgram,
    #[msg("Swap may not be handed the hole account")]
    ForbiddenSwapAccount,
    #[msg("Swap spent more than amount_in or left a vault delegated, closable or reassigned")]
    SwapViolation,
    #[msg("Swap bought less STONKSAUCE than min_out")]
    SwapShortfall,
    #[msg("Arithmetic overflow")]
    MathOverflow,
}
