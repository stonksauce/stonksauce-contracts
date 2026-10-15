use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

pub mod error;
pub mod events;
pub mod instructions;
pub mod state;
pub mod token_utils;

use error::HoleError;
use state::{Feeder, Hole, FEEDER_SEED, HOLE_SEED, SWAP_AUTHORITY_SEED};

declare_id!("Ho1ezeQ2fb1zEL3aK94e4q5j7g3vwGfi5PguPWGjB2VC");

/// No instruction in this program transfers, burns, approves, freezes or
/// closes the STONKSAUCE vault. The hole PDA signs only an approve and a
/// revoke on the STONK reward vault inside `get_sauced`.
#[program]
pub mod sauce_hole {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        keeper: Pubkey,
        swap_program: Pubkey,
        min_feed: u64,
        participants: u32,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, keeper, swap_program, min_feed, participants)
    }

    pub fn feed(ctx: Context<Feed>, amount: u64) -> Result<()> {
        instructions::feed::handler(ctx, amount)
    }

    pub fn get_sauced<'info>(
        ctx: Context<'_, '_, 'info, 'info, GetSauced<'info>>,
        amount_in: u64,
        min_out: u64,
        participants: u32,
        swap_data: Vec<u8>,
    ) -> Result<()> {
        instructions::get_sauced::handler(ctx, amount_in, min_out, participants, swap_data)
    }

    pub fn set_keeper(ctx: Context<AdminHole>, keeper: Pubkey) -> Result<()> {
        instructions::admin::set_keeper(ctx, keeper)
    }

    pub fn set_swap_program(ctx: Context<AdminHole>, swap_program: Pubkey) -> Result<()> {
        instructions::admin::set_swap_program(ctx, swap_program)
    }

    pub fn set_paused(ctx: Context<AdminHole>, paused: bool) -> Result<()> {
        instructions::admin::set_paused(ctx, paused)
    }

    pub fn set_min_feed(ctx: Context<AdminHole>, min_feed: u64) -> Result<()> {
        instructions::admin::set_min_feed(ctx, min_feed)
    }

    pub fn propose_admin(ctx: Context<AdminHole>, new_admin: Pubkey) -> Result<()> {
        instructions::admin::propose_admin(ctx, new_admin)
    }

    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        instructions::admin::accept_admin(ctx)
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(init, payer = admin, space = Hole::LEN, seeds = [HOLE_SEED], bump)]
    pub hole: Account<'info, Hole>,
    #[account(mint::token_program = sauce_token_program)]
    pub sauce_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mint::token_program = quote_token_program)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,
    /// `init_if_needed`: anyone can create an ATA for the hole before it is
    /// initialized, and that must not block initialization.
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = sauce_mint,
        associated_token::authority = hole,
        associated_token::token_program = sauce_token_program
    )]
    pub sauce_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = quote_mint,
        associated_token::authority = hole,
        associated_token::token_program = quote_token_program
    )]
    pub reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        constraint = program.programdata_address()? == Some(program_data.key())
            @ HoleError::NotUpgradeAuthority
    )]
    pub program: Program<'info, crate::program::SauceHole>,
    #[account(
        constraint = program_data.upgrade_authority_address == Some(admin.key())
            @ HoleError::NotUpgradeAuthority
    )]
    pub program_data: Account<'info, ProgramData>,
    pub sauce_token_program: Interface<'info, TokenInterface>,
    pub quote_token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Feed<'info> {
    #[account(mut)]
    pub feeder: Signer<'info>,
    #[account(mut, seeds = [HOLE_SEED], bump = hole.bump)]
    pub hole: Account<'info, Hole>,
    #[account(
        init_if_needed,
        payer = feeder,
        space = Feeder::LEN,
        seeds = [FEEDER_SEED, feeder.key().as_ref()],
        bump
    )]
    pub feeder_record: Account<'info, Feeder>,
    #[account(address = hole.sauce_mint, mint::token_program = sauce_token_program)]
    pub sauce_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        token::mint = sauce_mint,
        token::authority = feeder,
        token::token_program = sauce_token_program
    )]
    pub feeder_sauce: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, address = hole.sauce_vault, token::token_program = sauce_token_program)]
    pub sauce_vault: InterfaceAccount<'info, TokenAccount>,
    pub sauce_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct GetSauced<'info> {
    pub crank: Signer<'info>,
    #[account(mut, seeds = [HOLE_SEED], bump = hole.bump)]
    pub hole: Account<'info, Hole>,
    #[account(
        mut,
        address = hole.reward_vault,
        token::authority = hole,
        token::token_program = quote_token_program
    )]
    pub reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        address = hole.sauce_vault,
        token::authority = hole,
        token::token_program = sauce_token_program
    )]
    pub sauce_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: signs the swap in place of the hole. It owns nothing and is only
    /// ever a delegate on the reward vault for `amount_in`, revoked after the swap.
    #[account(seeds = [SWAP_AUTHORITY_SEED], bump)]
    pub swap_authority: UncheckedAccount<'info>,
    /// CHECK: pinned to the admin-configured swap program.
    #[account(executable, address = hole.swap_program @ HoleError::BadSwapProgram)]
    pub swap_program: UncheckedAccount<'info>,
    pub quote_token_program: Interface<'info, TokenInterface>,
    pub sauce_token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct AdminHole<'info> {
    pub admin: Signer<'info>,
    #[account(mut, seeds = [HOLE_SEED], bump = hole.bump, has_one = admin @ HoleError::NotAdmin)]
    pub hole: Account<'info, Hole>,
}

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub new_admin: Signer<'info>,
    #[account(
        mut,
        seeds = [HOLE_SEED],
        bump = hole.bump,
        constraint = hole.pending_admin == new_admin.key() @ HoleError::NotPendingAdmin
    )]
    pub hole: Account<'info, Hole>,
}
