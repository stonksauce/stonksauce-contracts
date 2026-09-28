use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

pub mod error;
pub mod events;
pub mod fenwick;
pub mod instructions;
pub mod randomness;
pub mod state;
pub mod token_utils;

use error::SauceError;
use state::{Config, Directory, Jar, Position, Shard};

declare_id!("FhGABn7RcdhGc4CrNx4ohDdAsFUFiHz48y4QDMfxe2UF");

#[program]
pub mod stonksauce {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        creation_fee: u64,
        round_duration_secs: i64,
        keeper: Pubkey,
        swap_program: Pubkey,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, creation_fee, round_duration_secs, keeper, swap_program)
    }

    pub fn set_creation_fee(ctx: Context<AdminConfig>, creation_fee: u64) -> Result<()> {
        instructions::admin::set_creation_fee(ctx, creation_fee)
    }

    pub fn set_paused(ctx: Context<AdminConfig>, paused: bool) -> Result<()> {
        instructions::admin::set_paused(ctx, paused)
    }

    pub fn set_round_duration(ctx: Context<AdminConfig>, round_duration_secs: i64) -> Result<()> {
        instructions::admin::set_round_duration(ctx, round_duration_secs)
    }

    pub fn set_keeper(ctx: Context<AdminConfig>, keeper: Pubkey) -> Result<()> {
        instructions::admin::set_keeper(ctx, keeper)
    }

    pub fn set_swap_program(ctx: Context<AdminConfig>, swap_program: Pubkey) -> Result<()> {
        instructions::admin::set_swap_program(ctx, swap_program)
    }

    pub fn set_open_jar_creation(ctx: Context<AdminConfig>, open: bool) -> Result<()> {
        instructions::admin::set_open_jar_creation(ctx, open)
    }

    pub fn propose_admin(ctx: Context<AdminConfig>, new_admin: Pubkey) -> Result<()> {
        instructions::admin::propose_admin(ctx, new_admin)
    }

    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        instructions::admin::accept_admin(ctx)
    }

    pub fn set_jar_min_deposit(ctx: Context<AdminJar>, min_deposit: u64) -> Result<()> {
        instructions::admin::set_jar_min_deposit(ctx, min_deposit)
    }

    pub fn set_jar_max_deposit(ctx: Context<AdminJar>, max_deposit: u64) -> Result<()> {
        instructions::admin::set_jar_max_deposit(ctx, max_deposit)
    }

    pub fn set_jar_cap(ctx: Context<AdminJar>, cap_bps: u16) -> Result<()> {
        instructions::admin::set_jar_cap(ctx, cap_bps)
    }

    pub fn create_jar(ctx: Context<CreateJar>, cap_bps: u16, min_deposit: u64, max_deposit: u64) -> Result<()> {
        instructions::create_jar::handler(ctx, cap_bps, min_deposit, max_deposit)
    }

    pub fn open_shard(ctx: Context<OpenShard>, index: u32) -> Result<()> {
        instructions::open_shard::handler(ctx, index)
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, amount)
    }

    pub fn request_draw(ctx: Context<RequestDraw>) -> Result<()> {
        instructions::request_draw::handler(ctx)
    }

    pub fn fulfill_draw(ctx: Context<FulfillDraw>) -> Result<()> {
        instructions::fulfill_draw::handler(ctx)
    }

    pub fn cancel_draw(ctx: Context<CancelDraw>) -> Result<()> {
        instructions::cancel_draw::handler(ctx)
    }

    pub fn buyback_and_burn<'info>(
        ctx: Context<'_, '_, 'info, 'info, BuybackAndBurn<'info>>,
        amount_in: u64,
        min_out: u64,
        swap_data: Vec<u8>,
    ) -> Result<()> {
        instructions::buyback::handler(ctx, amount_in, min_out, swap_data)
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = Config::LEN,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,
    pub sauce_mint: InterfaceAccount<'info, Mint>,
    #[account(
        constraint = program.programdata_address()? == Some(program_data.key())
            @ SauceError::NotUpgradeAuthority
    )]
    pub program: Program<'info, crate::program::Stonksauce>,
    #[account(
        constraint = program_data.upgrade_authority_address == Some(admin.key())
            @ SauceError::NotUpgradeAuthority
    )]
    pub program_data: Account<'info, ProgramData>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminConfig<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ SauceError::NotAdmin
    )]
    pub config: Account<'info, Config>,
}

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub new_admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.pending_admin == new_admin.key() @ SauceError::NotPendingAdmin
    )]
    pub config: Account<'info, Config>,
}

#[derive(Accounts)]
pub struct AdminJar<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ SauceError::NotAdmin
    )]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
}

#[derive(Accounts)]
pub struct CreateJar<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub stake_mint: Box<InterfaceAccount<'info, Mint>>,
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, address = config.sauce_mint)]
    pub sauce_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        token::mint = sauce_mint,
        token::authority = creator,
        token::token_program = sauce_token_program
    )]
    pub creator_sauce: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init,
        payer = creator,
        space = Jar::LEN,
        seeds = [b"jar", stake_mint.key().as_ref()],
        bump
    )]
    pub jar: Box<Account<'info, Jar>>,
    #[account(
        init,
        payer = creator,
        space = 8 + Directory::LEN,
        seeds = [b"directory", stake_mint.key().as_ref()],
        bump
    )]
    pub directory: AccountLoader<'info, Directory>,
    #[account(
        init,
        payer = creator,
        space = 8 + Shard::LEN,
        seeds = [b"shard", stake_mint.key().as_ref(), &0u32.to_le_bytes()],
        bump
    )]
    pub shard: AccountLoader<'info, Shard>,
    #[account(
        init,
        payer = creator,
        associated_token::mint = stake_mint,
        associated_token::authority = jar,
        associated_token::token_program = stake_token_program
    )]
    pub stake_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init,
        payer = creator,
        associated_token::mint = quote_mint,
        associated_token::authority = jar,
        associated_token::token_program = quote_token_program
    )]
    pub reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init,
        payer = creator,
        seeds = [b"fee_vault", stake_mint.key().as_ref()],
        bump,
        token::mint = quote_mint,
        token::authority = jar,
        token::token_program = quote_token_program
    )]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub stake_token_program: Interface<'info, TokenInterface>,
    pub quote_token_program: Interface<'info, TokenInterface>,
    pub sauce_token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Permissionless: whoever needs room pays the rent for the next shard.
#[derive(Accounts)]
#[instruction(index: u32)]
pub struct OpenShard<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
    #[account(mut, address = jar.directory @ SauceError::WrongShard)]
    pub directory: AccountLoader<'info, Directory>,
    #[account(
        init,
        payer = payer,
        space = 8 + Shard::LEN,
        seeds = [b"shard", jar.stake_mint.as_ref(), &index.to_le_bytes()],
        bump
    )]
    pub shard: AccountLoader<'info, Shard>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
    #[account(mut, address = jar.directory @ SauceError::WrongShard)]
    pub directory: AccountLoader<'info, Directory>,
    /// Checked against the jar and the position in the handler.
    #[account(mut)]
    pub shard: AccountLoader<'info, Shard>,
    #[account(
        init_if_needed,
        payer = depositor,
        space = Position::LEN,
        seeds = [b"position", jar.key().as_ref(), depositor.key().as_ref()],
        bump
    )]
    pub position: Account<'info, Position>,
    #[account(address = jar.stake_mint, mint::token_program = stake_token_program)]
    pub stake_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        token::mint = stake_mint,
        token::authority = depositor,
        token::token_program = stake_token_program
    )]
    pub user_stake: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        address = jar.stake_vault,
        token::authority = jar,
        token::token_program = stake_token_program
    )]
    pub stake_vault: InterfaceAccount<'info, TokenAccount>,
    pub stake_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    pub depositor: Signer<'info>,
    #[account(mut, seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
    #[account(mut, address = jar.directory @ SauceError::WrongShard)]
    pub directory: AccountLoader<'info, Directory>,
    /// Checked against the jar and the position in the handler.
    #[account(mut)]
    pub shard: AccountLoader<'info, Shard>,
    #[account(
        mut,
        seeds = [b"position", jar.key().as_ref(), depositor.key().as_ref()],
        bump = position.bump,
        has_one = jar,
        has_one = depositor
    )]
    pub position: Account<'info, Position>,
    #[account(address = jar.stake_mint, mint::token_program = stake_token_program)]
    pub stake_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        token::mint = stake_mint,
        token::authority = depositor,
        token::token_program = stake_token_program
    )]
    pub user_stake: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        address = jar.stake_vault,
        token::authority = jar,
        token::token_program = stake_token_program
    )]
    pub stake_vault: InterfaceAccount<'info, TokenAccount>,
    pub stake_token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct RequestDraw<'info> {
    pub crank: Signer<'info>,
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
    #[account(address = jar.reward_vault)]
    pub reward_vault: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: owner, discriminator, queue and freshness checked in `Randomness::load`.
    pub randomness_account: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct FulfillDraw<'info> {
    pub crank: Signer<'info>,
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
    #[account(mut, address = jar.directory @ SauceError::WrongShard)]
    pub directory: AccountLoader<'info, Directory>,
    /// The shard the ticket lands in; checked in the handler.
    #[account(mut)]
    pub shard: AccountLoader<'info, Shard>,
    /// Holder of the drawn slot; its index is checked in the handler.
    #[account(has_one = jar @ SauceError::WinnerMismatch)]
    pub winner_position: Box<Account<'info, Position>>,
    #[account(address = jar.quote_mint, mint::token_program = quote_token_program)]
    pub quote_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        address = jar.reward_vault,
        token::mint = quote_mint,
        token::authority = jar,
        token::token_program = quote_token_program
    )]
    pub reward_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        address = jar.fee_vault,
        token::mint = quote_mint,
        token::authority = jar,
        token::token_program = quote_token_program
    )]
    pub fee_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = quote_mint, token::token_program = quote_token_program)]
    pub winner_quote: InterfaceAccount<'info, TokenAccount>,
    pub quote_token_program: Interface<'info, TokenInterface>,
    /// CHECK: must be the account committed in `request_draw`; parsed in the handler.
    #[account(address = jar.randomness_account @ SauceError::BadRandomnessAccount)]
    pub randomness_account: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct CancelDraw<'info> {
    #[account(mut, seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
}

#[derive(Accounts)]
pub struct BuybackAndBurn<'info> {
    #[account(mut)]
    pub crank: Signer<'info>,
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(seeds = [b"jar", jar.stake_mint.as_ref()], bump = jar.bump)]
    pub jar: Account<'info, Jar>,
    #[account(
        mut,
        address = jar.fee_vault,
        token::authority = jar,
        token::token_program = quote_token_program
    )]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = config.sauce_mint)]
    pub sauce_mint: Box<InterfaceAccount<'info, Mint>>,
    /// A PDA rather than the jar's SAUCE ATA: when the stake mint is SAUCE that
    /// ATA is the stake vault, and burning from it would burn deposits.
    #[account(
        init_if_needed,
        payer = crank,
        seeds = [b"sauce_vault", jar.stake_mint.as_ref()],
        bump,
        token::mint = sauce_mint,
        token::authority = jar,
        token::token_program = sauce_token_program
    )]
    pub sauce_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: signs the swap in place of the jar. It owns nothing and is only
    /// ever a delegate on the fee vault for `amount_in`, revoked after the swap.
    #[account(seeds = [b"swap_authority", jar.stake_mint.as_ref()], bump)]
    pub swap_authority: UncheckedAccount<'info>,
    /// CHECK: pinned to the admin-configured swap program.
    #[account(executable, address = config.swap_program @ SauceError::BadSwapProgram)]
    pub swap_program: UncheckedAccount<'info>,
    pub quote_token_program: Interface<'info, TokenInterface>,
    pub sauce_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}
