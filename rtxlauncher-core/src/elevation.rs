use anyhow::Result;

#[cfg(windows)]
mod imp {
    use super::*;
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
        Err(anyhow::anyhow!("relaunch not implemented in core; UI should ShellExecuteW with runas"))
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    pub fn is_elevated() -> bool {
        // On Unix, consider root as elevated
        nix::unistd::Uid::effective().is_root()
    }
    pub fn relaunch_as_admin() -> Result<()> {
        // Leave elevation relaunch to UI layer (e.g., pkexec), keep core simple
        Err(anyhow::anyhow!("relaunch not implemented in core; UI should call pkexec/sudo"))
    }
}

pub use imp::{is_elevated, relaunch_as_admin};


