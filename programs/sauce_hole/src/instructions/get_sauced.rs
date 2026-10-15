use crate::error::HoleError;
use crate::events::GotSauced;
use crate::state::{HOLE_SEED, SWAP_AUTHORITY_SEED};
use crate::token_utils::require_untouched;
use crate::GetSauced;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use anchor_spl::token_interface::{self, Approve, Revoke};

/// Swaps up to `amount_in` STONK rewards for STONKSAUCE that stays in the
/// hole. The hole never signs for the swap program: it approves the swap
/// authority PDA as delegate on the reward vault for exactly `amount_in`,
/// that PDA signs the swap, and the delegation is revoked afterwards. The
/// swap has no authority over the STONKSAUCE vault, which it can only pay into.
pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetSauced<'info>>,
    amount_in: u64,
    min_out: u64,
    participants: u32,
    swap_data: Vec<u8>,
) -> Result<()> {
    let hole = &ctx.accounts.hole;
    require!(!hole.paused, HoleError::Paused);
    // min_out is caller-chosen; a permissionless crank could sandwich the swap.
    require!(hole.is_operator(&ctx.accounts.crank.key()), HoleError::NotKeeper);
    require!(amount_in > 0, HoleError::ZeroAmount);
    require!(
        ctx.accounts.reward_vault.amount >= amount_in,
        HoleError::InsufficientRewards
    );

    let hole_key = hole.key();
    let swap_authority = ctx.accounts.swap_authority.key();
    let mut metas = Vec::with_capacity(ctx.remaining_accounts.len());
    for account in ctx.remaining_accounts.iter() {
        require_keys_neq!(account.key(), hole_key, HoleError::ForbiddenSwapAccount);
        // Only the swap authority signs: forwarding the crank's signature would
        // let the swap program spend the keeper's own wallet.
        metas.push(AccountMeta {
            pubkey: account.key(),
            is_signer: account.key() == swap_authority,
            is_writable: account.is_writable,
        });
    }
    let mut infos = ctx.remaining_accounts.to_vec();
    infos.push(ctx.accounts.swap_program.to_account_info());

    let reward_before = ctx.accounts.reward_vault.amount;
    let sauce_before = ctx.accounts.sauce_vault.amount;
    let hole_seeds = &[HOLE_SEED, &[hole.bump]];
    let authority_seeds = &[SWAP_AUTHORITY_SEED, &[ctx.bumps.swap_authority]];

    token_interface::approve(
        CpiContext::new_with_signer(
            ctx.accounts.quote_token_program.to_account_info(),
            Approve {
                to: ctx.accounts.reward_vault.to_account_info(),
                delegate: ctx.accounts.swap_authority.to_account_info(),
                authority: ctx.accounts.hole.to_account_info(),
            },
            &[hole_seeds],
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
            source: ctx.accounts.reward_vault.to_account_info(),
            authority: ctx.accounts.hole.to_account_info(),
        },
        &[hole_seeds],
    ))?;

    ctx.accounts.reward_vault.reload()?;
    ctx.accounts.sauce_vault.reload()?;
    require_untouched(&ctx.accounts.reward_vault, &hole_key)?;
    require_untouched(&ctx.accounts.sauce_vault, &hole_key)?;

    let locked = ctx.accounts.sauce_vault.amount;
    let (spent, bought) = settle(
        reward_before,
        ctx.accounts.reward_vault.amount,
        sauce_before,
        locked,
        amount_in,
        min_out,
    )?;

    let now = Clock::get()?.unix_timestamp;
    let hole = &mut ctx.accounts.hole;
    hole.total_spent = hole.total_spent.checked_add(spent).ok_or(HoleError::MathOverflow)?;
    hole.total_bought = hole.total_bought.checked_add(bought).ok_or(HoleError::MathOverflow)?;
    hole.sauced_count = hole.sauced_count.saturating_add(1);
    hole.feeds_since_sauced = 0;
    hole.participants_at_sauced = participants;
    hole.last_sauced_ts = now;

    emit!(GotSauced {
        spent,
        bought,
        total_spent: hole.total_spent,
        total_bought: hole.total_bought,
        locked,
        sauced_count: hole.sauced_count,
        participants,
        timestamp: now,
    });
    Ok(())
}

/// STONK spent and STONKSAUCE bought, from the vault balances around the
/// swap. The swap must spend something but no more than `amount_in`, and
/// must buy at least `max(1, min_out)`.
pub fn settle(
    reward_before: u64,
    reward_after: u64,
    sauce_before: u64,
    sauce_after: u64,
    amount_in: u64,
    min_out: u64,
) -> Result<(u64, u64)> {
    let spent = reward_before
        .checked_sub(reward_after)
        .ok_or(HoleError::SwapViolation)?;
    require!(spent > 0 && spent <= amount_in, HoleError::SwapViolation);
    let bought = sauce_after
        .checked_sub(sauce_before)
        .ok_or(HoleError::SwapViolation)?;
    require!(bought > 0 && bought >= min_out, HoleError::SwapShortfall);
    Ok((spent, bought))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(result: Result<(u64, u64)>) -> u32 {
        match result.unwrap_err() {
            anchor_lang::error::Error::AnchorError(e) => e.error_code_number,
            other => panic!("unexpected error {other:?}"),
        }
    }

    fn err(e: HoleError) -> u32 {
        anchor_lang::error::ERROR_CODE_OFFSET + e as u32
    }

    #[test]
    fn exact_spend_and_buy() {
        assert_eq!(settle(1_000, 400, 50, 170, 600, 120).unwrap(), (600, 120));
    }

    #[test]
    fn partial_spend_is_allowed() {
        assert_eq!(settle(1_000, 900, 0, 5, 600, 1).unwrap(), (100, 5));
    }

    #[test]
    fn overspend_is_rejected() {
        assert_eq!(code(settle(1_000, 300, 0, 500, 600, 1)), err(HoleError::SwapViolation));
    }

    #[test]
    fn nothing_spent_is_rejected() {
        assert_eq!(code(settle(1_000, 1_000, 0, 500, 600, 1)), err(HoleError::SwapViolation));
    }

    #[test]
    fn reward_vault_growing_is_rejected() {
        assert_eq!(code(settle(1_000, 1_100, 0, 500, 600, 1)), err(HoleError::SwapViolation));
    }

    #[test]
    fn below_min_out_is_rejected() {
        assert_eq!(code(settle(1_000, 400, 0, 99, 600, 100)), err(HoleError::SwapShortfall));
    }

    #[test]
    fn zero_bought_is_rejected_even_with_zero_min_out() {
        assert_eq!(code(settle(1_000, 400, 50, 50, 600, 0)), err(HoleError::SwapShortfall));
    }

    #[test]
    fn sauce_vault_shrinking_is_rejected() {
        assert_eq!(code(settle(1_000, 400, 50, 49, 600, 0)), err(HoleError::SwapViolation));
    }
}
