//! ORAO VRF randomness without pulling in the ORAO crate, which needs a newer
//! Anchor. A request account (`RandomnessV2`) is laid out as:
//! disc(8) state(1: 0 pending, 1 fulfilled) client(32) seed(32) randomness([u8; 64])
//! The randomness is only written once a quorum of ORAO's fulfilment
//! authorities has signed the seed, so nobody, including the keeper, can know
//! it when the draw is requested.
use crate::error::SauceError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke;

/// Same program id on devnet and mainnet. Localnet loads `mock_orao` here.
pub const ORAO_VRF_PROGRAM_ID: Pubkey = pubkey!("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");
/// ORAO's network configuration PDA (`[b"orao-vrf-network-configuration"]`).
pub const ORAO_NETWORK_STATE: Pubkey = pubkey!("5ER1oENnV4srxYdAynUfRzWeQCPQaqMiAp4VqyMbSqnK");
pub const NETWORK_STATE_SEED: &[u8] = b"orao-vrf-network-configuration";
pub const REQUEST_SEED: &[u8] = b"orao-vrf-randomness-request";

pub const RANDOMNESS_V2_DISCRIMINATOR: [u8; 8] = [0x8b, 0xef, 0xb8, 0xd7, 0xe3, 0x56, 0xbf, 0xe2];
const REQUEST_V2_DISCRIMINATOR: [u8; 8] = [0x26, 0x97, 0xd1, 0x06, 0xc3, 0x66, 0x1c, 0xd9];

const STATE_OFFSET: usize = 8;
const SEED_OFFSET: usize = 41;
const VALUE_OFFSET: usize = 73;
const FULFILLED_LEN: usize = VALUE_OFFSET + 64;
const FULFILLED: u8 = 1;

/// Binds a request to one jar and round. ORAO creates each seed's account
/// once, so a seed whose value is already known can never be reused.
pub fn draw_seed(jar: &Pubkey, round_index: u64, nonce: u64) -> [u8; 32] {
    hashv(&[
        b"stonksauce-draw",
        jar.as_ref(),
        &round_index.to_le_bytes(),
        &nonce.to_le_bytes(),
    ])
    .to_bytes()
}

pub fn request_address(seed: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(&[REQUEST_SEED, seed], &ORAO_VRF_PROGRAM_ID).0
}

/// CPI into ORAO `request_v2`. `payer` pays the request fee and the request
/// account's rent; ORAO checks the treasury against its network state.
pub fn request<'info>(
    vrf_program: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    network_state: &AccountInfo<'info>,
    treasury: &AccountInfo<'info>,
    request: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    seed: [u8; 32],
) -> Result<()> {
    let ix = Instruction {
        program_id: ORAO_VRF_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(payer.key(), true),
            AccountMeta::new(network_state.key(), false),
            AccountMeta::new(treasury.key(), false),
            AccountMeta::new(request.key(), false),
            AccountMeta::new_readonly(system_program.key(), false),
        ],
        data: [REQUEST_V2_DISCRIMINATOR.as_slice(), seed.as_slice()].concat(),
    };
    invoke(
        &ix,
        &[
            payer.clone(),
            network_state.clone(),
            treasury.clone(),
            request.clone(),
            system_program.clone(),
            vrf_program.clone(),
        ],
    )?;
    Ok(())
}

/// First 16 bytes of a fulfilled request for `seed`, little-endian.
pub fn fulfilled_value(info: &AccountInfo, seed: &[u8; 32]) -> Result<u128> {
    require_keys_eq!(
        *info.owner,
        ORAO_VRF_PROGRAM_ID,
        SauceError::BadRandomnessAccount
    );
    parse_fulfilled(&info.try_borrow_data()?, seed)
}

/// Whether `info` holds a usable value for `seed`, i.e. `fulfill_draw` would accept it.
pub fn is_fulfilled(info: &AccountInfo, seed: &[u8; 32]) -> Result<bool> {
    if *info.owner != ORAO_VRF_PROGRAM_ID {
        return Ok(false);
    }
    Ok(parse_fulfilled(&info.try_borrow_data()?, seed).is_ok())
}

pub fn parse_fulfilled(data: &[u8], seed: &[u8; 32]) -> Result<u128> {
    require!(
        data.len() >= SEED_OFFSET + 32 && data[..8] == RANDOMNESS_V2_DISCRIMINATOR,
        SauceError::BadRandomnessAccount
    );
    require!(
        data[SEED_OFFSET..SEED_OFFSET + 32] == seed[..],
        SauceError::RandomnessStale
    );
    require!(
        data[STATE_OFFSET] == FULFILLED && data.len() >= FULFILLED_LEN,
        SauceError::RandomnessUnready
    );
    let value = &data[VALUE_OFFSET..FULFILLED_LEN];
    require!(value.iter().any(|b| *b != 0), SauceError::RandomnessUnready);
    Ok(u128::from_le_bytes(value[..16].try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::hash::hash;

    fn account(state: u8, seed: [u8; 32], value: [u8; 64]) -> Vec<u8> {
        let mut data = RANDOMNESS_V2_DISCRIMINATOR.to_vec();
        data.push(state);
        data.extend_from_slice(Pubkey::new_unique().as_ref());
        data.extend_from_slice(&seed);
        data.extend_from_slice(&value);
        data
    }

    #[test]
    fn discriminators_and_network_state_match_orao() {
        assert_eq!(
            hash(b"account:RandomnessV2").to_bytes()[..8],
            RANDOMNESS_V2_DISCRIMINATOR
        );
        assert_eq!(
            hash(b"global:request_v2").to_bytes()[..8],
            REQUEST_V2_DISCRIMINATOR
        );
        let (pda, _) = Pubkey::find_program_address(&[NETWORK_STATE_SEED], &ORAO_VRF_PROGRAM_ID);
        assert_eq!(pda, ORAO_NETWORK_STATE);
    }

    #[test]
    fn reads_the_first_16_bytes_of_a_fulfilled_request() {
        let seed = [3u8; 32];
        let mut value = [0u8; 64];
        value[0] = 42;
        value[16] = 0xff;
        assert_eq!(parse_fulfilled(&account(1, seed, value), &seed).unwrap(), 42);
    }

    #[test]
    fn pending_request_is_not_ready() {
        let seed = [3u8; 32];
        // A pending request is larger and holds responses where the value would be.
        let mut data = account(0, seed, [9u8; 64]);
        data.extend_from_slice(&[9u8; 600]);
        assert!(parse_fulfilled(&data, &seed).is_err());
        assert!(parse_fulfilled(&account(1, seed, [0u8; 64]), &seed).is_err());
    }

    #[test]
    fn other_seed_or_account_type_is_rejected() {
        let seed = [3u8; 32];
        assert!(parse_fulfilled(&account(1, [4u8; 32], [1u8; 64]), &seed).is_err());
        let mut data = account(1, seed, [1u8; 64]);
        data[0] ^= 1;
        assert!(parse_fulfilled(&data, &seed).is_err());
        assert!(parse_fulfilled(&data[..40], &seed).is_err());
    }

    #[test]
    fn only_an_answered_orao_request_counts_as_fulfilled() {
        let seed = [3u8; 32];
        let key = Pubkey::new_unique();
        let check = |owner: Pubkey, mut data: Vec<u8>| {
            let mut lamports = 1;
            let info = AccountInfo::new(&key, false, false, &mut lamports, &mut data, &owner, false, 0);
            is_fulfilled(&info, &seed).unwrap()
        };
        assert!(check(ORAO_VRF_PROGRAM_ID, account(1, seed, [1u8; 64])));
        assert!(!check(ORAO_VRF_PROGRAM_ID, account(0, seed, [1u8; 64])));
        assert!(!check(ORAO_VRF_PROGRAM_ID, account(1, [4u8; 32], [1u8; 64])));
        assert!(!check(Pubkey::new_unique(), account(1, seed, [1u8; 64])));
    }

    #[test]
    fn seeds_differ_per_jar_round_and_nonce() {
        let jar = Pubkey::new_unique();
        let base = draw_seed(&jar, 1, 7);
        assert_ne!(base, draw_seed(&Pubkey::new_unique(), 1, 7));
        assert_ne!(base, draw_seed(&jar, 2, 7));
        assert_ne!(base, draw_seed(&jar, 1, 8));
        assert_eq!(base, draw_seed(&jar, 1, 7));
    }
}
