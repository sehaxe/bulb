#[derive(Debug, Clone, Default)]
pub struct InstallScript {
    pub has_pre_install: bool,
    pub has_post_install: bool,
    pub has_pre_upgrade: bool,
    pub has_post_upgrade: bool,
    pub has_pre_remove: bool,
    pub post_remove: bool,
    pub raw: String,
}

impl InstallScript {
    pub fn parse(text: &str) -> Option<Self> {
        if text.trim().is_empty() {
            return None;
        }

        let has_pre_install = text.contains("pre_install()");
        let has_post_install = text.contains("post_install()");
        let has_pre_upgrade = text.contains("pre_upgrade()");
        let has_post_upgrade = text.contains("post_upgrade()");
        let has_pre_remove = text.contains("pre_remove()");
        let post_remove = text.contains("post_remove()");

        Some(InstallScript {
            has_pre_install,
            has_post_install,
            has_pre_upgrade,
            has_post_upgrade,
            has_pre_remove,
            post_remove,
            raw: text.to_string(),
        })
    }

    pub fn function_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.has_pre_install {
            names.push("pre_install");
        }
        if self.has_post_install {
            names.push("post_install");
        }
        if self.has_pre_upgrade {
            names.push("pre_upgrade");
        }
        if self.has_post_upgrade {
            names.push("post_upgrade");
        }
        if self.has_pre_remove {
            names.push("pre_remove");
        }
        if self.post_remove {
            names.push("post_remove");
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"
pre_install() {
    echo "Installing..."
}

post_install() {
    echo "Done"
}
"#;

    #[test]
    fn detects_functions() {
        let s = InstallScript::parse(SCRIPT).unwrap();
        assert!(s.has_pre_install);
        assert!(s.has_post_install);
        assert!(!s.has_pre_upgrade);
        assert!(!s.has_post_upgrade);
        assert!(!s.has_pre_remove);
        assert!(!s.post_remove);
        assert_eq!(s.function_names(), vec!["pre_install", "post_install"]);
    }

    #[test]
    fn empty_script_is_none() {
        assert!(InstallScript::parse("").is_none());
        assert!(InstallScript::parse("  \n  ").is_none());
    }
}
