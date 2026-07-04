use gpui_shared_string::SharedString;

#[derive(Clone)]
pub struct EnvVar {
    pub name: SharedString,
    /// Value of the environment variable. Also `None` when set to an empty string.
    pub value: Option<String>,
}

impl EnvVar {
    pub fn new(name: SharedString) -> Self {
        let value = std::env::var(name.as_str()).ok();
        if value.as_ref().is_some_and(|v| v.is_empty()) {
            Self { name, value: None }
        } else {
            Self { name, value }
        }
    }

    pub fn or(self, other: EnvVar) -> EnvVar {
        if self.value.is_some() { self } else { other }
    }
}

/// Creates a `LazyLock<EnvVar>` expression for use in a `static` declaration.
#[macro_export]
macro_rules! env_var {
    ($name:expr) => {
        ::std::sync::LazyLock::new(|| $crate::EnvVar::new(($name).into()))
    };
}

/// Generates a `LazyLock<bool>` expression for use in a `static` declaration. Checks if the
/// environment variable exists and is non-empty.
#[macro_export]
macro_rules! bool_env_var {
    ($name:expr) => {
        ::std::sync::LazyLock::new(|| $crate::EnvVar::new(($name).into()).value.is_some())
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        name: String,
        original: Option<String>,
    }

    impl EnvRestore {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                original: std::env::var(name).ok(),
            }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match self.original.as_deref() {
                Some(value) => unsafe { std::env::set_var(&self.name, value) },
                None => unsafe { std::env::remove_var(&self.name) },
            }
        }
    }

    #[test]
    fn reads_present_non_empty_values() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _restore = EnvRestore::new("ZZZ_ENV_VAR_PRESENT");
        unsafe { std::env::set_var("ZZZ_ENV_VAR_PRESENT", "enabled") };

        let env_var = EnvVar::new("ZZZ_ENV_VAR_PRESENT".into());

        assert_eq!(env_var.name, "ZZZ_ENV_VAR_PRESENT");
        assert_eq!(env_var.value.as_deref(), Some("enabled"));
    }

    #[test]
    fn normalizes_empty_values_to_none() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _restore = EnvRestore::new("ZZZ_ENV_VAR_EMPTY");
        unsafe { std::env::set_var("ZZZ_ENV_VAR_EMPTY", "") };

        let env_var = EnvVar::new("ZZZ_ENV_VAR_EMPTY".into());

        assert_eq!(env_var.value, None);
    }

    #[test]
    fn or_prefers_the_first_present_value() {
        let primary = EnvVar {
            name: "PRIMARY".into(),
            value: Some("first".to_string()),
        };
        let fallback = EnvVar {
            name: "FALLBACK".into(),
            value: Some("second".to_string()),
        };

        assert_eq!(
            primary.clone().or(fallback.clone()).value.as_deref(),
            Some("first")
        );
        assert_eq!(
            EnvVar {
                value: None,
                ..primary
            }
            .or(fallback)
            .value
            .as_deref(),
            Some("second")
        );
    }
}
