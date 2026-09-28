//! Reads Switchboard On-Demand `RandomnessAccountData` without pulling in the
//! Switchboard crate. Layout taken from the on-chain IDL:
//! disc(8) authority(32) queue(32) seed_slothash(32) seed_slot(u64)
//! oracle(32) reveal_slot(u64) value([u8; 32]) ...
use crate::error::SauceError;
use anchor_lang::prelude::*;

#[cfg(feature = "devnet")]
pub const SWITCHBOARD_PROGRAM_ID: Pubkey = pubkey!("Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2");
#[cfg(feature = "devnet")]
pub const SWITCHBOARD_QUEUE: Pubkey = pubkey!("EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7");

#[cfg(not(feature = "devnet"))]
pub const SWITCHBOARD_PROGRAM_ID: Pubkey = pubkey!("SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv");
#[cfg(not(feature = "devnet"))]
pub const SWITCHBOARD_QUEUE: Pubkey = pubkey!("A43DyUGA7s8eXPxqEjJY6EBu1KKbNgfxF8h17VAHn13w");

pub const RANDOMNESS_DISCRIMINATOR: [u8; 8] = [10, 66, 229, 135, 220, 239, 217, 114];

const QUEUE_OFFSET: usize = 40;
const SEED_SLOT_OFFSET: usize = 104;
const REVEAL_SLOT_OFFSET: usize = 144;
const VALUE_OFFSET: usize = 152;
const MIN_LEN: usize = VALUE_OFFSET + 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Randomness {
    pub queue: Pubkey,
    pub seed_slot: u64,
    pub reveal_slot: u64,
    pub value: [u8; 32],
}

impl Randomness {
    pub fn parse(data: &[u8]) -> Result<Self> {
        require!(data.len() >= MIN_LEN, SauceError::BadRandomnessAccount);
        require!(
            data[..8] == RANDOMNESS_DISCRIMINATOR,
            SauceError::BadRandomnessAccount
        );
        let u64_at = |o: usize| u64::from_le_bytes(data[o..o + 8].try_into().unwrap());
        let mut value = [0u8; 32];
        value.copy_from_slice(&data[VALUE_OFFSET..VALUE_OFFSET + 32]);
        Ok(Self {
            queue: Pubkey::try_from(&data[QUEUE_OFFSET..QUEUE_OFFSET + 32]).unwrap(),
            seed_slot: u64_at(SEED_SLOT_OFFSET),
            reveal_slot: u64_at(REVEAL_SLOT_OFFSET),
            value,
        })
    }

    /// Owner, discriminator and queue must all be the official Switchboard ones;
    /// a custom queue would let its operator run an oracle that signs any value.
    pub fn load(info: &AccountInfo) -> Result<Self> {
        require_keys_eq!(
            *info.owner,
            SWITCHBOARD_PROGRAM_ID,
            SauceError::BadRandomnessAccount
        );
        let parsed = Self::parse(&info.try_borrow_data()?)?;
        require_keys_eq!(parsed.queue, SWITCHBOARD_QUEUE, SauceError::BadRandomnessAccount);
        Ok(parsed)
    }

    /// `request_draw` must share a transaction with the commit, so the seed is
    /// the previous slot's hash and nobody has seen the value yet.
    pub fn require_fresh_commit(&self, current_slot: u64) -> Result<()> {
        require!(
            self.seed_slot != 0 && self.seed_slot == current_slot.saturating_sub(1),
            SauceError::RandomnessStale
        );
        require!(self.reveal_slot < self.seed_slot, SauceError::RandomnessStale);
        Ok(())
    }

    /// Mirrors Switchboard's `get_value`: the value is only trusted in the slot
    /// it was revealed, and only for the commitment recorded at request time.
    pub fn revealed_value(&self, commit_slot: u64, current_slot: u64) -> Result<u128> {
        require!(self.seed_slot == commit_slot, SauceError::RandomnessStale);
        require!(
            self.reveal_slot > self.seed_slot && self.reveal_slot == current_slot,
            SauceError::RandomnessUnready
        );
        require!(self.value != [0u8; 32], SauceError::RandomnessUnready);
        Ok(u128::from_le_bytes(self.value[..16].try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(seed_slot: u64, reveal_slot: u64, value: [u8; 32]) -> Vec<u8> {
        let mut data = vec![0u8; 480];
        data[..8].copy_from_slice(&RANDOMNESS_DISCRIMINATOR);
        data[QUEUE_OFFSET..QUEUE_OFFSET + 32].copy_from_slice(SWITCHBOARD_QUEUE.as_ref());
        data[SEED_SLOT_OFFSET..SEED_SLOT_OFFSET + 8].copy_from_slice(&seed_slot.to_le_bytes());
        data[REVEAL_SLOT_OFFSET..REVEAL_SLOT_OFFSET + 8]
            .copy_from_slice(&reveal_slot.to_le_bytes());
        data[VALUE_OFFSET..VALUE_OFFSET + 32].copy_from_slice(&value);
        data
    }

    #[test]
    fn parses_fields_at_idl_offsets() {
        let r = Randomness::parse(&account(100, 105, [7u8; 32])).unwrap();
        assert_eq!(r.queue, SWITCHBOARD_QUEUE);
        assert_eq!(r.seed_slot, 100);
        assert_eq!(r.reveal_slot, 105);
        assert_eq!(r.value, [7u8; 32]);
    }

    #[test]
    fn rejects_wrong_discriminator_and_short_data() {
        let mut data = account(1, 0, [0; 32]);
        data[0] ^= 1;
        assert!(Randomness::parse(&data).is_err());
        assert!(Randomness::parse(&[0u8; 100]).is_err());
    }

    #[test]
    fn commit_must_be_from_previous_slot() {
        let r = Randomness::parse(&account(99, 0, [0; 32])).unwrap();
        assert!(r.require_fresh_commit(100).is_ok());
        assert!(r.require_fresh_commit(101).is_err());
        assert!(r.require_fresh_commit(99).is_err());
    }

    #[test]
    fn already_revealed_commit_is_rejected() {
        let r = Randomness::parse(&account(99, 99, [1; 32])).unwrap();
        assert!(r.require_fresh_commit(100).is_err());
    }

    #[test]
    fn value_only_readable_in_reveal_slot_for_recorded_commit() {
        let mut value = [0u8; 32];
        value[0] = 42;
        let r = Randomness::parse(&account(99, 103, value)).unwrap();
        assert_eq!(r.revealed_value(99, 103).unwrap(), 42);
        assert!(r.revealed_value(99, 104).is_err(), "stale reveal");
        assert!(r.revealed_value(98, 103).is_err(), "re-committed seed");
        let unrevealed = Randomness::parse(&account(99, 0, [0; 32])).unwrap();
        assert!(unrevealed.revealed_value(99, 100).is_err());
    }
}
