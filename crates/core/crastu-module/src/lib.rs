//! Module trait, registry, intents and capability queries.
//!
//! A module (dictation, voice, clipper, editor, ...) contributes everything it offers through
//! [`Module::register`]: commands, settings, job kinds, MCP tools and intent handlers. Front ends
//! (GPUI, CLI, MCP, mobile) read the same [`Registry`], so leaving a module out of the build removes
//! its commands, tools and settings everywhere at once.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::Arc;

use serde_json::Value;

/// How much an action can change the user's data. Drives the approval policy for agents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Risk {
    /// Reads projects, transcripts, clips or memory.
    Read,
    /// Creates or edits data; always undoable.
    Change,
    /// Deletes or overwrites data; deleted items go to the trash first.
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleManifest {
    /// Stable lower-case id, for example `"dictation"`.
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
}

/// What a feature module implements.
pub trait Module: Send + Sync + 'static {
    fn manifest(&self) -> ModuleManifest;
    fn register(&self, registry: &mut Registry);
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandSpec {
    pub id: String,
    pub title: String,
    pub shortcut: Option<String>,
    pub risk: Risk,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingSpec {
    pub key: String,
    pub title: String,
    pub default: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobKindSpec {
    pub kind: String,
    pub title: String,
    pub uses_gpu: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    /// snake_case MCP tool name, unique across modules.
    pub name: String,
    pub description: String,
    pub risk: Risk,
    /// JSON Schema for the arguments.
    pub input_schema: Value,
}

/// Runs an MCP tool or command with JSON arguments.
pub trait ToolHandler: Send + Sync {
    fn call(&self, args: Value) -> Result<Value, ToolError>;
}

impl<F> ToolHandler for F
where
    F: Fn(Value) -> Result<Value, ToolError> + Send + Sync,
{
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        self(args)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    Unknown(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("{0}")]
    Failed(String),
}

/// A request from one module to whoever can handle it, without a compile-time dependency.
/// Example: the voice module sends `open_asset` and the editor (if built in) opens it.
#[derive(Clone, Debug, PartialEq)]
pub struct Intent {
    pub kind: String,
    pub payload: Value,
}

type IntentHandler = Arc<dyn Fn(&Intent) -> Result<(), ToolError> + Send + Sync>;

struct Tool {
    module: &'static str,
    spec: ToolSpec,
    handler: Arc<dyn ToolHandler>,
}

/// A problem found while modules registered themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryError(pub String);

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Everything the built-in modules contribute.
#[derive(Default)]
pub struct Registry {
    current: &'static str,
    modules: Vec<ModuleManifest>,
    commands: Vec<(&'static str, CommandSpec)>,
    settings: Vec<(&'static str, SettingSpec)>,
    job_kinds: Vec<(&'static str, JobKindSpec)>,
    tools: BTreeMap<String, Tool>,
    intents: BTreeMap<String, Vec<IntentHandler>>,
    errors: Vec<RegistryError>,
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field(
                "modules",
                &self.modules.iter().map(|m| m.id).collect::<Vec<_>>(),
            )
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn is_snake_case(s: &str) -> bool {
    !s.is_empty()
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Registry {
    /// Registers every module and reports all problems at once (duplicate ids, bad names).
    pub fn build(modules: &[&dyn Module]) -> Result<Registry, Vec<RegistryError>> {
        let mut r = Registry::default();
        let mut seen = HashSet::new();
        for m in modules {
            let manifest = m.manifest();
            if !seen.insert(manifest.id) {
                r.errors.push(RegistryError(format!(
                    "module {:?} registered twice",
                    manifest.id
                )));
                continue;
            }
            r.current = manifest.id;
            r.modules.push(manifest);
            m.register(&mut r);
        }
        r.current = "";
        if r.errors.is_empty() {
            Ok(r)
        } else {
            Err(r.errors)
        }
    }

    fn err(&mut self, msg: String) {
        self.errors
            .push(RegistryError(format!("{}: {msg}", self.current)));
    }

    pub fn command(&mut self, spec: CommandSpec) {
        if self.commands.iter().any(|(_, c)| c.id == spec.id) {
            self.err(format!("duplicate command {:?}", spec.id));
            return;
        }
        self.commands.push((self.current, spec));
    }

    pub fn setting(&mut self, spec: SettingSpec) {
        let prefix = format!("{}.", self.current);
        if !spec.key.starts_with(&prefix) {
            self.err(format!("setting {:?} must start with {prefix:?}", spec.key));
            return;
        }
        self.settings.push((self.current, spec));
    }

    pub fn job_kind(&mut self, spec: JobKindSpec) {
        if self.job_kinds.iter().any(|(_, j)| j.kind == spec.kind) {
            self.err(format!("duplicate job kind {:?}", spec.kind));
            return;
        }
        self.job_kinds.push((self.current, spec));
    }

    pub fn tool(&mut self, spec: ToolSpec, handler: impl ToolHandler + 'static) {
        if !is_snake_case(&spec.name) {
            self.err(format!("tool name {:?} must be snake_case", spec.name));
            return;
        }
        if self.tools.contains_key(&spec.name) {
            self.err(format!("duplicate tool {:?}", spec.name));
            return;
        }
        let name = spec.name.clone();
        self.tools.insert(
            name,
            Tool {
                module: self.current,
                spec,
                handler: Arc::new(handler),
            },
        );
    }

    pub fn on_intent(
        &mut self,
        kind: &str,
        handler: impl Fn(&Intent) -> Result<(), ToolError> + Send + Sync + 'static,
    ) {
        self.intents
            .entry(kind.to_string())
            .or_default()
            .push(Arc::new(handler));
    }

    pub fn modules(&self) -> &[ModuleManifest] {
        &self.modules
    }

    pub fn has_module(&self, id: &str) -> bool {
        self.modules.iter().any(|m| m.id == id)
    }

    pub fn commands(&self) -> impl Iterator<Item = &CommandSpec> {
        self.commands.iter().map(|(_, c)| c)
    }

    pub fn settings(&self) -> impl Iterator<Item = &SettingSpec> {
        self.settings.iter().map(|(_, s)| s)
    }

    pub fn job_kinds(&self) -> impl Iterator<Item = &JobKindSpec> {
        self.job_kinds.iter().map(|(_, j)| j)
    }

    /// Tool specs sorted by name, for listing to MCP clients.
    pub fn tools(&self) -> impl Iterator<Item = &ToolSpec> {
        self.tools.values().map(|t| &t.spec)
    }

    /// The module that owns a tool.
    pub fn tool_module(&self, name: &str) -> Option<&'static str> {
        self.tools.get(name).map(|t| t.module)
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::Unknown(name.to_string()))?;
        tool.handler.call(args)
    }

    /// Delivers an intent to every handler; returns how many handled it (0 when no module can).
    pub fn send_intent(&self, intent: &Intent) -> Result<usize, ToolError> {
        let Some(handlers) = self.intents.get(&intent.kind) else {
            return Ok(0);
        };
        for h in handlers {
            h(intent)?;
        }
        Ok(handlers.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    struct Voice;
    impl Module for Voice {
        fn manifest(&self) -> ModuleManifest {
            ModuleManifest {
                id: "voice",
                name: "Voice",
                version: "0.0.0",
            }
        }
        fn register(&self, r: &mut Registry) {
            r.command(CommandSpec {
                id: "voice.generate".into(),
                title: "Generate take".into(),
                shortcut: None,
                risk: Risk::Change,
            });
            r.setting(SettingSpec {
                key: "voice.speed".into(),
                title: "Speed".into(),
                default: json!(1.0),
            });
            r.job_kind(JobKindSpec {
                kind: "tts".into(),
                title: "Generate speech".into(),
                uses_gpu: true,
            });
            r.tool(
                ToolSpec {
                    name: "voice_list".into(),
                    description: "List voices".into(),
                    risk: Risk::Read,
                    input_schema: json!({"type": "object"}),
                },
                |_args: Value| Ok(json!(["F1", "F2"])),
            );
        }
    }

    struct Editor(Arc<Mutex<Vec<String>>>);
    impl Module for Editor {
        fn manifest(&self) -> ModuleManifest {
            ModuleManifest {
                id: "editor",
                name: "Editor",
                version: "0.0.0",
            }
        }
        fn register(&self, r: &mut Registry) {
            let opened = self.0.clone();
            r.on_intent("open_asset", move |i| {
                opened
                    .lock()
                    .unwrap()
                    .push(i.payload["path"].as_str().unwrap_or_default().to_string());
                Ok(())
            });
        }
    }

    #[test]
    fn modules_contribute_and_tools_run() {
        let r = Registry::build(&[&Voice]).unwrap();
        assert!(r.has_module("voice"));
        assert_eq!(r.commands().count(), 1);
        assert_eq!(r.settings().next().unwrap().key, "voice.speed");
        assert_eq!(r.tool_module("voice_list"), Some("voice"));
        assert_eq!(
            r.call_tool("voice_list", json!({})).unwrap(),
            json!(["F1", "F2"])
        );
        assert_eq!(
            r.call_tool("nope", json!({})),
            Err(ToolError::Unknown("nope".into()))
        );
    }

    #[test]
    fn intents_reach_whichever_module_is_built_in() {
        let opened = Arc::new(Mutex::new(Vec::new()));
        let editor = Editor(opened.clone());
        let intent = Intent {
            kind: "open_asset".into(),
            payload: json!({"path": "take-3.wav"}),
        };

        let with_editor = Registry::build(&[&Voice, &editor]).unwrap();
        assert_eq!(with_editor.send_intent(&intent).unwrap(), 1);
        assert_eq!(*opened.lock().unwrap(), vec!["take-3.wav".to_string()]);

        let without_editor = Registry::build(&[&Voice]).unwrap();
        assert_eq!(without_editor.send_intent(&intent).unwrap(), 0);
    }

    struct Bad;
    impl Module for Bad {
        fn manifest(&self) -> ModuleManifest {
            ModuleManifest {
                id: "bad",
                name: "Bad",
                version: "0.0.0",
            }
        }
        fn register(&self, r: &mut Registry) {
            r.setting(SettingSpec {
                key: "voice.speed".into(),
                title: "Steals a key".into(),
                default: json!(1),
            });
            r.tool(
                ToolSpec {
                    name: "Voice-List".into(),
                    description: String::new(),
                    risk: Risk::Read,
                    input_schema: json!({}),
                },
                |_a: Value| Ok(json!(null)),
            );
        }
    }

    #[test]
    fn registration_problems_are_all_reported() {
        let errors = Registry::build(&[&Voice, &Bad, &Voice]).unwrap_err();
        assert_eq!(errors.len(), 3, "{errors:?}");
    }
}
