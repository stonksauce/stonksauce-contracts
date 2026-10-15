use crate::{AcceptAdmin, AdminHole};
use anchor_lang::prelude::*;

// None of these touch the STONKSAUCE vault. They stay callable after the
// program is made immutable, so a swap venue upgrade or a lost keeper key
// cannot strand the rewards.

pub fn set_keeper(ctx: Context<AdminHole>, keeper: Pubkey) -> Result<()> {
    ctx.accounts.hole.keeper = keeper;
    Ok(())
}

pub fn set_swap_program(ctx: Context<AdminHole>, swap_program: Pubkey) -> Result<()> {
    ctx.accounts.hole.swap_program = swap_program;
    Ok(())
}

pub fn set_paused(ctx: Context<AdminHole>, paused: bool) -> Result<()> {
    ctx.accounts.hole.paused = paused;
    Ok(())
}

pub fn set_min_feed(ctx: Context<AdminHole>, min_feed: u64) -> Result<()> {
    ctx.accounts.hole.min_feed = min_feed;
    Ok(())
}

pub fn propose_admin(ctx: Context<AdminHole>, new_admin: Pubkey) -> Result<()> {
    ctx.accounts.hole.pending_admin = new_admin;
    Ok(())
}

pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
    let hole = &mut ctx.accounts.hole;
    hole.admin = ctx.accounts.new_admin.key();
    hole.pending_admin = Pubkey::default();
    Ok(())
}
