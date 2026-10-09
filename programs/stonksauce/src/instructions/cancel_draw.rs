use crate::error::SauceError;
use crate::events::DrawCancelled;
use crate::randomness;
use crate::state::{RoundStatus, DRAW_TIMEOUT_SECS};
use crate::CancelDraw;
use anchor_lang::prelude::*;

/// Permissionless and allowed while paused, so a draw that can never settle
/// does not keep deposits frozen. Nobody, including the admin, can cancel early.
/// Once ORAO has answered, the draw can only be settled: otherwise whoever
/// sees an unwelcome winner could let the timeout pass and re-roll.
pub fn handler(ctx: Context<CancelDraw>) -> Result<()> {
    let jar = &mut ctx.accounts.jar;
    require!(jar.status == RoundStatus::DrawPending, SauceError::BadStatus);
    let seed = randomness::draw_seed(&jar.key(), jar.round_index, jar.draw_nonce);
    require!(
        !randomness::is_fulfilled(&ctx.accounts.randomness_account, &seed)?,
        SauceError::DrawFulfilled
    );
    let clock = Clock::get()?;
    require!(
        clock.unix_timestamp >= jar.draw_requested_ts.saturating_add(DRAW_TIMEOUT_SECS),
        SauceError::DrawNotTimedOut
    );

    emit!(DrawCancelled {
        jar: jar.key(),
        randomness_account: jar.randomness_account,
        draw_nonce: jar.draw_nonce,
        timestamp: clock.unix_timestamp,
    });

    jar.status = RoundStatus::Open;
    jar.pending_pot = 0;
    jar.randomness_account = Pubkey::default();
    jar.draw_nonce = 0;
    Ok(())
}
