use crate::error::SauceError;
use crate::fenwick;
use crate::state::{
    check_caps, position_index, refresh_shard, require_shard_address, shard_of, slot_of, CapBreach,
    RoundStatus,
};
use crate::Deposit;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

pub fn handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(!ctx.accounts.config.paused, SauceError::Paused);
    require!(amount > 0, SauceError::ZeroAmount);
    require!(ctx.accounts.jar.status == RoundStatus::Open, SauceError::DrawPending);

    // Credit what the vault actually received, net of any Token-2022 fee.
    let before = ctx.accounts.stake_vault.amount;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.stake_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.user_stake.to_account_info(),
                mint: ctx.accounts.stake_mint.to_account_info(),
                to: ctx.accounts.stake_vault.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.stake_mint.decimals,
    )?;
    ctx.accounts.stake_vault.reload()?;
    let received = ctx.accounts.stake_vault.amount.saturating_sub(before);
    require!(received > 0, SauceError::ZeroAmount);

    let jar = &mut ctx.accounts.jar;
    let position = &mut ctx.accounts.position;
    let new_pos = position.amount.checked_add(received).ok_or(SauceError::MathOverflow)?;
    let new_total = jar.total_deposits.checked_add(received).ok_or(SauceError::MathOverflow)?;
    require!(new_pos >= jar.min_deposit, SauceError::BelowMinDeposit);
    check_caps(new_pos, new_total, jar.total_deposits, jar.cap_bps, jar.max_deposit).map_err(|breach| match breach {
        CapBreach::Share => error!(SauceError::OverCap),
        CapBreach::Absolute => error!(SauceError::OverMaxDeposit),
    })?;

    let mut directory = ctx.accounts.directory.load_mut()?;
    let mut shard = ctx.accounts.shard.load_mut()?;
    require_keys_eq!(shard.jar, jar.key(), SauceError::WrongShard);
    require_shard_address(&ctx.accounts.shard.key(), &jar.stake_mint, shard.index)?;
    let s = shard.index;
    refresh_shard(&mut directory, &mut shard, jar.round_index);
    if position.fenwick_index == 0 {
        let slot = shard.allocate_slot().ok_or(SauceError::ShardFull)?;
        position.jar = jar.key();
        position.depositor = ctx.accounts.depositor.key();
        position.fenwick_index = position_index(s, slot);
        position.bump = ctx.bumps.position;
        directory.occupied[s as usize] = directory.occupied[s as usize]
            .checked_add(1)
            .ok_or(SauceError::MathOverflow)?;
        jar.participant_count = jar.participant_count.saturating_add(1);
    }
    require!(shard_of(position.fenwick_index) == s, SauceError::WrongShard);

    let slot = slot_of(position.fenwick_index);
    let counted = jar.round_weight(received, Clock::get()?.unix_timestamp);
    shard.balances[slot as usize] = shard.balances[slot as usize].saturating_add(received);
    fenwick::add(&mut shard.tree, slot, counted);
    let i = s as usize;
    directory.balances[i] = directory.balances[i].saturating_add(received);
    directory.round_weights[i] = directory.round_weights[i].saturating_add(counted);

    position.amount = new_pos;
    jar.total_deposits = new_total;
    Ok(())
}
