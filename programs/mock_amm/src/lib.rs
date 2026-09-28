use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

declare_id!("2AfRDK7HmdcNqkctJkw3qqFAuUK8btLfNMNvi3X35Xxt");

#[program]
pub mod mock_amm {
    use super::*;

    pub fn initialize_pool(ctx: Context<InitializePool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.mint_a = ctx.accounts.mint_a.key();
        pool.mint_b = ctx.accounts.mint_b.key();
        pool.vault_a = ctx.accounts.vault_a.key();
        pool.vault_b = ctx.accounts.vault_b.key();
        pool.authority = ctx.accounts.authority.key();
        pool.bump = ctx.bumps.pool;
        Ok(())
    }

    pub fn add_liquidity(ctx: Context<AddLiquidity>, amount_a: u64, amount_b: u64) -> Result<()> {
        require!(amount_a > 0 && amount_b > 0, AmmError::ZeroAmount);
        transfer_in(
            &ctx.accounts.user_a,
            &ctx.accounts.mint_a,
            &ctx.accounts.vault_a,
            &ctx.accounts.user,
            &ctx.accounts.token_program_a,
            amount_a,
            ctx.accounts.mint_a.decimals,
        )?;
        transfer_in(
            &ctx.accounts.user_b,
            &ctx.accounts.mint_b,
            &ctx.accounts.vault_b,
            &ctx.accounts.user,
            &ctx.accounts.token_program_b,
            amount_b,
            ctx.accounts.mint_b.decimals,
        )?;
        Ok(())
    }

    pub fn swap(ctx: Context<Swap>, amount_in: u64, min_out: u64) -> Result<()> {
        require!(amount_in > 0, AmmError::ZeroAmount);
        let a_is_in = ctx.accounts.user_in.mint == ctx.accounts.pool.mint_a;
        require!(
            a_is_in || ctx.accounts.user_in.mint == ctx.accounts.pool.mint_b,
            AmmError::WrongMint
        );

        let pool = &ctx.accounts.pool;
        let (expect_in, expect_out) = if a_is_in {
            (pool.vault_a, pool.vault_b)
        } else {
            (pool.vault_b, pool.vault_a)
        };
        require_keys_eq!(ctx.accounts.vault_in.key(), expect_in, AmmError::WrongVault);
        require_keys_eq!(ctx.accounts.vault_out.key(), expect_out, AmmError::WrongVault);
        let reserve_in = ctx.accounts.vault_in.amount;
        let reserve_out = ctx.accounts.vault_out.amount;
        require!(reserve_in > 0 && reserve_out > 0, AmmError::EmptyPool);

        let amount_out = quote(reserve_in, reserve_out, amount_in)?;
        require!(amount_out >= min_out, AmmError::Slippage);
        require!(amount_out < reserve_out, AmmError::Slippage);

        transfer_in(
            &ctx.accounts.user_in,
            &ctx.accounts.mint_in,
            &ctx.accounts.vault_in,
            &ctx.accounts.authority,
            &ctx.accounts.token_program_in,
            amount_in,
            ctx.accounts.mint_in.decimals,
        )?;

        let pool_bump = ctx.accounts.pool.bump;
        let mint_a = ctx.accounts.pool.mint_a;
        let mint_b = ctx.accounts.pool.mint_b;
        let seeds = &[b"pool".as_ref(), mint_a.as_ref(), mint_b.as_ref(), &[pool_bump]];
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program_out.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.vault_out.to_account_info(),
                    mint: ctx.accounts.mint_out.to_account_info(),
                    to: ctx.accounts.user_out.to_account_info(),
                    authority: ctx.accounts.pool.to_account_info(),
                },
                &[seeds],
            ),
            amount_out,
            ctx.accounts.mint_out.decimals,
        )?;
        Ok(())
    }
}

fn quote(reserve_in: u64, reserve_out: u64, amount_in: u64) -> Result<u64> {
    let x = reserve_in as u128;
    let y = reserve_out as u128;
    let dx = amount_in as u128;
    let dy = y
        .checked_mul(dx)
        .ok_or(AmmError::Math)?
        .checked_div(x.checked_add(dx).ok_or(AmmError::Math)?)
        .ok_or(AmmError::Math)?;
    u64::try_from(dy).map_err(|_| error!(AmmError::Math))
}

fn transfer_in<'info>(
    from: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    to: &InterfaceAccount<'info, TokenAccount>,
    authority: &Signer<'info>,
    program: &Interface<'info, TokenInterface>,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    token_interface::transfer_checked(
        CpiContext::new(
            program.to_account_info(),
            TransferChecked {
                from: from.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: authority.to_account_info(),
            },
        ),
        amount,
        decimals,
    )
}

#[account]
pub struct Pool {
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub vault_a: Pubkey,
    pub vault_b: Pubkey,
    pub authority: Pubkey,
    pub bump: u8,
}

impl Pool {
    pub const LEN: usize = 8 + 32 * 5 + 1;
}

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = Pool::LEN,
        seeds = [b"pool", mint_a.key().as_ref(), mint_b.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        payer = authority,
        token::mint = mint_a,
        token::authority = pool,
        token::token_program = token_program_a
    )]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init,
        payer = authority,
        token::mint = mint_b,
        token::authority = pool,
        token::token_program = token_program_b
    )]
    pub vault_b: InterfaceAccount<'info, TokenAccount>,
    pub token_program_a: Interface<'info, TokenInterface>,
    pub token_program_b: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    pub user: Signer<'info>,
    #[account(
        seeds = [b"pool", pool.mint_a.as_ref(), pool.mint_b.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, Pool>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint_a, token::authority = user)]
    pub user_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = user)]
    pub user_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, address = pool.vault_a)]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, address = pool.vault_b)]
    pub vault_b: InterfaceAccount<'info, TokenAccount>,
    pub token_program_a: Interface<'info, TokenInterface>,
    pub token_program_b: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct Swap<'info> {
    pub authority: Signer<'info>,
    #[account(
        seeds = [b"pool", pool.mint_a.as_ref(), pool.mint_b.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, Pool>,
    pub mint_in: InterfaceAccount<'info, Mint>,
    pub mint_out: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint_in)]
    pub user_in: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_out)]
    pub user_out: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub vault_in: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub vault_out: InterfaceAccount<'info, TokenAccount>,
    pub token_program_in: Interface<'info, TokenInterface>,
    pub token_program_out: Interface<'info, TokenInterface>,
}

#[error_code]
pub enum AmmError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Wrong mint for this pool")]
    WrongMint,
    #[msg("Pool has no liquidity")]
    EmptyPool,
    #[msg("Slippage exceeded")]
    Slippage,
    #[msg("Math overflow")]
    Math,
    #[msg("Vault is not this pool's vault")]
    WrongVault,
}
