//! Loaded by `solana-test-validator` at the Switchboard mainnet program address
//! so the production stonksauce binary runs unchanged on localnet. The account
//! layout and discriminator match Switchboard's `RandomnessAccountData`; the
//! authority chooses the revealed value, which is only acceptable locally.
use anchor_lang::prelude::*;

declare_id!("SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv");

#[program]
pub mod mock_switchboard {
    use super::*;

    pub fn randomness_init(ctx: Context<RandomnessInit>, queue: Pubkey) -> Result<()> {
        let mut r = ctx.accounts.randomness.load_init()?;
        r.authority = ctx.accounts.authority.key();
        r.queue = queue;
        Ok(())
    }

    pub fn randomness_commit(ctx: Context<RandomnessUpdate>) -> Result<()> {
        let slot = Clock::get()?.slot;
        let mut r = ctx.accounts.randomness.load_mut()?;
        r.seed_slot = slot.saturating_sub(1);
        r.reveal_slot = 0;
        r.value = [0u8; 32];
        Ok(())
    }

    pub fn randomness_reveal(ctx: Context<RandomnessUpdate>, value: [u8; 32]) -> Result<()> {
        let slot = Clock::get()?.slot;
        let mut r = ctx.accounts.randomness.load_mut()?;
        require!(r.seed_slot != 0 && slot > r.seed_slot, MockError::NotCommitted);
        r.reveal_slot = slot;
        r.value = value;
        Ok(())
    }
}

#[account(zero_copy)]
#[repr(C)]
pub struct RandomnessAccountData {
    pub authority: Pubkey,
    pub queue: Pubkey,
    pub seed_slothash: [u8; 32],
    pub seed_slot: u64,
    pub oracle: Pubkey,
    pub reveal_slot: u64,
    pub value: [u8; 32],
    pub lut_slot: u64,
    pub _ebuf3: [u8; 24],
    pub _ebuf2: [u8; 64],
    pub _ebuf1: [u8; 128],
    pub active_secp256k1_signer: [u8; 64],
    pub active_secp256k1_expiration: i64,
}

#[derive(Accounts)]
pub struct RandomnessInit<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<RandomnessAccountData>()
    )]
    pub randomness: AccountLoader<'info, RandomnessAccountData>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RandomnessUpdate<'info> {
    pub authority: Signer<'info>,
    #[account(mut, constraint = randomness.load()?.authority == authority.key() @ MockError::NotAuthority)]
    pub randomness: AccountLoader<'info, RandomnessAccountData>,
}

#[error_code]
pub enum MockError {
    #[msg("Only the randomness authority can commit or reveal")]
    NotAuthority,
    #[msg("Commit before reveal, in an earlier slot")]
    NotCommitted,
}
