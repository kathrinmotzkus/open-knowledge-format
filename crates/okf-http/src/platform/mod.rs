use std::path::PathBuf;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use fallback as platform_impl;
#[cfg(target_os = "linux")]
use linux as platform_impl;
#[cfg(target_os = "macos")]
use macos as platform_impl;

#[derive(Clone, Debug, Eq, PartialEq)]
struct OkfPlatformPaths {
    private_state_root: Option<PathBuf>,
}

impl OkfPlatformPaths {
    fn private_state_root_or_error(&self) -> Result<PathBuf, String> {
        self.private_state_root.clone().ok_or_else(|| {
            "cannot determine private state directory: set XDG_STATE_HOME or HOME".to_string()
        })
    }

    fn okf_state_dir(&self) -> Result<PathBuf, String> {
        Ok(self.private_state_root_or_error()?.join("okf"))
    }

    fn okf_tls_dir(&self) -> Result<PathBuf, String> {
        Ok(self.okf_state_dir()?.join("tls"))
    }

    fn okf_auth_database(&self) -> Result<PathBuf, String> {
        Ok(self.okf_state_dir()?.join("auth.sqlite"))
    }
}

pub fn default_browser_root() -> PathBuf {
    platform_impl::default_browser_root()
}

pub fn private_state_root() -> Option<PathBuf> {
    platform_impl::private_state_root()
}

pub fn runtime_root() -> PathBuf {
    platform_impl::runtime_root()
}

pub fn tls_dir() -> Result<PathBuf, String> {
    platform_impl::paths().okf_tls_dir()
}

pub fn auth_database() -> Result<PathBuf, String> {
    platform_impl::paths().okf_auth_database()
}
