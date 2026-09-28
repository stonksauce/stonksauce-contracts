use crate::error::SauceError;
use crate::randomness::Randomness;
use crate::state::RoundStatus;
use crate::RequestDraw;
use anchor_lang::prelude::*;

pub fn handler(ctx: Context<RequestDraw>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.paused, SauceError::Paused);
    // Switchboard only lets the randomness authority reveal, so whoever commits
    // can peek at the outcome and withhold it. Keep that power with the keeper.
    require!(
        config.is_operator(&ctx.accounts.crank.key()),
        SauceError::NotKeeper
    );
    require!(ctx.accounts.jar.status == RoundStatus::Open, SauceError::BadStatus);
    let clock = Clock::get()?;
    require!(
        clock.unix_timestamp >= ctx.accounts.jar.next_draw_ts,
        SauceError::TooEarly
    );
    require!(ctx.accounts.jar.total_deposits > 0, SauceError::NoDeposits);
    let pot = ctx.accounts.reward_vault.amount;
    require!(pot > 0, SauceError::EmptyPot);

    let randomness = Randomness::load(&ctx.accounts.randomness_account.to_account_info())?;
    randomness.require_fresh_commit(clock.slot)?;

    let jar = &mut ctx.accounts.jar;
    jar.status = RoundStatus::DrawPending;
    jar.pending_pot = pot;
    jar.pending_winner = Pubkey::default();
    jar.randomness_account = ctx.accounts.randomness_account.key();
    jar.commit_slot = randomness.seed_slot;
    jar.draw_requested_ts = clock.unix_timestamp;
    Ok(())
}
