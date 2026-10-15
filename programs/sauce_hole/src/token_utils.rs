use crate::error::HoleError;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};
use spl_token_2022::extension::{BaseStateWithExtensions, ExtensionType, PodStateWithExtensions};
use spl_token_2022::pod::PodMint;

/// Token-2022 extensions that would let a third party freeze, seize or block
/// tokens held by the hole, which would break the "locked forever" promise.
pub fn is_forbidden_extension(ext: ExtensionType) -> bool {
    matches!(
        ext,
        ExtensionType::TransferHook
            | ExtensionType::PermanentDelegate
            | ExtensionType::NonTransferable
            | ExtensionType::Pausable
            | ExtensionType::DefaultAccountState
    )
}

/// Rejects a mint with a freeze authority on either token program, or a
/// Token-2022 mint with a forbidden extension.
pub fn require_safe_mint(mint: &InterfaceAccount<Mint>) -> Result<()> {
    require!(mint.freeze_authority.is_none(), HoleError::UnsupportedMint);
    let mint_info = mint.to_account_info();
    if *mint_info.owner != spl_token_2022::id() {
        return Ok(());
    }
    let data = mint_info.try_borrow_data()?;
    let mint = PodStateWithExtensions::<PodMint>::unpack(&data)
        .map_err(|_| error!(HoleError::UnsupportedMint))?;
    let types = mint
        .get_extension_types()
        .map_err(|_| error!(HoleError::UnsupportedMint))?;
    require!(
        !types.into_iter().any(is_forbidden_extension),
        HoleError::UnsupportedMint
    );
    Ok(())
}

/// A delegate, close authority or new owner left on a vault would let
/// someone drain it in a later transaction.
pub fn require_untouched(vault: &InterfaceAccount<TokenAccount>, hole: &Pubkey) -> Result<()> {
    require_keys_eq!(vault.owner, *hole, HoleError::SwapViolation);
    require!(vault.delegate.is_none(), HoleError::SwapViolation);
    require!(vault.close_authority.is_none(), HoleError::SwapViolation);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seizing_extensions_are_forbidden() {
        for ext in [
            ExtensionType::TransferHook,
            ExtensionType::PermanentDelegate,
            ExtensionType::NonTransferable,
            ExtensionType::Pausable,
            ExtensionType::DefaultAccountState,
        ] {
            assert!(is_forbidden_extension(ext), "{ext:?}");
        }
    }

    #[test]
    fn stonksauce_extensions_are_allowed() {
        for ext in [
            ExtensionType::TransferFeeConfig,
            ExtensionType::MetadataPointer,
            ExtensionType::TokenMetadata,
        ] {
            assert!(!is_forbidden_extension(ext), "{ext:?}");
        }
    }
}
