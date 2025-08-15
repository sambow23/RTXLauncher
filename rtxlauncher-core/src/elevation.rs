use anyhow::Result;
use windows::Win32::{
    Foundation::HANDLE,
    Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len = 0u32;
        // windows-rs expects raw pointers inside Option
        let ret_len_ptr: *mut u32 = &mut ret_len as *mut u32;
        if GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            ret_len_ptr,
        ).is_err() {
            return false;
        }
        elevation.TokenIsElevated != 0
    }
}

pub fn relaunch_as_admin() -> Result<()> {
    // Lightweight placeholder: instruct UI layer to relaunch via ShellExecuteW using windows-rs if needed.
    // For now we return an error to be replaced with a UI-side implementation.
    Err(anyhow::anyhow!("relaunch not implemented in core; use UI layer to ShellExecute runas"))
}


