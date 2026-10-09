use crate::error::SauceError;
use crate::events::JarCreated;
use crate::instructions::admin::{require_cap, require_limits};
use crate::state::RoundStatus;
use crate::token_utils::{require_safe_mint, require_transfer_fee_mint};
use crate::CreateJar;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn};

pub fn handler(ctx: Context<CreateJar>, cap_bps: u16, min_deposit: u64, max_deposit: u64) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.paused, SauceError::Paused);
    // One jar per stake mint, so an open door lets anyone squat a mint with a
    // bogus quote mint and strand the rewards StonkFun pays that jar.
    require!(
        config.open_jar_creation || ctx.accounts.creator.key() == config.admin,
        SauceError::JarCreationClosed
    );
    require_cap(cap_bps)?;
    require!(min_deposit > 0, SauceError::ZeroAmount);
    require_limits(min_deposit, max_deposit)?;
    require_keys_neq!(
        ctx.accounts.stake_mint.key(),
        ctx.accounts.quote_mint.key(),
        SauceError::UnsupportedMint
    );

    let stake_info = ctx.accounts.stake_mint.to_account_info();
    require_transfer_fee_mint(&stake_info)?;
    require_keys_eq!(
        *stake_info.owner,
        ctx.accounts.stake_token_program.key(),
        SauceError::NotRewardMint
    );
    require_safe_mint(&ctx.accounts.stake_mint)?;
    require_safe_mint(&ctx.accounts.quote_mint)?;

    let fee = config.creation_fee;
    if fee > 0 {
        token_interface::burn(
            CpiContext::new(
                ctx.accounts.sauce_token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.sauce_mint.to_account_info(),
                    from: ctx.accounts.creator_sauce.to_account_info(),
                    authority: ctx.accounts.creator.to_account_info(),
                },
            ),
            fee,
        )?;
    }

    let round_duration = ctx.accounts.config.round_duration_secs;
    let clock = Clock::get()?;
    let jar = &mut ctx.accounts.jar;
    jar.stake_mint = ctx.accounts.stake_mint.key();
    jar.quote_mint = ctx.accounts.quote_mint.key();
    jar.stake_vault = ctx.accounts.stake_vault.key();
    jar.reward_vault = ctx.accounts.reward_vault.key();
    jar.fee_vault = ctx.accounts.fee_vault.key();
    jar.directory = ctx.accounts.directory.key();
    jar.creator = ctx.accounts.creator.key();
    jar.total_deposits = 0;
    jar.participant_count = 0;
    jar.cap_bps = cap_bps;
    jar.status = RoundStatus::Open;
    jar.last_draw_ts = clock.unix_timestamp;
    jar.next_draw_ts = clock.unix_timestamp.saturating_add(round_duration);
    jar.pending_pot = 0;
    jar.last_winner = Pubkey::default();
    jar.last_pot = 0;
    jar.last_prize = 0;
    jar.last_fee = 0;
    jar.last_randomness = 0;
    jar.pending_winner = Pubkey::default();
    jar.round_index = 0;
    jar.bump = ctx.bumps.jar;
    jar.min_deposit = min_deposit;
    jar.randomness_account = Pubkey::default();
    jar.draw_nonce = 0;
    jar.draw_requested_ts = 0;
    jar.max_deposit = max_deposit;

    emit!(JarCreated {
        jar: jar.key(),
        stake_mint: jar.stake_mint,
        quote_mint: jar.quote_mint,
        creator: jar.creator,
        cap_bps: jar.cap_bps,
        timestamp: clock.unix_timestamp,
        max_deposit,
    });

    let mut directory = ctx.accounts.directory.load_init()?;
    directory.jar = jar.key();
    directory.shard_count = 1;
    let mut shard = ctx.accounts.shard.load_init()?;
    shard.jar = jar.key();
    shard.index = 0;
    shard.next_slot = 1;
    Ok(())
}
