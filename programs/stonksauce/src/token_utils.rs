use crate::error::SauceError;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use spl_token_2022::extension::{
    transfer_fee::TransferFeeConfig, BaseStateWithExtensions, ExtensionType,
    PodStateWithExtensions,
};
use spl_token_2022::pod::PodMint;

pub fn require_transfer_fee_mint(mint_info: &AccountInfo) -> Result<()> {
    require_keys_eq!(
        *mint_info.owner,
        spl_token_2022::id(),
        SauceError::NotRewardMint
    );
    let data = mint_info.try_borrow_data()?;
    let mint = PodStateWithExtensions::<PodMint>::unpack(&data)
        .map_err(|_| error!(SauceError::NotRewardMint))?;
    mint.get_extension::<TransferFeeConfig>()
        .map_err(|_| error!(SauceError::NotRewardMint))?;
    Ok(())
}

/// Rejects mints that let a third party block transfers out of a jar vault or
/// move tokens out of it: a freeze authority on either token program, or one
/// of the Token-2022 extensions below.
pub fn require_safe_mint(mint: &InterfaceAccount<Mint>) -> Result<()> {
    require!(mint.freeze_authority.is_none(), SauceError::UnsupportedMint);
    let mint_info = mint.to_account_info();
    if *mint_info.owner != spl_token_2022::id() {
        return Ok(());
    }
    let data = mint_info.try_borrow_data()?;
    let mint = PodStateWithExtensions::<PodMint>::unpack(&data)
        .map_err(|_| error!(SauceError::UnsupportedMint))?;
    let types = mint
        .get_extension_types()
        .map_err(|_| error!(SauceError::UnsupportedMint))?;
    for ext in types {
        if matches!(
            ext,
            ExtensionType::TransferHook
                | ExtensionType::PermanentDelegate
                | ExtensionType::NonTransferable
                | ExtensionType::Pausable
                | ExtensionType::DefaultAccountState
        ) {
            return err!(SauceError::UnsupportedMint);
        }
    }
    Ok(())
}
