use crate::error::SauceError;
use crate::events::BuybackFed;
use crate::BuybackAndFeed;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use anchor_spl::token_interface::{self, Approve, Revoke, TokenAccount};

/// The jar never signs for the swap program, so the swap cannot touch any
/// jar-owned account's authority, extensions or other vaults. The jar approves
/// the swap authority PDA as delegate on the fee vault for exactly `amount_in`,
/// that PDA signs the swap, and the delegation is revoked afterwards.
///
/// The swap pays STONKSAUCE straight into the SauceHole's vault, so the token
/// tax is paid once, on the swap, rather than again on a second transfer.
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, BuybackAndFeed<'info>>,
    amount_in: u64,
    min_out: u64,
    swap_data: Vec<u8>,
) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.paused, SauceError::Paused);
    // min_out is caller-chosen; a permissionless crank could sandwich the swap.
    require!(
        config.is_operator(&ctx.accounts.crank.key()),
        SauceError::NotKeeper
    );
    require!(amount_in > 0, SauceError::ZeroAmount);
    require!(
        ctx.accounts.fee_vault.amount >= amount_in,
        SauceError::InsufficientPosition
    );

    let jar = &ctx.accounts.jar;
    let jar_key = jar.key();
    let swap_authority = ctx.accounts.swap_authority.key();
    let forbidden = [jar_key, jar.stake_vault, jar.reward_vault];
    let mut metas = Vec::with_capacity(ctx.remaining_accounts.len());
    for account in ctx.remaining_accounts.iter() {
        require!(
            !forbidden.contains(account.key),
            SauceError::ForbiddenSwapAccount
        );
        // Only the swap authority signs: forwarding the crank's signature
        // would let the swap program spend from the crank's wallet.
        metas.push(AccountMeta {
            pubkey: account.key(),
            is_signer: account.key() == swap_authority,
            is_writable: account.is_writable,
        });
    }
    let mut infos = ctx.remaining_accounts.to_vec();
    infos.push(ctx.accounts.swap_program.to_account_info());

    let fee_before = ctx.accounts.fee_vault.amount;
    let hole_before = ctx.accounts.hole_vault.amount;
    let jar_bump = jar.bump;
    let stake_mint = jar.stake_mint;
    let jar_seeds = &[b"jar".as_ref(), stake_mint.as_ref(), &[jar_bump]];
    let authority_seeds = &[
        b"swap_authority".as_ref(),
        stake_mint.as_ref(),
        &[ctx.bumps.swap_authority],
    ];

    token_interface::approve(
        CpiContext::new_with_signer(
            ctx.accounts.quote_token_program.to_account_info(),
            Approve {
                to: ctx.accounts.fee_vault.to_account_info(),
                delegate: ctx.accounts.swap_authority.to_account_info(),
                authority: ctx.accounts.jar.to_account_info(),
            },
            &[jar_seeds],
        ),
        amount_in,
    )?;

    invoke_signed(
        &Instruction {
            program_id: ctx.accounts.swap_program.key(),
            accounts: metas,
            data: swap_data,
        },
        &infos,
        &[authority_seeds],
    )?;

    token_interface::revoke(CpiContext::new_with_signer(
        ctx.accounts.quote_token_program.to_account_info(),
        Revoke {
            source: ctx.accounts.fee_vault.to_account_info(),
            authority: ctx.accounts.jar.to_account_info(),
        },
        &[jar_seeds],
    ))?;

    ctx.accounts.fee_vault.reload()?;
    ctx.accounts.hole_vault.reload()?;
    require_untouched(&ctx.accounts.fee_vault, &jar_key)?;
    require_keys_eq!(
        ctx.accounts.hole_vault.owner,
        ctx.accounts.hole.key(),
        SauceError::BuybackViolation
    );

    let spent = fee_before
        .checked_sub(ctx.accounts.fee_vault.amount)
        .ok_or(SauceError::BuybackViolation)?;
    require!(spent > 0 && spent <= amount_in, SauceError::BuybackViolation);
    // Net of the token tax, which Token-2022 withholds in the hole vault.
    let fed = ctx.accounts.hole_vault.amount.saturating_sub(hole_before);
    require!(fed > 0 && fed >= min_out, SauceError::BuybackEmpty);

    emit!(BuybackFed {
        jar: jar_key,
        spent,
        fed,
        timestamp: Clock::get()?.unix_timestamp,
    });
    Ok(())
}

/// A delegate, close authority or new owner left behind would let someone
/// drain the vault in a later transaction.
fn require_untouched(vault: &InterfaceAccount<TokenAccount>, jar: &Pubkey) -> Result<()> {
    require_keys_eq!(vault.owner, *jar, SauceError::BuybackViolation);
    require!(vault.delegate.is_none(), SauceError::BuybackViolation);
    require!(vault.close_authority.is_none(), SauceError::BuybackViolation);
    Ok(())
}
