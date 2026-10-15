use crate::error::HoleError;
use crate::token_utils::require_safe_mint;
use crate::Initialize;
use anchor_lang::prelude::*;

pub fn handler(
    ctx: Context<Initialize>,
    keeper: Pubkey,
    swap_program: Pubkey,
    min_feed: u64,
    participants: u32,
) -> Result<()> {
    require_keys_neq!(
        ctx.accounts.sauce_mint.key(),
        ctx.accounts.quote_mint.key(),
        HoleError::SameMint
    );
    require_safe_mint(&ctx.accounts.sauce_mint)?;
    require_safe_mint(&ctx.accounts.quote_mint)?;

    let hole = &mut ctx.accounts.hole;
    hole.admin = ctx.accounts.admin.key();
    hole.pending_admin = Pubkey::default();
    hole.keeper = keeper;
    hole.swap_program = swap_program;
    hole.sauce_mint = ctx.accounts.sauce_mint.key();
    hole.quote_mint = ctx.accounts.quote_mint.key();
    hole.sauce_vault = ctx.accounts.sauce_vault.key();
    hole.reward_vault = ctx.accounts.reward_vault.key();
    hole.paused = false;
    hole.total_fed = 0;
    hole.total_bought = 0;
    hole.total_spent = 0;
    hole.sauced_count = 0;
    hole.feed_count = 0;
    hole.min_feed = min_feed;
    hole.feeder_count = 0;
    hole.feeds_since_sauced = 0;
    hole.participants_at_sauced = participants;
    hole.opened_ts = Clock::get()?.unix_timestamp;
    hole.last_sauced_ts = 0;
    hole.bump = ctx.bumps.hole;
    Ok(())
}
