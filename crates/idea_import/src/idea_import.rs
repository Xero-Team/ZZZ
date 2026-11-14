// Ported from tau (https://github.com/gopherlian/tau) at
// a7708f8354e4f939caf77e297aa4f4ee2ecd00a3, licensed GPL-3.0-or-later.
// Original copyright notice retained in NOTICE.
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, Ipv4Addr},
};

use quick_xml::{
    Reader, XmlVersion,
    events::{BytesStart, Event},
};
use serde_json::{Map, Value};
use task::{DebugScenario, DebugTaskFile, TcpArgumentsTemplate};

mod twin;
mod workspace_integration;

pub use twin::{TwinOutcome, ensure_twin};
pub use workspace_integration::{ImportIdeaProject, init};

/// The JetBrains project files the importer reads. [`import`] never touches the file system;
/// [`ensure_twin`] loads these files read-only.
#[derive(Clone, Debug, Default)]
pub struct IdeaProject {
    /// Directory that contains `.idea`, relative to the worktree root, with `/` separators.
    /// Empty when `.idea` is at the root of the worktree.
    pub directory_in_worktree: String,
    pub user_home: Option<String>,
    /// `.idea/runConfigurations/*.xml` as `(file name, contents)`.
    pub run_configuration_files: Vec<(String, String)>,
    pub workspace_xml: Option<String>,
    pub modules_xml: Option<String>,
    /// `go.mod` next to `.idea`, used to turn Go package import paths into directories.
    pub go_mod: Option<String>,
    /// Files that exist but could not be loaded, reported as notes.
    pub read_errors: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdeaImport {
    pub debug_scenarios: DebugTaskFile,
    pub report: ImportReport,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    pub imported: Vec<ImportedConfiguration>,
    pub skipped: Vec<SkippedConfiguration>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedConfiguration {
    pub name: String,
    pub kind: String,
    pub adapter: String,
    pub source: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkippedConfiguration {
    pub name: String,
    pub type_id: String,
    pub source: String,
    pub reason: String,
}

const RUN_CONFIGURATION_COMPONENTS: &[&str] = &["ProjectRunConfigurationManager", "RunManager"];

const JAVA_CONFIGURATION_TYPES: &[&str] = &[
    "Application",
    "JarApplication",
    "Remote",
    "JUnit",
    "TestNG",
    "SpringBootApplicationConfigurationType",
    "JetRunConfigurationType",
    "KotlinStandaloneScriptRunConfigurationType",
    "GradleRunConfiguration",
    "MavenRunConfiguration",
];

pub fn import(project: &IdeaProject) -> IdeaImport {
    let (idea_directory, project_directory) = if project.directory_in_worktree.is_empty() {
        (".idea".to_owned(), "${ZZZ_WORKTREE_ROOT}".to_owned())
    } else {
        (
            format!("{}/.idea", project.directory_in_worktree),
            format!("${{ZZZ_WORKTREE_ROOT}}/{}", project.directory_in_worktree),
        )
    };
    let mut report = ImportReport {
        notes: project.read_errors.clone(),
        ..ImportReport::default()
    };
    let mut context = ImportContext {
        project_directory,
        user_home: project.user_home.clone(),
        module_directories: BTreeMap::new(),
        go_module_path: project.go_mod.as_deref().and_then(parse_go_module_path),
    };
    if let Some(modules_xml) = &project.modules_xml {
        match parse_module_directories(modules_xml, &context) {
            Ok(module_directories) => context.module_directories = module_directories,
            Err(error) => report.notes.push(format!(
                "`{idea_directory}/modules.xml` could not be read, so `$MODULE_DIR$` is not resolved: {error:#}"
            )),
        }
    }

    let mut run_configuration_files = project.run_configuration_files.iter().collect::<Vec<_>>();
    run_configuration_files.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut sources = run_configuration_files
        .into_iter()
        .map(|(file_name, contents)| {
            (
                format!("{idea_directory}/runConfigurations/{file_name}"),
                contents.as_str(),
            )
        })
        .collect::<Vec<_>>();
    match &project.workspace_xml {
        Some(contents) => sources.push((format!("{idea_directory}/workspace.xml"), contents)),
        None => report.notes.push(format!(
            "`{idea_directory}/workspace.xml` was not found, so run configurations that were not \
             shared with the project could not be imported."
        )),
    }

    let mut debug_scenarios = Vec::new();
    for (source, contents) in sources {
        let configurations = match parse_run_configurations(contents) {
            Ok(configurations) => configurations,
            Err(error) => {
                report
                    .notes
                    .push(format!("`{source}` could not be read: {error:#}"));
                continue;
            }
        };
        for configuration in configurations {
            if configuration.attribute("default") == Some("true") {
                continue;
            }
            match convert(&configuration, &context) {
                Ok(conversion) => {
                    report.imported.push(ImportedConfiguration {
                        name: configuration.name().to_owned(),
                        kind: conversion.kind.to_owned(),
                        adapter: conversion.scenario.adapter.to_string(),
                        source: source.clone(),
                        warnings: conversion.warnings,
                    });
                    debug_scenarios.push(conversion.scenario);
                }
                Err(reason) => report.skipped.push(SkippedConfiguration {
                    name: configuration.name().to_owned(),
                    type_id: configuration
                        .attribute("type")
                        .unwrap_or_default()
                        .to_owned(),
                    source: source.clone(),
                    reason,
                }),
            }
        }
    }
    if report.imported.is_empty() && report.skipped.is_empty() {
        report.notes.push(format!(
            "No run configurations were found in `{idea_directory}`."
        ));
    }

    IdeaImport {
        debug_scenarios: DebugTaskFile(debug_scenarios),
        report,
    }
}

impl ImportReport {
    pub fn to_markdown(&self) -> String {
        let mut markdown = String::from(
            "# `.idea` import report\n\n\
             ZZZ imported these run configurations from `.idea` without modifying it. \
             Edit `debug.json` in this folder to adjust them; ZZZ never overwrites it. \
             To import again, delete this folder.\n",
        );
        if !self.imported.is_empty() {
            markdown.push_str(&format!("\n## Imported ({})\n\n", self.imported.len()));
            for configuration in &self.imported {
                markdown.push_str(&format!(
                    "- **{}** ({}, {}), from `{}`\n",
                    configuration.name,
                    configuration.kind,
                    configuration.adapter,
                    configuration.source
                ));
                for warning in &configuration.warnings {
                    markdown.push_str(&format!("  - {warning}\n"));
                }
            }
        }
        if !self.skipped.is_empty() {
            markdown.push_str(&format!("\n## Not imported ({})\n\n", self.skipped.len()));
            for configuration in &self.skipped {
                markdown.push_str(&format!(
                    "- **{}** (`{}`), from `{}`: {}\n",
                    configuration.name,
                    configuration.type_id,
                    configuration.source,
                    configuration.reason
                ));
            }
        }
        if !self.notes.is_empty() {
            markdown.push_str("\n## Notes\n\n");
            for note in &self.notes {
                markdown.push_str(&format!("- {note}\n"));
            }
        }
        markdown
    }
}

struct ImportContext {
    project_directory: String,
    user_home: Option<String>,
    module_directories: BTreeMap<String, String>,
    go_module_path: Option<String>,
}

impl ImportContext {
    fn go_package_directory(&self, package: &str) -> Option<String> {
        let module_path = self.go_module_path.as_deref()?;
        if package == module_path {
            return Some(self.project_directory.clone());
        }
        let relative = package.strip_prefix(module_path)?.strip_prefix('/')?;
        Some(format!("{}/{relative}", self.project_directory))
    }
}

#[derive(Debug, Default)]
struct RunConfiguration {
    attributes: BTreeMap<String, String>,
    options: BTreeMap<String, String>,
    /// Children like `<kind value="PACKAGE" />`, keyed by element name.
    values: BTreeMap<String, String>,
    envs: Vec<(String, String)>,
    module: Option<String>,
    compound_members: Vec<String>,
}

impl RunConfiguration {
    fn name(&self) -> &str {
        self.attribute("name").unwrap_or_default()
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    fn option(&self, name: &str) -> Option<&str> {
        self.options
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    fn read_child(
        &mut self,
        parent: Option<&str>,
        element: &str,
        attributes: &BTreeMap<String, String>,
    ) {
        match (parent, element) {
            (None, "option") => {
                if let (Some(name), Some(value)) = (attributes.get("name"), attributes.get("value"))
                {
                    self.options.insert(name.clone(), value.clone());
                }
            }
            (None, "module") => self.module = attributes.get("name").cloned(),
            (None, "toRun") => self
                .compound_members
                .extend(attributes.get("name").cloned()),
            (Some("envs"), "env") => {
                if let Some(name) = attributes.get("name") {
                    let value = attributes.get("value").cloned().unwrap_or_default();
                    self.envs.push((name.clone(), value));
                }
            }
            (None, _) => {
                if let Some(value) = attributes.get("value") {
                    self.values.insert(element.to_owned(), value.clone());
                }
            }
            _ => {}
        }
    }
}

fn parse_run_configurations(xml: &str) -> anyhow::Result<Vec<RunConfiguration>> {
    let mut reader = Reader::from_str(xml);
    let mut configurations = Vec::new();
    let mut component = None;
    let mut current: Option<RunConfiguration> = None;
    let mut open_elements: Vec<String> = Vec::new();
    loop {
        match reader.read_event()? {
            event @ (Event::Start(_) | Event::Empty(_)) => {
                let is_empty = matches!(event, Event::Empty(_));
                let (Event::Start(element) | Event::Empty(element)) = &event else {
                    continue;
                };
                let element_name = element_name(element)?;
                let attributes = element_attributes(element)?;
                if let Some(configuration) = current.as_mut() {
                    configuration.read_child(
                        open_elements.last().map(String::as_str),
                        &element_name,
                        &attributes,
                    );
                    if !is_empty {
                        open_elements.push(element_name);
                    }
                } else if element_name == "component" {
                    if !is_empty {
                        component = attributes.get("name").cloned();
                    }
                } else if element_name == "configuration"
                    && component
                        .as_deref()
                        .is_some_and(|name| RUN_CONFIGURATION_COMPONENTS.contains(&name))
                {
                    let configuration = RunConfiguration {
                        attributes,
                        ..RunConfiguration::default()
                    };
                    if is_empty {
                        configurations.push(configuration);
                    } else {
                        current = Some(configuration);
                    }
                }
            }
            Event::End(element) => {
                if current.is_some() {
                    if open_elements.pop().is_none() {
                        configurations.extend(current.take());
                    }
                } else if element.name().as_ref() == "component" {
                    component = None;
                }
            }
            Event::Eof => {
                anyhow::ensure!(
                    current.is_none(),
                    "the file ends inside a <configuration> element"
                );
                break;
            }
            _ => {}
        }
    }
    Ok(configurations)
}

fn parse_module_directories(
    modules_xml: &str,
    context: &ImportContext,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut reader = Reader::from_str(modules_xml);
    let mut module_directories = BTreeMap::new();
    loop {
        match reader.read_event()? {
            Event::Start(element) | Event::Empty(element) => {
                if element_name(&element)? != "module" {
                    continue;
                }
                let attributes = element_attributes(&element)?;
                let Some(file_path) = attributes.get("filepath") else {
                    continue;
                };
                let Some((directory, file_name)) = file_path.rsplit_once('/') else {
                    continue;
                };
                let Some(module_name) = file_name.strip_suffix(".iml") else {
                    continue;
                };
                // Mirrors IntelliJ's `PathMacroUtil.getModuleDir`: a module file stored inside
                // `.idea` is rooted at the folder that holds `.idea`.
                let directory = match directory.rsplit_once('/') {
                    Some((parent, ".idea")) if !parent.is_empty() => parent,
                    _ => directory,
                };
                let directory = MacroExpander::new(context, None).expand(directory);
                module_directories.insert(module_name.to_owned(), directory);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(module_directories)
}

fn parse_go_module_path(go_mod: &str) -> Option<String> {
    go_mod.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("module")?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let path = rest
            .split("//")
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches(|character| character == '"' || character == '`');
        (!path.is_empty()).then(|| path.to_owned())
    })
}

fn element_name(element: &BytesStart) -> anyhow::Result<String> {
    Ok(element.name().as_ref().to_owned())
}

fn element_attributes(element: &BytesStart) -> anyhow::Result<BTreeMap<String, String>> {
    let mut attributes = BTreeMap::new();
    for attribute in element.attributes() {
        let attribute = attribute?;
        let key = attribute.key.as_ref().to_owned();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)?
            .into_owned();
        attributes.insert(key, value);
    }
    Ok(attributes)
}

struct Conversion {
    kind: &'static str,
    scenario: DebugScenario,
    warnings: Vec<String>,
}

fn convert(
    configuration: &RunConfiguration,
    context: &ImportContext,
) -> Result<Conversion, String> {
    match configuration.attribute("type").unwrap_or_default() {
        "PythonConfigurationType" => convert_python(configuration, context),
        "GoApplicationRunConfiguration" => convert_go_application(configuration, context),
        "GoTestRunConfiguration" => convert_go_test(configuration, context),
        "GoRemoteDebugConfigurationType" => convert_go_remote(configuration),
        "CompoundRunConfigurationType" => {
            let members = configuration
                .compound_members
                .iter()
                .map(|member| format!("`{member}`"))
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Compound configurations have no Debug Adapter Protocol equivalent. \
                 Debug its members on their own: {members}."
            ))
        }
        type_id if JAVA_CONFIGURATION_TYPES.contains(&type_id) => Err(
            "Java and Kotlin configurations need a Java debug adapter, which ZZZ does not include yet."
                .to_owned(),
        ),
        type_id => Err(format!(
            "Configuration type `{type_id}` is not supported yet."
        )),
    }
}

fn convert_python(
    configuration: &RunConfiguration,
    context: &ImportContext,
) -> Result<Conversion, String> {
    let script = configuration
        .option("SCRIPT_NAME")
        .ok_or_else(|| "No script or module is set.".to_owned())?;
    let mut expander = MacroExpander::new(context, configuration.module.as_deref());
    let mut warnings = Vec::new();
    let mut config = Map::new();
    config.insert("request".to_owned(), "launch".into());
    if configuration.option("MODULE_MODE") == Some("true") {
        config.insert("module".to_owned(), expander.expand(script).into());
    } else {
        config.insert("program".to_owned(), expander.expand(script).into());
    }
    insert_parameters(
        &mut config,
        "args",
        split_parameters(configuration.option("PARAMETERS").unwrap_or_default()),
        &mut expander,
    );
    insert_expanded(
        &mut config,
        "cwd",
        configuration.option("WORKING_DIRECTORY"),
        &mut expander,
    );
    insert_env(&mut config, &configuration.envs, &mut expander);
    if let Some(env_files) = configuration.option("ENV_FILES") {
        if env_files.contains(';') {
            warnings.push(format!(
                "Several env files are set (`{env_files}`), but Debugpy takes one `envFile`, so none was imported."
            ));
        } else {
            config.insert("envFile".to_owned(), expander.expand(env_files).into());
        }
    }
    match configuration.option("SDK_HOME") {
        Some(sdk_home) => {
            config.insert("python".to_owned(), expander.expand(sdk_home).into());
        }
        None => {
            if let Some(sdk_name) = configuration.option("SDK_NAME") {
                warnings.push(format!(
                    "Uses the interpreter `{sdk_name}` from the IDE's global SDK table, which is not \
                     stored in `.idea`. ZZZ uses the Python toolchain selected for the project; set \
                     `python` to pin one."
                ));
            } else if configuration.option("IS_MODULE_SDK") == Some("true") {
                warnings.push(
                    "Uses the module's interpreter from the IDE's global SDK table, which is not \
                     stored in `.idea`. ZZZ uses the Python toolchain selected for the project; set \
                     `python` to pin one."
                        .to_owned(),
                );
            }
        }
    }
    insert_parameters(
        &mut config,
        "pythonArgs",
        split_parameters(
            configuration
                .option("INTERPRETER_OPTIONS")
                .unwrap_or_default(),
        ),
        &mut expander,
    );
    if configuration.option("PARENT_ENVS") == Some("false") {
        warnings.push(
            "Does not inherit the parent environment in the IDE; Debugpy always inherits it."
                .to_owned(),
        );
    }
    if configuration.option("REDIRECT_INPUT") == Some("true") {
        let input_file = expander.expand(configuration.option("INPUT_FILE").unwrap_or_default());
        warnings.push(format!(
            "Redirects input from `{input_file}`, which Debugpy does not support."
        ));
    }
    warnings.extend(expander.into_warnings());
    Ok(Conversion {
        kind: "Python",
        scenario: debug_scenario(configuration.name(), "Debugpy", config),
        warnings,
    })
}

fn convert_go_application(
    configuration: &RunConfiguration,
    context: &ImportContext,
) -> Result<Conversion, String> {
    let mut expander = MacroExpander::new(context, configuration.module.as_deref());
    let mut warnings = Vec::new();
    let program = go_program(configuration, context, &mut expander, &mut warnings)?;
    let mut config = Map::new();
    config.insert("request".to_owned(), "launch".into());
    config.insert("mode".to_owned(), "debug".into());
    config.insert("program".to_owned(), program.into());
    insert_parameters(
        &mut config,
        "args",
        split_parameters(configuration.value("parameters").unwrap_or_default()),
        &mut expander,
    );
    insert_go_common(&mut config, configuration, &mut expander);
    warnings.extend(expander.into_warnings());
    Ok(Conversion {
        kind: "Go Application",
        scenario: debug_scenario(configuration.name(), "Delve", config),
        warnings,
    })
}

fn convert_go_test(
    configuration: &RunConfiguration,
    context: &ImportContext,
) -> Result<Conversion, String> {
    let mut expander = MacroExpander::new(context, configuration.module.as_deref());
    let mut warnings = Vec::new();
    let program = go_program(configuration, context, &mut expander, &mut warnings)?;
    if configuration.value("kind") == Some("DIRECTORY") {
        warnings.push(format!(
            "Delve debugs a single package: only the tests of the package in `{program}` run, \
             not those of its subdirectories."
        ));
    }
    let mut config = Map::new();
    config.insert("request".to_owned(), "launch".into());
    config.insert("mode".to_owned(), "test".into());
    config.insert("program".to_owned(), program.into());
    let mut arguments = Vec::new();
    if let Some(pattern) = configuration.value("pattern") {
        match configuration.value("framework").unwrap_or("gotest") {
            "gotest" => arguments.extend(["-test.run".to_owned(), pattern.to_owned()]),
            framework => warnings.push(format!(
                "The `{framework}` test filter `{pattern}` was not imported; add it to `args` by hand."
            )),
        }
    }
    arguments.extend(split_parameters(
        configuration.value("parameters").unwrap_or_default(),
    ));
    insert_parameters(&mut config, "args", arguments, &mut expander);
    insert_go_common(&mut config, configuration, &mut expander);
    warnings.extend(expander.into_warnings());
    Ok(Conversion {
        kind: "Go Test",
        scenario: debug_scenario(configuration.name(), "Delve", config),
        warnings,
    })
}

fn convert_go_remote(configuration: &RunConfiguration) -> Result<Conversion, String> {
    let port = configuration
        .attribute("port")
        .filter(|port| !port.is_empty())
        .ok_or_else(|| "No port is set.".to_owned())?;
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("`{port}` is not a valid port."))?;
    let host = match configuration.attribute("host").unwrap_or_default().trim() {
        "" => None,
        "localhost" => Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        host => Some(host.parse::<IpAddr>().map_err(|_| {
            format!(
                "The host `{host}` is not an IP address, and ZZZ's debug scenarios connect only to \
                 IP addresses. Add a Delve attach scenario with its IP address by hand."
            )
        })?),
    };
    let mut config = Map::new();
    config.insert("request".to_owned(), "attach".into());
    config.insert("mode".to_owned(), "remote".into());
    let mut scenario = debug_scenario(configuration.name(), "Delve", config);
    scenario.tcp_connection = Some(TcpArgumentsTemplate {
        port: Some(port),
        host,
        timeout: None,
    });
    Ok(Conversion {
        kind: "Go Remote",
        scenario,
        warnings: Vec::new(),
    })
}

fn go_program(
    configuration: &RunConfiguration,
    context: &ImportContext,
    expander: &mut MacroExpander,
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let kind = match configuration.value("kind") {
        Some(kind) => kind,
        None if configuration.value("package").is_some() => "PACKAGE",
        None if configuration.value("directory").is_some() => "DIRECTORY",
        None => "FILE",
    };
    match kind {
        "PACKAGE" => {
            let package = configuration
                .value("package")
                .ok_or_else(|| "No package is set.".to_owned())?;
            Ok(
                if let Some(directory) = context.go_package_directory(package) {
                    directory
                } else {
                    warnings.push(format!(
                    "`{package}` is not in the `go.mod` next to `.idea`, so `program` keeps the \
                     import path and Delve builds it from the working directory."
                ));
                    package.to_owned()
                },
            )
        }
        "DIRECTORY" => configuration
            .value("directory")
            .map(|directory| expander.expand(directory))
            .ok_or_else(|| "No directory is set.".to_owned()),
        "FILE" => configuration
            .value("filePath")
            .map(|file_path| expander.expand(file_path))
            .ok_or_else(|| "No file is set.".to_owned()),
        kind => Err(format!("The Go run kind `{kind}` is not supported.")),
    }
}

fn insert_go_common(
    config: &mut Map<String, Value>,
    configuration: &RunConfiguration,
    expander: &mut MacroExpander,
) {
    insert_expanded(
        config,
        "cwd",
        configuration.value("working_directory"),
        expander,
    );
    insert_env(config, &configuration.envs, expander);
    insert_expanded(
        config,
        "buildFlags",
        configuration.value("go_parameters"),
        expander,
    );
}

fn insert_expanded(
    config: &mut Map<String, Value>,
    key: &str,
    value: Option<&str>,
    expander: &mut MacroExpander,
) {
    if let Some(value) = value {
        config.insert(key.to_owned(), expander.expand(value).into());
    }
}

fn insert_parameters(
    config: &mut Map<String, Value>,
    key: &str,
    parameters: Vec<String>,
    expander: &mut MacroExpander,
) {
    if !parameters.is_empty() {
        let parameters = parameters
            .iter()
            .map(|parameter| expander.expand(parameter))
            .collect::<Vec<_>>();
        config.insert(key.to_owned(), parameters.into());
    }
}

fn insert_env(
    config: &mut Map<String, Value>,
    envs: &[(String, String)],
    expander: &mut MacroExpander,
) {
    if !envs.is_empty() {
        let env = envs
            .iter()
            .map(|(name, value)| (name.clone(), Value::from(expander.expand(value))))
            .collect::<Map<_, _>>();
        config.insert("env".to_owned(), env.into());
    }
}

fn debug_scenario(label: &str, adapter: &str, config: Map<String, Value>) -> DebugScenario {
    DebugScenario {
        label: label.to_owned().into(),
        adapter: adapter.to_owned().into(),
        build: None,
        config: Value::Object(config),
        tcp_connection: None,
    }
}

/// Expands IntelliJ path macros (`$NAME$`). Unknown or unresolvable macros are left as is and
/// reported, because guessing a path would silently debug the wrong thing.
struct MacroExpander<'a> {
    context: &'a ImportContext,
    module_directory: Option<String>,
    unresolved: BTreeSet<String>,
}

impl<'a> MacroExpander<'a> {
    fn new(context: &'a ImportContext, module: Option<&str>) -> Self {
        Self {
            context,
            module_directory: module
                .and_then(|module| context.module_directories.get(module))
                .cloned(),
            unresolved: BTreeSet::new(),
        }
    }

    fn expand(&mut self, value: &str) -> String {
        let mut expanded = String::with_capacity(value.len());
        let mut rest = value;
        while let Some(start) = rest.find('$') {
            expanded.push_str(&rest[..start]);
            let after_dollar = &rest[start + 1..];
            let macro_name = after_dollar
                .find('$')
                .map(|end| &after_dollar[..end])
                .filter(|name| is_macro_name(name));
            let Some(macro_name) = macro_name else {
                expanded.push('$');
                rest = after_dollar;
                continue;
            };
            let macro_length = macro_name.len() + 2;
            if let Some(expansion) = self.resolve(macro_name) {
                expanded.push_str(&expansion);
            } else {
                let original = &rest[start..start + macro_length];
                self.unresolved.insert(original.to_owned());
                expanded.push_str(original);
            }
            rest = &rest[start + macro_length..];
        }
        expanded.push_str(rest);
        expanded
    }

    fn resolve(&self, macro_name: &str) -> Option<String> {
        match macro_name {
            "PROJECT_DIR" => Some(self.context.project_directory.clone()),
            "USER_HOME" => self.context.user_home.clone(),
            "MODULE_DIR" => self.module_directory.clone(),
            _ => None,
        }
    }

    fn into_warnings(self) -> Vec<String> {
        self.unresolved
            .into_iter()
            .map(|macro_text| format!("Could not resolve `{macro_text}`, so it was left as is."))
            .collect()
    }
}

fn is_macro_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Splits a parameter string the way IntelliJ's `ParametersListUtil.parse` does by default:
/// only double quotes group, quotes are dropped, and `\"` is a literal quote.
fn split_parameters(parameters: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut token = String::new();
    let mut in_quotes = false;
    let mut escaped_quote = false;
    // A pair of quotes produces a parameter even when nothing is between them.
    let mut quoted = false;
    let mut characters = parameters.trim().chars().peekable();
    while let Some(character) = characters.next() {
        if character == '"' {
            if !escaped_quote {
                in_quotes = !in_quotes;
                quoted = true;
                continue;
            }
            escaped_quote = false;
        } else if character.is_whitespace() {
            if !in_quotes {
                if !token.is_empty() || quoted {
                    result.push(std::mem::take(&mut token));
                    quoted = false;
                }
                continue;
            }
        } else if character == '\\' && characters.peek() == Some(&'"') {
            escaped_quote = true;
            continue;
        }
        token.push(character);
    }
    if !token.is_empty() || quoted {
        result.push(token);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use pretty_assertions::assert_eq;
    use serde_json::json;
    use task::{DebugScenario, TcpArgumentsTemplate};

    use super::*;

    fn fixture_project() -> IdeaProject {
        IdeaProject {
            directory_in_worktree: String::new(),
            user_home: Some("/home/test".to_owned()),
            run_configuration_files: vec![
                (
                    "controller__kube_.xml".to_owned(),
                    include_str!("../test_data/runConfigurations/controller__kube_.xml").to_owned(),
                ),
                (
                    "API_server.xml".to_owned(),
                    include_str!("../test_data/runConfigurations/API_server.xml").to_owned(),
                ),
                (
                    "Debug_agent.xml".to_owned(),
                    include_str!("../test_data/runConfigurations/Debug_agent.xml").to_owned(),
                ),
            ],
            workspace_xml: Some(include_str!("../test_data/workspace.xml").to_owned()),
            modules_xml: Some(include_str!("../test_data/modules.xml").to_owned()),
            go_mod: Some(include_str!("../test_data/go.mod").to_owned()),
            read_errors: Vec::new(),
        }
    }

    fn single_configuration_project(configuration: &str) -> IdeaProject {
        IdeaProject {
            user_home: Some("/home/test".to_owned()),
            run_configuration_files: vec![(
                "config.xml".to_owned(),
                format!(
                    r#"<component name="ProjectRunConfigurationManager">{configuration}</component>"#
                ),
            )],
            workspace_xml: Some(r#"<project version="4"></project>"#.to_owned()),
            ..IdeaProject::default()
        }
    }

    fn scenario(label: &str, adapter: &str, config: serde_json::Value) -> DebugScenario {
        DebugScenario {
            label: label.to_owned().into(),
            adapter: adapter.to_owned().into(),
            build: None,
            config,
            tcp_connection: None,
        }
    }

    #[test]
    fn test_split_parameters_like_intellij() {
        assert_eq!(
            split_parameters(r#"  --title "My API"   --port 8080  "#),
            vec!["--title", "My API", "--port", "8080"]
        );
        assert_eq!(
            split_parameters("'single quotes' do not group"),
            vec!["'single", "quotes'", "do", "not", "group"]
        );
        assert_eq!(split_parameters(r#"say \"hi\""#), vec!["say", r#""hi""#]);
        assert_eq!(split_parameters(r#""a\"b""#), vec![r#"a"b"#]);
        assert_eq!(split_parameters(r#"a "" b"#), vec!["a", "", "b"]);
        assert_eq!(
            split_parameters(r"C:\tools\run.exe"),
            vec![r"C:\tools\run.exe"]
        );
        assert_eq!(split_parameters("   "), Vec::<String>::new());
    }

    #[test]
    fn test_imports_the_fixture_project() {
        let result = import(&fixture_project());

        let mut remote = scenario(
            "Debug agent",
            "Delve",
            json!({ "request": "attach", "mode": "remote" }),
        );
        remote.tcp_connection = Some(TcpArgumentsTemplate {
            port: Some(2346),
            host: None,
            timeout: None,
        });
        assert_eq!(
            result.debug_scenarios,
            DebugTaskFile(vec![
                scenario(
                    "API server",
                    "Debugpy",
                    json!({
                        "request": "launch",
                        "program": "${ZZZ_WORKTREE_ROOT}/server/api/main.py",
                        "args": ["--port", "8080", "--title", "My API"],
                        "cwd": "${ZZZ_WORKTREE_ROOT}/server/api",
                        "env": {
                            "PYTHONUNBUFFERED": "1",
                            "SERVICE_ENV_FILE": "${ZZZ_WORKTREE_ROOT}/hack/service.env",
                        },
                    }),
                ),
                remote,
                scenario(
                    "controller [kube]",
                    "Delve",
                    json!({
                        "request": "launch",
                        "mode": "debug",
                        "program": "${ZZZ_WORKTREE_ROOT}/cmd/controller",
                        "args": [
                            "--platform-config",
                            "hack/platform.yaml",
                            "--kubeconfig-path",
                            "/home/test/.kube/config",
                        ],
                        "cwd": "${ZZZ_WORKTREE_ROOT}",
                        "env": { "APP_ENV": "dev" },
                        "buildFlags": "-tags=kube -race",
                    }),
                ),
                scenario(
                    "cli",
                    "Debugpy",
                    json!({
                        "request": "launch",
                        "module": "tools.cli",
                        "envFile": "${ZZZ_WORKTREE_ROOT}/.env",
                        "python": "/home/test/.venvs/tools/bin/python",
                        "pythonArgs": ["-X", "dev"],
                    }),
                ),
                scenario(
                    "parser tests",
                    "Delve",
                    json!({
                        "request": "launch",
                        "mode": "test",
                        "program": "${ZZZ_WORKTREE_ROOT}/internal/parser",
                        "args": ["-test.run", r"^\QTestParse\E$", "-test.v"],
                        "cwd": "${ZZZ_WORKTREE_ROOT}/internal/parser",
                        "buildFlags": "-count=1",
                    }),
                ),
            ])
        );

        let imported = result
            .report
            .imported
            .iter()
            .map(|configuration| {
                (
                    configuration.name.as_str(),
                    configuration.kind.as_str(),
                    configuration.adapter.as_str(),
                    configuration.source.as_str(),
                    configuration.warnings.len(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            imported,
            vec![
                (
                    "API server",
                    "Python",
                    "Debugpy",
                    ".idea/runConfigurations/API_server.xml",
                    1
                ),
                (
                    "Debug agent",
                    "Go Remote",
                    "Delve",
                    ".idea/runConfigurations/Debug_agent.xml",
                    0
                ),
                (
                    "controller [kube]",
                    "Go Application",
                    "Delve",
                    ".idea/runConfigurations/controller__kube_.xml",
                    0
                ),
                ("cli", "Python", "Debugpy", ".idea/workspace.xml", 0),
                ("parser tests", "Go Test", "Delve", ".idea/workspace.xml", 1),
            ]
        );

        let skipped = result
            .report
            .skipped
            .iter()
            .map(|configuration| {
                (
                    configuration.name.as_str(),
                    configuration.type_id.as_str(),
                    configuration.source.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            skipped,
            vec![
                ("Main", "Application", ".idea/workspace.xml"),
                ("Attach JVM", "Remote", ".idea/workspace.xml"),
                (
                    "Everything",
                    "CompoundRunConfigurationType",
                    ".idea/workspace.xml"
                ),
            ]
        );
        assert!(
            result.report.skipped[2].reason.contains("API server")
                && result.report.skipped[2]
                    .reason
                    .contains("controller [kube]"),
            "compound reason should name its members: {}",
            result.report.skipped[2].reason
        );
        assert_eq!(result.report.notes, Vec::<String>::new());
    }

    #[test]
    fn test_module_dir_follows_intellij_for_modules_stored_in_idea() {
        let configuration = |module: &str| {
            format!(
                r#"<configuration name="run" type="PythonConfigurationType" factoryName="Python">
                    <module name="{module}" />
                    <option name="SDK_HOME" value="/usr/bin/python3" />
                    <option name="SCRIPT_NAME" value="$MODULE_DIR$/main.py" />
                </configuration>"#
            )
        };
        let modules_xml = r#"<project version="4">
            <component name="ProjectModuleManager">
              <modules>
                <module fileurl="file://$PROJECT_DIR$/.idea/service.iml" filepath="$PROJECT_DIR$/.idea/service.iml" />
                <module fileurl="file://$PROJECT_DIR$/tools/tools.iml" filepath="$PROJECT_DIR$/tools/tools.iml" />
              </modules>
            </component>
          </project>"#;

        for (module, expected_program) in [
            // IntelliJ treats a module file stored in `.idea` as rooted at the project.
            ("service", "${ZZZ_WORKTREE_ROOT}/main.py"),
            ("tools", "${ZZZ_WORKTREE_ROOT}/tools/main.py"),
        ] {
            let mut project = single_configuration_project(&configuration(module));
            project.modules_xml = Some(modules_xml.to_owned());
            let result = import(&project);
            assert_eq!(
                result.debug_scenarios.0[0].config["program"], expected_program,
                "module {module}"
            );
            assert_eq!(result.report.imported[0].warnings, Vec::<String>::new());
        }
    }

    #[test]
    fn test_project_dir_macro_in_a_nested_project() {
        let mut project = single_configuration_project(
            r#"<configuration name="run" type="PythonConfigurationType" factoryName="Python">
                <option name="SCRIPT_NAME" value="$PROJECT_DIR$/main.py" />
                <option name="SDK_HOME" value="/usr/bin/python3" />
            </configuration>"#,
        );
        project.directory_in_worktree = "services/billing".to_owned();
        let result = import(&project);
        assert_eq!(
            result.debug_scenarios,
            DebugTaskFile(vec![scenario(
                "run",
                "Debugpy",
                json!({
                    "request": "launch",
                    "program": "${ZZZ_WORKTREE_ROOT}/services/billing/main.py",
                    "python": "/usr/bin/python3",
                }),
            )])
        );
        assert_eq!(
            result.report.imported[0].source,
            "services/billing/.idea/runConfigurations/config.xml"
        );
    }

    #[test]
    fn test_unresolved_macros_are_kept_and_reported() {
        let mut project = single_configuration_project(
            r#"<configuration name="run" type="PythonConfigurationType" factoryName="Python">
                <module name="missing" />
                <option name="SDK_HOME" value="/usr/bin/python3" />
                <option name="SCRIPT_NAME" value="$MODULE_DIR$/main.py" />
                <option name="PARAMETERS" value="--home $USER_HOME$ --shell $SHELL --custom $MyMacro$" />
            </configuration>"#,
        );
        project.user_home = None;
        let result = import(&project);
        assert_eq!(
            result.debug_scenarios,
            DebugTaskFile(vec![scenario(
                "run",
                "Debugpy",
                json!({
                    "request": "launch",
                    "program": "$MODULE_DIR$/main.py",
                    "args": ["--home", "$USER_HOME$", "--shell", "$SHELL", "--custom", "$MyMacro$"],
                    "python": "/usr/bin/python3",
                }),
            )])
        );
        let warnings = &result.report.imported[0].warnings;
        for unresolved in ["$MODULE_DIR$", "$USER_HOME$", "$MyMacro$"] {
            assert!(
                warnings.iter().any(|warning| warning.contains(unresolved)),
                "expected a warning about {unresolved}, got {warnings:?}"
            );
        }
        assert!(
            !warnings.iter().any(|warning| warning.contains("$SHELL")),
            "$SHELL is not an IntelliJ macro: {warnings:?}"
        );
    }

    #[test]
    fn test_python_options_that_cannot_be_replicated_are_reported() {
        let result = import(&single_configuration_project(
            r#"<configuration name="run" type="PythonConfigurationType" factoryName="Python">
                <option name="PARENT_ENVS" value="false" />
                <option name="SDK_HOME" value="" />
                <option name="SDK_NAME" value="Python 3.12 (service)" />
                <option name="IS_MODULE_SDK" value="false" />
                <option name="ENV_FILES" value="$PROJECT_DIR$/a.env;$PROJECT_DIR$/b.env" />
                <option name="SCRIPT_NAME" value="$PROJECT_DIR$/main.py" />
                <option name="REDIRECT_INPUT" value="true" />
                <option name="INPUT_FILE" value="$PROJECT_DIR$/input.txt" />
            </configuration>"#,
        ));
        assert_eq!(
            result.debug_scenarios,
            DebugTaskFile(vec![scenario(
                "run",
                "Debugpy",
                json!({
                    "request": "launch",
                    "program": "${ZZZ_WORKTREE_ROOT}/main.py",
                }),
            )])
        );
        let warnings = &result.report.imported[0].warnings;
        for expected in [
            "Python 3.12 (service)",
            "parent environment",
            "env files",
            "input",
        ] {
            assert!(
                warnings.iter().any(|warning| warning.contains(expected)),
                "expected a warning mentioning {expected:?}, got {warnings:?}"
            );
        }
    }

    #[test]
    fn test_python_configuration_without_script_is_skipped() {
        let result = import(&single_configuration_project(
            r#"<configuration name="empty" type="PythonConfigurationType" factoryName="Python">
                <option name="SCRIPT_NAME" value="" />
            </configuration>"#,
        ));
        assert_eq!(result.debug_scenarios, DebugTaskFile(Vec::new()));
        assert_eq!(result.report.skipped[0].name, "empty");
    }

    #[test]
    fn test_go_package_without_go_mod_keeps_the_import_path() {
        let result = import(&single_configuration_project(
            r#"<configuration name="server" type="GoApplicationRunConfiguration" factoryName="Go Application">
                <kind value="PACKAGE" />
                <package value="example.com/server/cmd/server" />
            </configuration>"#,
        ));
        assert_eq!(
            result.debug_scenarios,
            DebugTaskFile(vec![scenario(
                "server",
                "Delve",
                json!({
                    "request": "launch",
                    "mode": "debug",
                    "program": "example.com/server/cmd/server",
                }),
            )])
        );
        assert!(result.report.imported[0].warnings[0].contains("go.mod"));
    }

    #[test]
    fn test_go_file_kind_uses_the_file() {
        let result = import(&single_configuration_project(
            r#"<configuration name="tool" type="GoApplicationRunConfiguration" factoryName="Go Application">
                <kind value="FILE" />
                <filePath value="$PROJECT_DIR$/tools/gen/main.go" />
            </configuration>"#,
        ));
        assert_eq!(
            result.debug_scenarios,
            DebugTaskFile(vec![scenario(
                "tool",
                "Delve",
                json!({
                    "request": "launch",
                    "mode": "debug",
                    "program": "${ZZZ_WORKTREE_ROOT}/tools/gen/main.go",
                }),
            )])
        );
    }

    #[test]
    fn test_go_remote_hosts() {
        let localhost = import(&single_configuration_project(
            r#"<configuration name="local" type="GoRemoteDebugConfigurationType" factoryName="Go Remote" host="localhost" port="40000" />"#,
        ));
        assert_eq!(
            localhost.debug_scenarios.0[0].tcp_connection,
            Some(TcpArgumentsTemplate {
                port: Some(40000),
                host: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                timeout: None,
            })
        );

        let named_host = import(&single_configuration_project(
            r#"<configuration name="box" type="GoRemoteDebugConfigurationType" factoryName="Go Remote" host="build-box" port="2345" />"#,
        ));
        assert_eq!(named_host.debug_scenarios, DebugTaskFile(Vec::new()));
        assert!(named_host.report.skipped[0].reason.contains("build-box"));

        let without_port = import(&single_configuration_project(
            r#"<configuration name="no port" type="GoRemoteDebugConfigurationType" factoryName="Go Remote" host="" />"#,
        ));
        assert_eq!(without_port.debug_scenarios, DebugTaskFile(Vec::new()));
        assert_eq!(without_port.report.skipped[0].name, "no port");
    }

    #[test]
    fn test_unknown_configuration_types_are_reported() {
        let result = import(&single_configuration_project(
            r#"<configuration name="lint" type="ShConfigurationType">
                <option name="SCRIPT_TEXT" value="make lint" />
            </configuration>"#,
        ));
        assert_eq!(result.debug_scenarios, DebugTaskFile(Vec::new()));
        assert_eq!(result.report.skipped[0].type_id, "ShConfigurationType");
        assert!(
            result.report.skipped[0]
                .reason
                .contains("ShConfigurationType")
        );
    }

    #[test]
    fn test_missing_and_malformed_files_are_reported() {
        let mut project = single_configuration_project("<configuration name=\"broken\"");
        project.workspace_xml = None;
        let result = import(&project);
        assert_eq!(result.debug_scenarios, DebugTaskFile(Vec::new()));
        assert!(
            result
                .report
                .notes
                .iter()
                .any(|note| note.contains(".idea/runConfigurations/config.xml")),
            "malformed file should be reported: {:?}",
            result.report.notes
        );
        assert!(
            result
                .report
                .notes
                .iter()
                .any(|note| note.contains("workspace.xml")),
            "missing workspace.xml should be reported: {:?}",
            result.report.notes
        );

        let empty = import(&IdeaProject {
            workspace_xml: Some(r#"<project version="4"></project>"#.to_owned()),
            ..IdeaProject::default()
        });
        assert!(
            empty
                .report
                .notes
                .iter()
                .any(|note| note.contains("No run configurations")),
            "an empty project should say so: {:?}",
            empty.report.notes
        );
    }

    #[test]
    fn test_report_markdown() {
        let report = ImportReport {
            imported: vec![ImportedConfiguration {
                name: "API server".to_owned(),
                kind: "Python".to_owned(),
                adapter: "Debugpy".to_owned(),
                source: ".idea/runConfigurations/API_server.xml".to_owned(),
                warnings: vec!["Pick the interpreter in ZZZ.".to_owned()],
            }],
            skipped: vec![SkippedConfiguration {
                name: "Main".to_owned(),
                type_id: "Application".to_owned(),
                source: ".idea/workspace.xml".to_owned(),
                reason: "Needs a Java debug adapter.".to_owned(),
            }],
            notes: vec!["`.idea/workspace.xml` was not found.".to_owned()],
        };
        assert_eq!(
            report.to_markdown(),
            "# `.idea` import report\n\
             \n\
             ZZZ imported these run configurations from `.idea` without modifying it. \
             Edit `debug.json` in this folder to adjust them; ZZZ never overwrites it. \
             To import again, delete this folder.\n\
             \n\
             ## Imported (1)\n\
             \n\
             - **API server** (Python, Debugpy), from `.idea/runConfigurations/API_server.xml`\n  \
             - Pick the interpreter in ZZZ.\n\
             \n\
             ## Not imported (1)\n\
             \n\
             - **Main** (`Application`), from `.idea/workspace.xml`: Needs a Java debug adapter.\n\
             \n\
             ## Notes\n\
             \n\
             - `.idea/workspace.xml` was not found.\n"
        );
    }
}
