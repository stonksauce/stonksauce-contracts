use crate::error::SauceError;
use crate::randomness;
use crate::state::RoundStatus;
use crate::RequestDraw;
use anchor_lang::prelude::*;

/// `nonce` only has to make the seed unused: a retry after `cancel_draw` in
/// the same round needs a fresh one.
pub fn handler(ctx: Context<RequestDraw>, nonce: u64) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.paused, SauceError::Paused);
    // The requester pays ORAO's fee and the request rent.
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

    let seed = randomness::draw_seed(&ctx.accounts.jar.key(), ctx.accounts.jar.round_index, nonce);
    require_keys_eq!(
        ctx.accounts.randomness_request.key(),
        randomness::request_address(&seed),
        SauceError::BadRandomnessAccount
    );
    randomness::request(
        &ctx.accounts.vrf_program.to_account_info(),
        &ctx.accounts.crank.to_account_info(),
        &ctx.accounts.network_state.to_account_info(),
        &ctx.accounts.treasury.to_account_info(),
        &ctx.accounts.randomness_request.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        seed,
    )?;

    let jar = &mut ctx.accounts.jar;
    jar.status = RoundStatus::DrawPending;
    jar.pending_pot = pot;
    jar.pending_winner = Pubkey::default();
    jar.randomness_account = ctx.accounts.randomness_request.key();
    jar.draw_nonce = nonce;
    jar.draw_requested_ts = clock.unix_timestamp;
    Ok(())
}
