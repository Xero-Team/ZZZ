//! Declared, per-agent metadata: environment injection, client capability meta
//! and auth methods.
//!
//! The agent layer is ACP-only, so a provider quirk belongs here as data, not as
//! a `match` on the agent id at the call site. A profile records what an agent is
//! *known* to need; everything an agent *reports* is read from its `initialize`
//! result instead, so a third-party agent degrades through capabilities rather
//! than inheriting a patch meant for one of the four below.
//!
//! Adding an agent id comparison outside this table is the thing this module
//! exists to prevent: add an entry here, and let the call site read it.

use agent_client_protocol::v1::schema as acp;
use project::agent_server_store::{AgentId, AgentServerCommand};

use crate::{CLAUDE_AGENT_ID, CODEX_ID, CURSOR_ID, GEMINI_ID};

/// The auth method id ZZZ synthesizes for Gemini CLI's `/auth` command.
///
/// Gemini CLI does not report an ACP auth method yet, so [`AuthMethodSpec::terminal`]
/// says to reuse its launch command with [`TerminalAuthSpec::strip_args`] removed.
pub const GEMINI_TERMINAL_AUTH_METHOD_ID: &str = "spawn-gemini-cli";

/// The client capability meta key Cursor reads to show its model picker.
pub const PARAMETERIZED_MODEL_PICKER_META_KEY: &str = "parameterizedModelPicker";

/// Where the value for an injected environment variable comes from.
pub enum EnvSource {
    /// Insert the variable with an empty value, so an agent that reads it sees a
    /// set-but-empty key instead of falling back to an interactive prompt.
    Empty,
    /// Forward this host environment variable when it is set.
    HostEnv(&'static str),
    /// Insert this literal value.
    Literal(&'static str),
    /// Resolve a stored credential and inject it.
    Credential(&'static CredentialSpec),
}

/// Where a credential an agent reads from its environment is stored.
pub struct CredentialSpec {
    /// Host environment variables checked first, in order.
    pub env_vars: &'static [&'static str],
    /// Keychain entry URL holding the key, used when no environment variable is set.
    pub keychain_url: &'static str,
}

/// How ZZZ turns an agent's launch command into a login command.
pub struct TerminalAuthSpec {
    /// Label of the terminal tab and of the auth picker entry.
    pub label: &'static str,
    /// Arguments that only make sense for the ACP session and that the login
    /// subcommand rejects.
    pub strip_args: &'static [&'static str],
}

/// What ZZZ knows about one auth method an agent exposes.
pub struct AuthMethodSpec {
    /// The ACP auth method id, e.g. `claude-login`.
    pub id: &'static str,
    /// Name shown in the auth picker when ZZZ synthesizes the method.
    pub label: &'static str,
    /// Description shown in the auth picker when ZZZ synthesizes the method.
    pub description: &'static str,
    /// `Some` when ZZZ synthesizes the method by reusing the launch command,
    /// `None` when the agent reports the method itself and only the fields below
    /// apply.
    pub terminal: Option<TerminalAuthSpec>,
    /// Terminal output that means the login succeeded while the command keeps
    /// running. Empty means the exit status is the only signal.
    pub success_patterns: &'static [&'static str],
    /// Whether a login that exits without a success pattern should be retried
    /// locally when the project runs on a remote server.
    pub retry_on_remote: bool,
}

/// Everything ZZZ declares for one agent id.
///
/// An agent with no entry gets the spec-conformant defaults: the shared client
/// capabilities, no injected environment, and whatever auth methods it reports.
pub struct AgentProfile {
    /// The agent id this profile applies to.
    pub id: &'static str,
    /// Environment variables injected when ZZZ launches the agent.
    pub env: &'static [(&'static str, EnvSource)],
    /// Boolean client capability meta flags sent in `initialize`.
    pub client_meta: &'static [(&'static str, bool)],
    /// Auth methods ZZZ knows about for this agent.
    pub auth_methods: &'static [AuthMethodSpec],
}

/// Every agent ZZZ ships metadata for.
///
/// This table is the only place that names an agent id to decide behavior. It
/// shrinks as agents start reporting the same information themselves.
pub const PROFILES: &[AgentProfile] = &[
    AgentProfile {
        id: CLAUDE_AGENT_ID,
        env: &[("ANTHROPIC_API_KEY", EnvSource::Empty)],
        client_meta: &[],
        auth_methods: &[AuthMethodSpec {
            id: "claude-login",
            label: "Login",
            description: "Login with your Anthropic account",
            terminal: None,
            success_patterns: &["Login successful", "Type your message"],
            retry_on_remote: false,
        }],
    },
    AgentProfile {
        id: CODEX_ID,
        env: &[
            ("CODEX_API_KEY", EnvSource::HostEnv("CODEX_API_KEY")),
            ("OPEN_AI_API_KEY", EnvSource::HostEnv("OPEN_AI_API_KEY")),
        ],
        client_meta: &[],
        auth_methods: &[],
    },
    AgentProfile {
        id: GEMINI_ID,
        env: &[
            ("SURFACE", EnvSource::Literal("zzz")),
            (
                "GEMINI_API_KEY",
                EnvSource::Credential(&CredentialSpec {
                    env_vars: &["GEMINI_API_KEY", "GOOGLE_AI_API_KEY"],
                    keychain_url: "https://generativelanguage.googleapis.com",
                }),
            ),
        ],
        client_meta: &[],
        // TODO: Remove this method once Google releases their official auth methods.
        auth_methods: &[AuthMethodSpec {
            id: GEMINI_TERMINAL_AUTH_METHOD_ID,
            label: "Login",
            description: "Login with your Google or Vertex AI account",
            terminal: Some(TerminalAuthSpec {
                label: "gemini /auth",
                strip_args: &["--experimental-acp", "--acp"],
            }),
            success_patterns: &["Login successful", "Type your message"],
            retry_on_remote: true,
        }],
    },
    AgentProfile {
        id: CURSOR_ID,
        env: &[],
        client_meta: &[(PARAMETERIZED_MODEL_PICKER_META_KEY, true)],
        auth_methods: &[],
    },
];

/// The profile for `agent_id`, if ZZZ ships metadata for it.
pub fn profile_for(agent_id: &AgentId) -> Option<&'static AgentProfile> {
    PROFILES
        .iter()
        .find(|profile| profile.id == agent_id.as_ref())
}

/// The auth method spec with `method_id`, across every profile.
pub fn auth_method_spec(method_id: &str) -> Option<&'static AuthMethodSpec> {
    PROFILES
        .iter()
        .flat_map(|profile| profile.auth_methods)
        .find(|method| method.id == method_id)
}

/// Terminal output that means `method_id` logged in successfully.
///
/// Empty for a method ZZZ knows nothing about, in which case the login command's
/// exit status decides.
pub fn auth_method_success_patterns(method_id: &str) -> &'static [&'static str] {
    auth_method_spec(method_id).map_or(&[], |method| method.success_patterns)
}

/// Whether a login through `method_id` should be retried locally when the remote
/// command exits without a success pattern.
pub fn auth_method_retries_on_remote(method_id: &str) -> bool {
    auth_method_spec(method_id).is_some_and(|method| method.retry_on_remote)
}

/// The client capability meta a profile declares, as `initialize` reports it.
pub fn client_meta_for(agent_id: &AgentId) -> impl Iterator<Item = (&'static str, bool)> {
    profile_for(agent_id)
        .into_iter()
        .flat_map(|profile| profile.client_meta.iter().copied())
}

/// The auth method ZZZ synthesizes for `agent_id`, if its profile declares one.
///
/// Agents that report their own methods return `None`, and the reported list is
/// used unchanged.
pub fn synthesized_auth_method(
    agent_id: &AgentId,
    command: &AgentServerCommand,
) -> Option<acp::AuthMethod> {
    let (spec, terminal) = profile_for(agent_id)?
        .auth_methods
        .iter()
        .find_map(|spec| Some((spec, spec.terminal.as_ref()?)))?;
    let args = command
        .args
        .iter()
        .filter(|arg| !terminal.strip_args.contains(&arg.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let value = serde_json::json!({
        "label": terminal.label,
        "command": command.path.to_string_lossy(),
        "args": args,
        "env": command.env.clone().unwrap_or_default(),
    });
    let meta = acp::Meta::from_iter([("terminal-auth".to_owned(), value)]);
    Some(acp::AuthMethod::Agent(
        acp::AuthMethodAgent::new(spec.id, spec.label)
            .description(spec.description)
            .meta(meta),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id constant `custom.rs` exports, which is every agent we ship.
    const SHIPPED_IDS: &[&str] = &[CLAUDE_AGENT_ID, CODEX_ID, GEMINI_ID, CURSOR_ID];

    #[test]
    fn every_shipped_id_has_exactly_one_profile() {
        for id in SHIPPED_IDS {
            let profile = profile_for(&AgentId::new(*id))
                .unwrap_or_else(|| panic!("{id} has no profile, so its quirks are unbudgeted"));
            assert_eq!(profile.id, *id);
            assert_eq!(
                PROFILES.iter().filter(|profile| profile.id == *id).count(),
                1,
                "{id} is profiled twice"
            );
        }
        assert_eq!(PROFILES.len(), SHIPPED_IDS.len(), "unused profile entry");
        assert!(profile_for(&AgentId::new("not-a-shipped-agent")).is_none());
    }

    #[test]
    fn auth_method_ids_are_unique_and_resolvable() {
        for profile in PROFILES {
            for method in profile.auth_methods {
                assert_eq!(
                    auth_method_spec(method.id).map(|spec| spec.id),
                    Some(method.id),
                    "{} is not reachable by id",
                    method.id
                );
                assert_eq!(
                    PROFILES
                        .iter()
                        .flat_map(|profile| profile.auth_methods)
                        .filter(|other| other.id == method.id)
                        .count(),
                    1,
                    "{} is declared twice",
                    method.id
                );
            }
        }
        assert!(auth_method_spec("not-a-known-method").is_none());
        assert!(auth_method_success_patterns("not-a-known-method").is_empty());
        assert!(!auth_method_retries_on_remote("not-a-known-method"));
    }

    #[test]
    fn synthesized_auth_methods_strip_the_session_args() {
        let command = AgentServerCommand {
            path: "/usr/bin/gemini".into(),
            args: vec![
                "--experimental-acp".into(),
                "--verbose".into(),
                "--acp".into(),
            ],
            env: Some(collections::HashMap::from_iter([(
                "SURFACE".to_owned(),
                "zzz".to_owned(),
            )])),
        };

        let method = synthesized_auth_method(&AgentId::new(GEMINI_ID), &command)
            .expect("gemini's profile declares a synthesized auth method");
        let acp::AuthMethod::Agent(agent) = method else {
            panic!("expected an agent auth method");
        };

        assert_eq!(agent.id.0.as_ref(), GEMINI_TERMINAL_AUTH_METHOD_ID);
        let terminal_auth = agent
            .meta
            .as_ref()
            .and_then(|meta| meta.get("terminal-auth"))
            .expect("expected the terminal-auth meta entry");
        assert_eq!(terminal_auth["args"], serde_json::json!(["--verbose"]));
        assert_eq!(terminal_auth["label"], serde_json::json!("gemini /auth"));

        assert!(synthesized_auth_method(&AgentId::new(CURSOR_ID), &command).is_none());
    }

    #[test]
    fn only_cursor_declares_the_parameterized_model_picker() {
        let meta = |id: &str| client_meta_for(&AgentId::new(id)).collect::<Vec<_>>();

        assert_eq!(
            meta(CURSOR_ID),
            vec![(PARAMETERIZED_MODEL_PICKER_META_KEY, true)]
        );
        assert!(meta(CODEX_ID).is_empty());
        assert!(meta("not-a-shipped-agent").is_empty());
    }

    /// An agent-id comparison outside this table is the branch the table should
    /// have replaced, so the sites it replaced must not grow a new one.
    #[test]
    fn the_table_is_the_only_agent_id_dispatch() {
        const SOURCES: &[(&str, &str)] = &[
            ("custom.rs", include_str!("custom.rs")),
            ("acp.rs", include_str!("acp.rs")),
        ];

        for (name, source) in SOURCES {
            for id in SHIPPED_IDS {
                for pattern in [format!("== {id}"), format!("{id} =>")] {
                    assert!(
                        !source.contains(&pattern),
                        "{name} compares on an agent id ({pattern}); put the decision in PROFILES"
                    );
                }
            }
        }
    }
}
