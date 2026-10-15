use crate::error::HoleError;
use crate::events::HoleFed;
use crate::Feed;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

pub fn handler(ctx: Context<Feed>, amount: u64) -> Result<()> {
    require!(!ctx.accounts.hole.paused, HoleError::Paused);
    require!(amount > 0, HoleError::ZeroAmount);
    require!(amount >= ctx.accounts.hole.min_feed, HoleError::BelowMinFeed);

    // Credit what the vault actually received, net of the transfer tax.
    let before = ctx.accounts.sauce_vault.amount;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.sauce_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.feeder_sauce.to_account_info(),
                mint: ctx.accounts.sauce_mint.to_account_info(),
                to: ctx.accounts.sauce_vault.to_account_info(),
                authority: ctx.accounts.feeder.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.sauce_mint.decimals,
    )?;
    ctx.accounts.sauce_vault.reload()?;
    let locked = ctx.accounts.sauce_vault.amount;
    let received = locked.saturating_sub(before);
    require!(received > 0, HoleError::ZeroAmount);

    let now = Clock::get()?.unix_timestamp;
    let hole = &mut ctx.accounts.hole;
    let record = &mut ctx.accounts.feeder_record;
    if record.wallet == Pubkey::default() {
        record.wallet = ctx.accounts.feeder.key();
        record.first_fed_ts = now;
        record.bump = ctx.bumps.feeder_record;
        hole.feeder_count = hole.feeder_count.saturating_add(1);
    }
    record.total_fed = record.total_fed.checked_add(received).ok_or(HoleError::MathOverflow)?;
    record.feed_count = record.feed_count.saturating_add(1);
    record.last_fed_ts = now;

    hole.total_fed = hole.total_fed.checked_add(received).ok_or(HoleError::MathOverflow)?;
    hole.feed_count = hole.feed_count.saturating_add(1);
    hole.feeds_since_sauced = hole.feeds_since_sauced.saturating_add(1);

    emit!(HoleFed {
        feeder: record.wallet,
        amount: received,
        feeder_total: record.total_fed,
        total_fed: hole.total_fed,
        locked,
        timestamp: now,
    });
    Ok(())
}
