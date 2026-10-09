//! Loaded by `solana-test-validator` at the ORAO VRF program address so the
//! production stonksauce binary runs unchanged on localnet. Account layouts,
//! names and the `request_v2` instruction match ORAO's; the network authority
//! chooses the fulfilled value, which is only acceptable locally.
use anchor_lang::prelude::*;

declare_id!("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");

pub const CONFIG_ACCOUNT_SEED: &[u8] = b"orao-vrf-network-configuration";
pub const RANDOMNESS_ACCOUNT_SEED: &[u8] = b"orao-vrf-randomness-request";
/// Room for ORAO's largest pending request: client, seed and 7 responses.
const PENDING_SIZE: usize = 1 + 32 + 32 + 4 + (32 + 64) * 7;
const NETWORK_STATE_SIZE: usize = 32 + 32 + 8 + 4 + 1 + 8;

#[program]
pub mod mock_orao {
    use super::*;

    pub fn init_network(ctx: Context<InitNetwork>, treasury: Pubkey) -> Result<()> {
        ctx.accounts.network_state.config = NetworkConfiguration {
            authority: ctx.accounts.payer.key(),
            treasury,
            request_fee: 0,
            fulfillment_authorities: Vec::new(),
            token_fee_config: None,
        };
        Ok(())
    }

    pub fn request_v2(ctx: Context<RequestV2>, seed: [u8; 32]) -> Result<()> {
        ctx.accounts.request.request = RequestAccount::Pending(PendingRequest {
            client: ctx.accounts.payer.key(),
            seed,
            responses: Vec::new(),
        });
        ctx.accounts.network_state.num_received += 1;
        Ok(())
    }

    pub fn mock_fulfill(ctx: Context<MockFulfill>, randomness: [u8; 64]) -> Result<()> {
        let (client, seed) = match &ctx.accounts.request.request {
            RequestAccount::Pending(p) => (p.client, p.seed),
            RequestAccount::Fulfilled(_) => return err!(MockError::AlreadyFulfilled),
        };
        ctx.accounts.request.request = RequestAccount::Fulfilled(FulfilledRequest {
            client,
            seed,
            randomness,
        });
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct NetworkConfiguration {
    pub authority: Pubkey,
    pub treasury: Pubkey,
    pub request_fee: u64,
    pub fulfillment_authorities: Vec<Pubkey>,
    pub token_fee_config: Option<OraoTokenFeeConfig>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct OraoTokenFeeConfig {
    pub mint: Pubkey,
    pub treasury: Pubkey,
    pub fee: u64,
}

#[account]
pub struct NetworkState {
    pub config: NetworkConfiguration,
    pub num_received: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RandomnessResponse {
    pub pubkey: Pubkey,
    pub randomness: [u8; 64],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PendingRequest {
    pub client: Pubkey,
    pub seed: [u8; 32],
    pub responses: Vec<RandomnessResponse>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct FulfilledRequest {
    pub client: Pubkey,
    pub seed: [u8; 32],
    pub randomness: [u8; 64],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum RequestAccount {
    Pending(PendingRequest),
    Fulfilled(FulfilledRequest),
}

#[account]
pub struct RandomnessV2 {
    pub request: RequestAccount,
}

#[derive(Accounts)]
pub struct InitNetwork<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(init, payer = payer, space = 8 + NETWORK_STATE_SIZE, seeds = [CONFIG_ACCOUNT_SEED], bump)]
    pub network_state: Account<'info, NetworkState>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(seed: [u8; 32])]
pub struct RequestV2<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, seeds = [CONFIG_ACCOUNT_SEED], bump)]
    pub network_state: Account<'info, NetworkState>,
    /// CHECK: must be the configured treasury, as on ORAO.
    #[account(mut, address = network_state.config.treasury @ MockError::UnknownTreasury)]
    pub treasury: UncheckedAccount<'info>,
    #[account(init, payer = payer, space = 8 + PENDING_SIZE, seeds = [RANDOMNESS_ACCOUNT_SEED, &seed], bump)]
    pub request: Account<'info, RandomnessV2>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MockFulfill<'info> {
    pub authority: Signer<'info>,
    #[account(
        seeds = [CONFIG_ACCOUNT_SEED],
        bump,
        constraint = network_state.config.authority == authority.key() @ MockError::NotAuthority
    )]
    pub network_state: Account<'info, NetworkState>,
    #[account(mut)]
    pub request: Account<'info, RandomnessV2>,
}

#[error_code]
pub enum MockError {
    #[msg("Treasury is not the configured one")]
    UnknownTreasury,
    #[msg("Only the network authority can fulfill")]
    NotAuthority,
    #[msg("Request is already fulfilled")]
    AlreadyFulfilled,
}
