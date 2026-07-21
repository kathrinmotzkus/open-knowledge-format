use std::env;
use std::path::PathBuf;

use super::OkfPlatformPaths;

pub(super) fn paths() -> OkfPlatformPaths {
    OkfPlatformPaths {
        private_state_root: private_state_root(),
    }
}

pub(super) fn default_browser_root() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("docs-browser")
}

pub(super) fn private_state_root() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
}

pub(super) fn runtime_root() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_state_home_takes_precedence() {
        let temp = env::temp_dir().join(format!("okf-platform-linux-{}", std::process::id()));
        env::set_var("XDG_STATE_HOME", &temp);
        env::set_var("HOME", temp.join("home"));
        assert_eq!(private_state_root(), Some(temp));
        env::remove_var("XDG_STATE_HOME");
    }
}
