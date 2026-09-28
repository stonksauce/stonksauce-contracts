use crate::error::SauceError;
use crate::state::PROTOCOL_FEE_BPS;
use crate::Initialize;
use anchor_lang::prelude::*;

pub fn handler(
    ctx: Context<Initialize>,
    creation_fee: u64,
    round_duration_secs: i64,
    keeper: Pubkey,
    swap_program: Pubkey,
) -> Result<()> {
    require!(round_duration_secs > 0, SauceError::InvalidRoundDuration);
    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.sauce_mint = ctx.accounts.sauce_mint.key();
    config.creation_fee = creation_fee;
    config.paused = false;
    config.protocol_fee_bps = PROTOCOL_FEE_BPS;
    config.round_duration_secs = round_duration_secs;
    config.bump = ctx.bumps.config;
    config.keeper = keeper;
    config.pending_admin = Pubkey::default();
    config.swap_program = swap_program;
    config.open_jar_creation = false;
    Ok(())
}
