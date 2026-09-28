use crate::error::SauceError;
use crate::events::DrawCancelled;
use crate::state::{RoundStatus, DRAW_TIMEOUT_SECS};
use crate::CancelDraw;
use anchor_lang::prelude::*;

/// Permissionless and allowed while paused, so a draw that can never settle
/// does not keep deposits frozen. Nobody, including the admin, can cancel early.
pub fn handler(ctx: Context<CancelDraw>) -> Result<()> {
    let jar = &mut ctx.accounts.jar;
    require!(jar.status == RoundStatus::DrawPending, SauceError::BadStatus);
    let clock = Clock::get()?;
    require!(
        clock.unix_timestamp >= jar.draw_requested_ts.saturating_add(DRAW_TIMEOUT_SECS),
        SauceError::DrawNotTimedOut
    );

    emit!(DrawCancelled {
        jar: jar.key(),
        randomness_account: jar.randomness_account,
        commit_slot: jar.commit_slot,
        timestamp: clock.unix_timestamp,
    });

    jar.status = RoundStatus::Open;
    jar.pending_pot = 0;
    jar.randomness_account = Pubkey::default();
    jar.commit_slot = 0;
    Ok(())
}
