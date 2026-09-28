use crate::error::SauceError;
use crate::fenwick;
use crate::state::{refresh_shard, require_shard_address, shard_of, slot_of, RoundStatus};
use crate::Withdraw;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

/// Deliberately ignores `Config::paused`: pausing must never lock deposits.
pub fn handler(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    require!(amount > 0, SauceError::ZeroAmount);
    require!(ctx.accounts.jar.status == RoundStatus::Open, SauceError::DrawPending);
    let remaining = ctx
        .accounts
        .position
        .amount
        .checked_sub(amount)
        .ok_or(SauceError::InsufficientPosition)?;
    require!(
        remaining == 0 || remaining >= ctx.accounts.jar.min_deposit,
        SauceError::BelowMinDeposit
    );

    let jar_bump = ctx.accounts.jar.bump;
    let stake_mint = ctx.accounts.jar.stake_mint;
    let seeds = &[b"jar".as_ref(), stake_mint.as_ref(), &[jar_bump]];

    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.stake_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.stake_vault.to_account_info(),
                mint: ctx.accounts.stake_mint.to_account_info(),
                to: ctx.accounts.user_stake.to_account_info(),
                authority: ctx.accounts.jar.to_account_info(),
            },
            &[seeds],
        ),
        amount,
        ctx.accounts.stake_mint.decimals,
    )?;

    let mut directory = ctx.accounts.directory.load_mut()?;
    let mut shard = ctx.accounts.shard.load_mut()?;
    let position = &mut ctx.accounts.position;
    let jar = &mut ctx.accounts.jar;
    require_keys_eq!(shard.jar, jar.key(), SauceError::WrongShard);
    require_shard_address(&ctx.accounts.shard.key(), &jar.stake_mint, shard.index)?;
    require!(
        position.fenwick_index != 0 && shard_of(position.fenwick_index) == shard.index,
        SauceError::WrongShard
    );
    refresh_shard(&mut directory, &mut shard, jar.round_index);

    let slot = slot_of(position.fenwick_index);
    let balance = shard.balances[slot as usize];
    let counted = fenwick::leaf(&shard.tree, slot);
    // The position keeps the same fraction of its round weight as of its balance.
    let kept = if balance == 0 {
        0
    } else {
        ((counted as u128) * (remaining as u128) / (balance as u128)) as u64
    };
    let dropped = counted.saturating_sub(kept);
    shard.balances[slot as usize] = balance.saturating_sub(amount);
    fenwick::sub(&mut shard.tree, slot, dropped);
    let i = shard.index as usize;
    directory.balances[i] = directory.balances[i].saturating_sub(amount);
    directory.round_weights[i] = directory.round_weights[i].saturating_sub(dropped);

    position.amount = remaining;
    jar.total_deposits = jar.total_deposits.saturating_sub(amount);
    if remaining == 0 {
        // Free the slot so one-time depositors cannot fill the shard forever.
        shard.release_slot(slot);
        directory.occupied[i] = directory.occupied[i].saturating_sub(1);
        position.fenwick_index = 0;
        jar.participant_count = jar.participant_count.saturating_sub(1);
    }
    Ok(())
}
