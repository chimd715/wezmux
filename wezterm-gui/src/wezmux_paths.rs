use std::path::{Path, PathBuf};

pub fn integration_bin(exe: &Path) -> Option<PathBuf> {
    exe.parent()?
        .ancestors()
        .map(|dir| dir.join("bin"))
        .find(|bin| bin.join("claude").is_file() && bin.join("hooks/on-prompt-submit.sh").is_file())
}

pub fn should_seed_config(home: &Path, config_dirs: &[PathBuf]) -> bool {
    ![".wezmux.lua", ".wezterm.lua"]
        .iter()
        .any(|name| home.join(name).exists())
        && !config_dirs.iter().any(|dir| {
            ["wezmux.lua", "wezterm.lua"]
                .iter()
                .any(|name| dir.join(name).exists())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let base = std::env::var_os("TMPDIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let path = base.join(format!(
                "wezmux-paths-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn wrapper(&self, dir: &Path) {
            fs::create_dir_all(dir.join("hooks")).unwrap();
            fs::write(dir.join("claude"), "wrapper").unwrap();
            fs::write(dir.join("hooks/on-prompt-submit.sh"), "hook").unwrap();
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_installed_and_development_wrappers() {
        let scratch = Scratch::new();
        let installed = scratch.0.join("lib/wezmux");
        scratch.wrapper(&installed.join("bin"));
        assert_eq!(
            integration_bin(&installed.join("wezterm-gui")),
            Some(installed.join("bin"))
        );
        let repo = scratch.0.join("repo");
        scratch.wrapper(&repo.join("bin"));
        assert_eq!(
            integration_bin(&repo.join("target/debug/wezterm-gui")),
            Some(repo.join("bin"))
        );
    }

    #[test]
    fn does_not_mistake_real_claude_for_bundled_wrappers() {
        let scratch = Scratch::new();
        fs::create_dir_all(scratch.0.join("bin")).unwrap();
        fs::write(scratch.0.join("bin/claude"), "real executable").unwrap();
        assert_eq!(integration_bin(&scratch.0.join("bin/wezterm-gui")), None);
    }

    #[test]
    fn keeps_existing_legacy_and_xdg_configs() {
        let scratch = Scratch::new();
        assert!(should_seed_config(&scratch.0, &[]));
        fs::write(scratch.0.join(".wezterm.lua"), "user config").unwrap();
        assert!(!should_seed_config(&scratch.0, &[]));
        fs::remove_file(scratch.0.join(".wezterm.lua")).unwrap();
        let xdg = scratch.0.join("xdg");
        fs::create_dir_all(&xdg).unwrap();
        fs::write(xdg.join("wezmux.lua"), "user config").unwrap();
        assert!(!should_seed_config(&scratch.0, &[xdg]));
    }
}
