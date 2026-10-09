use crate::error::SauceError;
use crate::events::DrawSettled;
use crate::fenwick;
use crate::randomness;
use crate::state::{
    position_index, refresh_shard, require_shard_address, RoundStatus, MAX_SHARDS,
    PROTOCOL_FEE_BPS,
};
use crate::FulfillDraw;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

pub fn handler(ctx: Context<FulfillDraw>) -> Result<()> {
    require!(!ctx.accounts.config.paused, SauceError::Paused);
    require!(
        ctx.accounts.jar.status == RoundStatus::DrawPending,
        SauceError::BadStatus
    );
    require!(ctx.accounts.jar.total_deposits > 0, SauceError::NoDeposits);

    let clock = Clock::get()?;
    let seed = randomness::draw_seed(
        &ctx.accounts.jar.key(),
        ctx.accounts.jar.round_index,
        ctx.accounts.jar.draw_nonce,
    );
    let randomness =
        randomness::fulfilled_value(&ctx.accounts.randomness_account.to_account_info(), &seed)?;

    let pot = ctx.accounts.jar.pending_pot.min(ctx.accounts.reward_vault.amount);
    require!(pot > 0, SauceError::EmptyPot);

    let round = ctx.accounts.jar.round_index;
    let mut directory = ctx.accounts.directory.load_mut()?;
    let mut shard = ctx.accounts.shard.load_mut()?;
    require_keys_eq!(shard.jar, ctx.accounts.jar.key(), SauceError::WrongShard);
    require_shard_address(
        &ctx.accounts.shard.key(),
        &ctx.accounts.jar.stake_mint,
        shard.index,
    )?;
    let count = directory.shard_count as usize;
    require!(count <= MAX_SHARDS, SauceError::TooManyShards);
    let balances = directory.balances[..count]
        .iter()
        .fold(0u64, |sum, w| sum.saturating_add(*w));
    require!(
        balances == ctx.accounts.jar.total_deposits,
        SauceError::TreeMismatch
    );
    let round_total = (0..count)
        .fold(0u64, |sum, i| sum.saturating_add(directory.round_weight_of(i, round)));
    // Everyone joined after the countdown ended, so nobody holds round
    // weight; they are equally late, so draw on balances instead.
    let on_balances = round_total == 0;
    let weight_of = |i: usize| {
        if on_balances {
            directory.balances[i]
        } else {
            directory.round_weight_of(i, round)
        }
    };
    let mut ticket = (randomness % (if on_balances { balances } else { round_total }) as u128) as u64;
    let mut chosen = count;
    for i in 0..count {
        let w = weight_of(i);
        if ticket < w {
            chosen = i;
            break;
        }
        ticket -= w;
    }
    require!(shard.index as usize == chosen, SauceError::WrongShard);

    if on_balances {
        let sh = &mut *shard;
        fenwick::rebuild(&mut sh.tree, &sh.balances);
        sh.round = round;
        directory.rounds[chosen] = round;
        directory.round_weights[chosen] = directory.balances[chosen];
    } else {
        refresh_shard(&mut directory, &mut shard, round);
    }
    let slot = fenwick::find(&shard.tree, ticket);
    require!(slot <= fenwick::MAX_INDEX, SauceError::WinnerMismatch);
    let drawn = position_index(shard.index, slot);
    drop(shard);
    drop(directory);

    // Shards are not reset here: bumping `round_index` below marks them all
    // stale, and each rebuilds to full balances the next time it is touched.
    let position = &ctx.accounts.winner_position;
    require!(
        position.fenwick_index == drawn && position.amount > 0,
        SauceError::WinnerMismatch
    );
    let winner = position.depositor;
    require_keys_eq!(
        ctx.accounts.winner_quote.owner,
        winner,
        SauceError::WinnerMismatch
    );

    let fee = pot
        .checked_mul(PROTOCOL_FEE_BPS as u64)
        .ok_or(SauceError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(SauceError::MathOverflow)?;
    let prize = pot.checked_sub(fee).ok_or(SauceError::MathOverflow)?;

    let jar_bump = ctx.accounts.jar.bump;
    let stake_mint = ctx.accounts.jar.stake_mint;
    let seeds = &[b"jar".as_ref(), stake_mint.as_ref(), &[jar_bump]];
    let decimals = ctx.accounts.quote_mint.decimals;

    for (to, amount) in [
        (ctx.accounts.winner_quote.to_account_info(), prize),
        (ctx.accounts.fee_vault.to_account_info(), fee),
    ] {
        if amount == 0 {
            continue;
        }
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.quote_token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.reward_vault.to_account_info(),
                    mint: ctx.accounts.quote_mint.to_account_info(),
                    to,
                    authority: ctx.accounts.jar.to_account_info(),
                },
                &[seeds],
            ),
            amount,
            decimals,
        )?;
    }

    let duration = ctx.accounts.config.round_duration_secs;
    let jar = &mut ctx.accounts.jar;
    jar.status = RoundStatus::Open;
    jar.last_draw_ts = clock.unix_timestamp;
    jar.next_draw_ts = clock.unix_timestamp.saturating_add(duration);
    jar.last_winner = winner;
    jar.last_pot = pot;
    jar.last_prize = prize;
    jar.last_fee = fee;
    jar.last_randomness = randomness;
    jar.pending_pot = 0;
    jar.pending_winner = winner;
    jar.randomness_account = Pubkey::default();
    jar.draw_nonce = 0;
    jar.round_index = jar.round_index.saturating_add(1);

    emit!(DrawSettled {
        jar: jar.key(),
        stake_mint: jar.stake_mint,
        quote_mint: jar.quote_mint,
        winner,
        round_index: jar.round_index,
        pot,
        prize,
        fee,
        randomness,
        participant_count: jar.participant_count,
        total_deposits: jar.total_deposits,
        timestamp: clock.unix_timestamp,
    });
    Ok(())
}
