use crate::error::SauceError;
use crate::state::{MAX_CAP_BPS, MIN_CAP_BPS};
use crate::{AcceptAdmin, AdminConfig, AdminJar};
use anchor_lang::prelude::*;

pub fn set_creation_fee(ctx: Context<AdminConfig>, creation_fee: u64) -> Result<()> {
    ctx.accounts.config.creation_fee = creation_fee;
    Ok(())
}

pub fn set_paused(ctx: Context<AdminConfig>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    Ok(())
}

pub fn set_round_duration(ctx: Context<AdminConfig>, round_duration_secs: i64) -> Result<()> {
    require!(round_duration_secs > 0, SauceError::InvalidRoundDuration);
    ctx.accounts.config.round_duration_secs = round_duration_secs;
    Ok(())
}

pub fn set_keeper(ctx: Context<AdminConfig>, keeper: Pubkey) -> Result<()> {
    ctx.accounts.config.keeper = keeper;
    Ok(())
}

pub fn set_swap_program(ctx: Context<AdminConfig>, swap_program: Pubkey) -> Result<()> {
    ctx.accounts.config.swap_program = swap_program;
    Ok(())
}

pub fn set_open_jar_creation(ctx: Context<AdminConfig>, open: bool) -> Result<()> {
    ctx.accounts.config.open_jar_creation = open;
    Ok(())
}

pub fn propose_admin(ctx: Context<AdminConfig>, new_admin: Pubkey) -> Result<()> {
    ctx.accounts.config.pending_admin = new_admin;
    Ok(())
}

pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.new_admin.key();
    config.pending_admin = Pubkey::default();
    Ok(())
}

pub fn set_jar_min_deposit(ctx: Context<AdminJar>, min_deposit: u64) -> Result<()> {
    require!(min_deposit > 0, SauceError::ZeroAmount);
    let jar = &mut ctx.accounts.jar;
    require_limits(min_deposit, jar.max_deposit)?;
    jar.min_deposit = min_deposit;
    Ok(())
}

/// Applies to later deposits only; positions already above it are not forced out.
pub fn set_jar_max_deposit(ctx: Context<AdminJar>, max_deposit: u64) -> Result<()> {
    let jar = &mut ctx.accounts.jar;
    require_limits(jar.min_deposit, max_deposit)?;
    jar.max_deposit = max_deposit;
    Ok(())
}

/// Applies to later deposits only, like `set_jar_max_deposit`.
pub fn set_jar_cap(ctx: Context<AdminJar>, cap_bps: u16) -> Result<()> {
    require_cap(cap_bps)?;
    ctx.accounts.jar.cap_bps = cap_bps;
    Ok(())
}

pub fn require_cap(cap_bps: u16) -> Result<()> {
    require!(
        (MIN_CAP_BPS..=MAX_CAP_BPS).contains(&cap_bps),
        SauceError::InvalidCap
    );
    Ok(())
}

/// A maximum of 0 means unlimited; otherwise it must admit the minimum.
pub fn require_limits(min_deposit: u64, max_deposit: u64) -> Result<()> {
    require!(
        max_deposit == 0 || max_deposit >= min_deposit,
        SauceError::InvalidMaxDeposit
    );
    Ok(())
}
