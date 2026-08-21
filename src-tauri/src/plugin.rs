use crate::error::{KfResult, LocalizedError};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use std::{
    collections::{BTreeMap, HashSet},
    path::{Component, Path},
};

pub const PLUGIN_PROTOCOL_VERSION: &str = "knightframe.plugin.v1";
pub const DSH_ADAPTER_DATA_VERSION: &str = "knightframe.dsh.adapter.v1";
pub const DSH_DEFINE_TOOL: &str = "cordis_define";
pub const DSH_HOST_RUNNER_PACKAGE: &str = "@deepseek-ai/dsh-cordis-host-runner";
pub const DSH_CLIENT_RUNNER_PACKAGE: &str = "@deepseek-ai/dsh-cordis-client-runner";
pub const DSH_DYNAMIC_CORDIS_YAML: &str = "# Studio UI is a process-local dynamic Package.\n# Submit cordis-define-arguments.json through cordis_define, then use the returned IDs with cordis_run.\n[]\n";
pub const MAX_JSONL_FRAME_BYTES: usize = 1024 * 1024;
pub const CANVAS_MAX: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginRuntime {
    Rust,
    Node,
    Python,
    Command,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StudioTarget {
    #[default]
    Knightframe,
    Dsh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginPermission {
    Ui,
    UiController,
    WorkspaceRead,
    WorkspaceWrite,
    Process,
    Network,
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginText {
    Plain(String),
    Localized(LocalizedPluginText),
}

impl PluginText {
    fn primary(&self) -> &str {
        match self {
            Self::Plain(value) => value,
            Self::Localized(value) => value
                .values
                .get(&value.default_locale)
                .map(String::as_str)
                .unwrap_or_default(),
        }
    }

    fn validate(&self, field: &str) -> KfResult<()> {
        match self {
            Self::Plain(value) => validate_text(value, field),
            Self::Localized(value) => value.validate(field),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalizedPluginText {
    pub default_locale: String,
    pub values: BTreeMap<String, String>,
}

impl LocalizedPluginText {
    fn validate(&self, field: &str) -> KfResult<()> {
        if !valid_locale(&self.default_locale)
            || !self.values.contains_key(&self.default_locale)
            || self.values.is_empty()
        {
            return Err(LocalizedError::new("error.plugin_text_locale").arg("field", field));
        }
        for (locale, value) in &self.values {
            if !valid_locale(locale) {
                return Err(LocalizedError::new("error.plugin_text_locale")
                    .arg("field", field)
                    .arg("locale", locale));
            }
            validate_text(value, field)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub protocol_version: String,
    pub id: String,
    pub name: PluginText,
    pub version: String,
    pub runtime: PluginRuntime,
    pub entry: String,
    #[serde(default = "default_config_schema")]
    pub config_schema: Value,
    #[serde(default)]
    pub inject: Vec<String>,
    #[serde(default)]
    pub provide: Vec<String>,
    #[serde(default)]
    pub intercept: BTreeMap<String, Value>,
    #[serde(default)]
    pub isolate: BTreeMap<String, IsolationBinding>,
    #[serde(default)]
    pub tools: Vec<PluginTool>,
    #[serde(default)]
    pub ui: Vec<UiContribution>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub style_css: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<PluginControllerSource>,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
}

fn default_config_schema() -> Value {
    json!({ "type": "object" })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IsolationBinding {
    Local(bool),
    Shared(String),
}

impl IsolationBinding {
    fn validate(&self, service: &str) -> KfResult<()> {
        match self {
            Self::Local(true) => Ok(()),
            Self::Local(false) => Err(LocalizedError::new("error.plugin_isolate")
                .arg("service", service)
                .arg("reason", "false")),
            Self::Shared(label) if valid_identifier(label, 128) => Ok(()),
            Self::Shared(_) => {
                Err(LocalizedError::new("error.plugin_isolate").arg("service", service))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UiComponentKind {
    Button,
    Toggle,
    Text,
    Input,
    Select,
    Separator,
    Panel,
    Container,
    Grid,
    Markdown,
    Image,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginControllerLanguage {
    JavaScript,
    TypeScript,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginControllerSource {
    pub language: PluginControllerLanguage,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiDataBinding {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum UiAction {
    Command {
        command: String,
        #[serde(default)]
        arguments: Value,
    },
    SetData {
        path: String,
        value: Value,
    },
    ToggleData {
        path: String,
    },
    Emit {
        event: String,
        #[serde(default)]
        payload: Value,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiComponentExtension {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub style: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, UiDataBinding>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub actions: BTreeMap<String, Vec<UiAction>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CanvasBounds {
    pub fn validate(&self) -> KfResult<()> {
        let x_end = self.x.checked_add(self.width);
        let y_end = self.y.checked_add(self.height);
        if self.width == 0
            || self.height == 0
            || self.x > CANVAS_MAX
            || self.y > CANVAS_MAX
            || x_end.is_none_or(|value| value > CANVAS_MAX)
            || y_end.is_none_or(|value| value > CANVAS_MAX)
        {
            return Err(LocalizedError::new("error.plugin_ui_bounds"));
        }
        Ok(())
    }

    pub fn clamped(self) -> Self {
        let x = self.x.min(CANVAS_MAX - 1);
        let y = self.y.min(CANVAS_MAX - 1);
        Self {
            x,
            y,
            width: self.width.max(1).min(CANVAS_MAX - x),
            height: self.height.max(1).min(CANVAS_MAX - y),
        }
    }

    pub fn to_pixels(self, viewport: PixelViewport) -> KfResult<PixelBounds> {
        viewport.validate()?;
        self.validate()?;
        let (x, width) = normalized_axis(self.x, self.width, viewport.width);
        let (y, height) = normalized_axis(self.y, self.height, viewport.height);
        Ok(PixelBounds {
            x,
            y,
            width,
            height,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PixelViewport {
    pub width: u32,
    pub height: u32,
}

impl Default for PixelViewport {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
        }
    }
}

impl PixelViewport {
    fn validate(self) -> KfResult<()> {
        if self.width == 0 || self.height == 0 {
            return Err(LocalizedError::new("error.plugin_viewport"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PixelBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelBounds {
    pub fn to_canvas(self, viewport: PixelViewport) -> KfResult<CanvasBounds> {
        viewport.validate()?;
        let (x, width) = pixel_axis(self.x, self.width, viewport.width);
        let (y, height) = pixel_axis(self.y, self.height, viewport.height);
        Ok(CanvasBounds {
            x,
            y,
            width,
            height,
        }
        .clamped())
    }
}

fn scale_to_pixels(value: u32, extent: u32) -> u32 {
    ((u64::from(value) * u64::from(extent) + u64::from(CANVAS_MAX / 2)) / u64::from(CANVAS_MAX))
        as u32
}

fn scale_to_canvas(value: u32, extent: u32) -> u32 {
    ((u64::from(value) * u64::from(CANVAS_MAX) + u64::from(extent / 2)) / u64::from(extent)) as u32
}

fn normalized_axis(start: u32, size: u32, extent: u32) -> (u32, u32) {
    let mut pixel_start = scale_to_pixels(start, extent).min(extent.saturating_sub(1));
    let mut pixel_end = scale_to_pixels(start + size, extent).min(extent);
    if pixel_end <= pixel_start {
        if pixel_start < extent {
            pixel_end = pixel_start + 1;
        } else {
            pixel_start = extent - 1;
            pixel_end = extent;
        }
    }
    (pixel_start, pixel_end - pixel_start)
}

fn pixel_axis(start: u32, size: u32, extent: u32) -> (u32, u32) {
    let mut pixel_start = start.min(extent - 1);
    let mut pixel_end = start.saturating_add(size).min(extent);
    if pixel_end <= pixel_start {
        if pixel_start < extent {
            pixel_end = pixel_start + 1;
        } else {
            pixel_start = extent - 1;
            pixel_end = extent;
        }
    }
    let canvas_start = scale_to_canvas(pixel_start, extent).min(CANVAS_MAX - 1);
    let canvas_end = scale_to_canvas(pixel_end, extent)
        .max(canvas_start + 1)
        .min(CANVAS_MAX);
    (canvas_start, canvas_end - canvas_start)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UiContribution {
    Button(ButtonComponent),
    Toggle(ToggleComponent),
    Text(TextComponent),
    Input(InputComponent),
    Select(SelectComponent),
    Separator(SeparatorComponent),
    Panel(PanelComponent),
    Container(ContainerComponent),
    Grid(GridComponent),
    Markdown(MarkdownComponent),
    Image(ImageComponent),
    Code(CodeComponent),
}

macro_rules! component_base {
    ($name:ident { $($field:tt)* }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name {
            pub id: String,
            pub slot: String,
            pub bounds: CanvasBounds,
            #[serde(flatten)]
            pub extension: UiComponentExtension,
            $($field)*
        }
    };
}

component_base!(ButtonComponent {
    pub label: PluginText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub disabled: bool,
});

component_base!(ToggleComponent {
    pub label: PluginText,
    #[serde(default)]
    pub value: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub disabled: bool,
});

component_base!(TextComponent {
    pub text: PluginText,
});

component_base!(InputComponent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<PluginText>,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub disabled: bool,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectOption {
    pub value: String,
    pub label: PluginText,
}

component_base!(SelectComponent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<PluginText>,
    pub options: Vec<SelectOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub disabled: bool,
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeparatorOrientation {
    Horizontal,
    Vertical,
}

component_base!(SeparatorComponent {
    #[serde(default = "default_separator_orientation")]
    pub orientation: SeparatorOrientation,
});

fn default_separator_orientation() -> SeparatorOrientation {
    SeparatorOrientation::Horizontal
}

component_base!(PanelComponent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PluginText>,
    #[serde(default)]
    pub elevated: bool,
});

component_base!(ContainerComponent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<PluginText>,
    #[serde(default)]
    pub children: Vec<String>,
});

component_base!(GridComponent {
    #[serde(default = "default_grid_columns")]
    pub columns: u16,
    #[serde(default)]
    pub gap: u16,
    #[serde(default)]
    pub children: Vec<String>,
});

fn default_grid_columns() -> u16 {
    2
}

component_base!(MarkdownComponent {
    pub content: String,
});

component_base!(ImageComponent {
    pub source: String,
    #[serde(default)]
    pub alt: String,
});

component_base!(CodeComponent {
    pub code: String,
    #[serde(default)]
    pub language: String,
});

impl UiContribution {
    pub fn id(&self) -> &str {
        match self {
            Self::Button(value) => &value.id,
            Self::Toggle(value) => &value.id,
            Self::Text(value) => &value.id,
            Self::Input(value) => &value.id,
            Self::Select(value) => &value.id,
            Self::Separator(value) => &value.id,
            Self::Panel(value) => &value.id,
            Self::Container(value) => &value.id,
            Self::Grid(value) => &value.id,
            Self::Markdown(value) => &value.id,
            Self::Image(value) => &value.id,
            Self::Code(value) => &value.id,
        }
    }

    pub fn slot(&self) -> &str {
        match self {
            Self::Button(value) => &value.slot,
            Self::Toggle(value) => &value.slot,
            Self::Text(value) => &value.slot,
            Self::Input(value) => &value.slot,
            Self::Select(value) => &value.slot,
            Self::Separator(value) => &value.slot,
            Self::Panel(value) => &value.slot,
            Self::Container(value) => &value.slot,
            Self::Grid(value) => &value.slot,
            Self::Markdown(value) => &value.slot,
            Self::Image(value) => &value.slot,
            Self::Code(value) => &value.slot,
        }
    }

    pub fn bounds(&self) -> CanvasBounds {
        match self {
            Self::Button(value) => value.bounds,
            Self::Toggle(value) => value.bounds,
            Self::Text(value) => value.bounds,
            Self::Input(value) => value.bounds,
            Self::Select(value) => value.bounds,
            Self::Separator(value) => value.bounds,
            Self::Panel(value) => value.bounds,
            Self::Container(value) => value.bounds,
            Self::Grid(value) => value.bounds,
            Self::Markdown(value) => value.bounds,
            Self::Image(value) => value.bounds,
            Self::Code(value) => value.bounds,
        }
    }

    pub fn kind(&self) -> UiComponentKind {
        match self {
            Self::Button(_) => UiComponentKind::Button,
            Self::Toggle(_) => UiComponentKind::Toggle,
            Self::Text(_) => UiComponentKind::Text,
            Self::Input(_) => UiComponentKind::Input,
            Self::Select(_) => UiComponentKind::Select,
            Self::Separator(_) => UiComponentKind::Separator,
            Self::Panel(_) => UiComponentKind::Panel,
            Self::Container(_) => UiComponentKind::Container,
            Self::Grid(_) => UiComponentKind::Grid,
            Self::Markdown(_) => UiComponentKind::Markdown,
            Self::Image(_) => UiComponentKind::Image,
            Self::Code(_) => UiComponentKind::Code,
        }
    }

    fn extension(&self) -> &UiComponentExtension {
        match self {
            Self::Button(value) => &value.extension,
            Self::Toggle(value) => &value.extension,
            Self::Text(value) => &value.extension,
            Self::Input(value) => &value.extension,
            Self::Select(value) => &value.extension,
            Self::Separator(value) => &value.extension,
            Self::Panel(value) => &value.extension,
            Self::Container(value) => &value.extension,
            Self::Grid(value) => &value.extension,
            Self::Markdown(value) => &value.extension,
            Self::Image(value) => &value.extension,
            Self::Code(value) => &value.extension,
        }
    }

    fn children(&self) -> &[String] {
        match self {
            Self::Container(value) => &value.children,
            Self::Grid(value) => &value.children,
            _ => &[],
        }
    }

    fn validate(&self, target: StudioTarget) -> KfResult<()> {
        if !valid_identifier(self.id(), 128) {
            return Err(LocalizedError::new("error.plugin_ui_id").arg("id", self.id()));
        }
        if !valid_identifier(self.slot(), 128) {
            return Err(LocalizedError::new("error.plugin_ui_slot").arg("slot", self.slot()));
        }
        if target == StudioTarget::Dsh {
            let runtime_slot = map_to_dsh_slot(target, self.slot());
            let safe_slot = dsh_slot_descriptor(&runtime_slot).is_some_and(|descriptor| {
                descriptor.replace_risk == DshReplaceRisk::None
                    && matches!(descriptor.kind, DshSlotKind::List | DshSlotKind::Keyed)
            });
            if !safe_slot {
                return Err(LocalizedError::new("error.plugin_ui_slot").arg("slot", runtime_slot));
            }
        }
        self.bounds()
            .validate()
            .map_err(|error| error.arg("id", self.id()))?;
        validate_component_extension(self.extension())?;
        match self {
            Self::Button(value) => {
                value.label.validate("label")?;
                validate_command(&value.command)?;
            }
            Self::Toggle(value) => {
                value.label.validate("label")?;
                validate_command(&value.command)?;
            }
            Self::Text(value) => value.text.validate("text")?,
            Self::Input(value) => {
                if let Some(label) = &value.label {
                    label.validate("label")?;
                }
                validate_optional_text(&value.placeholder, "placeholder")?;
                validate_optional_text(&value.value, "value")?;
                validate_command(&value.command)?;
            }
            Self::Select(value) => {
                if let Some(label) = &value.label {
                    label.validate("label")?;
                }
                if value.options.is_empty() || value.options.len() > 100 {
                    return Err(
                        LocalizedError::new("error.plugin_ui_select_options").arg("id", &value.id)
                    );
                }
                let mut option_values = HashSet::new();
                for option in &value.options {
                    if !valid_identifier(&option.value, 128)
                        || !option_values.insert(option.value.as_str())
                    {
                        return Err(LocalizedError::new("error.plugin_ui_select_options")
                            .arg("id", &value.id));
                    }
                    option.label.validate("optionLabel")?;
                }
                if value
                    .value
                    .as_ref()
                    .is_some_and(|selected| !option_values.contains(selected.as_str()))
                {
                    return Err(
                        LocalizedError::new("error.plugin_ui_select_value").arg("id", &value.id)
                    );
                }
                validate_command(&value.command)?;
            }
            Self::Separator(_) => {}
            Self::Panel(value) => {
                if let Some(title) = &value.title {
                    title.validate("title")?;
                }
            }
            Self::Container(value) => {
                if let Some(title) = &value.title {
                    title.validate("title")?;
                }
                validate_child_ids(&value.children)?;
            }
            Self::Grid(value) => {
                if value.columns == 0 || value.columns > 24 || value.gap > 512 {
                    return Err(LocalizedError::new("error.plugin_ui_bounds").arg("id", &value.id));
                }
                validate_child_ids(&value.children)?;
            }
            Self::Markdown(value) => validate_text_block(&value.content, "markdown")?,
            Self::Image(value) => {
                validate_image_source(&value.source)?;
                validate_optional_text(&value.alt, "alt")?;
            }
            Self::Code(value) => {
                validate_text_block(&value.code, "code")?;
                if !value.language.is_empty() && !valid_identifier(&value.language, 64) {
                    return Err(
                        LocalizedError::new("error.plugin_ui_text").arg("field", "language")
                    );
                }
            }
        }
        Ok(())
    }
}

fn map_to_dsh_slot(_target: StudioTarget, slot: &str) -> String {
    match slot {
        "tool.view.cordis" | "tool.view.plugin" => "tool.view.cordis",
        "shell.overlay" | "workspace.overlay" | "conversation.before" | "conversation.after" => {
            "shell.overlay"
        }
        "composer.before" => "conversation.input.left",
        "header.actions" => "conversation.session.header.actions",
        value => value,
    }
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DshSlotKind {
    Single,
    List,
    Keyed,
    Chain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DshSlotScope {
    Root,
    Session,
    SessionMaybe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DshReplaceRisk {
    None,
    ShadowsShippedUi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshSlotDescriptor {
    pub name: &'static str,
    pub kind: DshSlotKind,
    pub scope: DshSlotScope,
    pub replace_risk: DshReplaceRisk,
}

macro_rules! dsh_slot {
    ($name:literal, $kind:ident, $scope:ident, $risk:ident) => {
        DshSlotDescriptor {
            name: $name,
            kind: DshSlotKind::$kind,
            scope: DshSlotScope::$scope,
            replace_risk: DshReplaceRisk::$risk,
        }
    };
}

pub const DSH_SLOT_CATALOG: [DshSlotDescriptor; 42] = [
    dsh_slot!("conversation", Single, SessionMaybe, ShadowsShippedUi),
    dsh_slot!("conversation.chat.assistant-actions", List, Session, None),
    dsh_slot!("conversation.chat.commandview", Keyed, Session, None),
    dsh_slot!("conversation.chat.node", Keyed, Session, ShadowsShippedUi),
    dsh_slot!("conversation.chat.turnTail", Chain, Session, None),
    dsh_slot!("conversation.composer", Chain, Session, None),
    dsh_slot!(
        "conversation.composer.bar",
        Single,
        SessionMaybe,
        ShadowsShippedUi
    ),
    dsh_slot!("conversation.composer.dock", List, Session, None),
    dsh_slot!(
        "conversation.details.tool",
        Single,
        Session,
        ShadowsShippedUi
    ),
    dsh_slot!(
        "conversation.hero.agentPreset",
        Single,
        Root,
        ShadowsShippedUi
    ),
    dsh_slot!(
        "conversation.hero.workspace",
        Single,
        Root,
        ShadowsShippedUi
    ),
    dsh_slot!(
        "conversation.hero.workspace.directoryFlow",
        Single,
        Root,
        ShadowsShippedUi
    ),
    dsh_slot!("conversation.input.dock", List, Session, None),
    dsh_slot!("conversation.input.left", List, Session, None),
    dsh_slot!(
        "conversation.input.model",
        Single,
        Session,
        ShadowsShippedUi
    ),
    dsh_slot!("conversation.input.overlay", List, Session, None),
    dsh_slot!("conversation.input.plan", Single, Session, ShadowsShippedUi),
    dsh_slot!("conversation.input.right", List, Session, None),
    dsh_slot!("conversation.session", Single, Session, ShadowsShippedUi),
    dsh_slot!(
        "conversation.session.header",
        Single,
        Session,
        ShadowsShippedUi
    ),
    dsh_slot!("conversation.session.header.actions", List, Session, None),
    dsh_slot!("conversation.session.header.utilities", List, Session, None),
    dsh_slot!("conversation.view", List, Session, None),
    dsh_slot!("details", Single, Session, ShadowsShippedUi),
    dsh_slot!("root", Single, Root, ShadowsShippedUi),
    dsh_slot!("settings.action", List, Root, None),
    dsh_slot!("settings.close", Single, Root, ShadowsShippedUi),
    dsh_slot!("settings.general.item", List, Root, None),
    dsh_slot!("settings.header", Single, Root, ShadowsShippedUi),
    dsh_slot!("settings.onboarding", List, Root, None),
    dsh_slot!("settings.plugin.item", List, Root, None),
    dsh_slot!("settings.plugins.tab", List, Root, None),
    dsh_slot!("settings.section", List, Root, None),
    dsh_slot!("settings.trigger", Single, Root, ShadowsShippedUi),
    dsh_slot!("shell.overlay", List, Root, None),
    dsh_slot!("sidebar", Single, Root, ShadowsShippedUi),
    dsh_slot!("sidebar.footer.action", List, Root, None),
    dsh_slot!("sidebar.settings", Single, Root, ShadowsShippedUi),
    dsh_slot!("sidebar.workspaces", Single, Root, ShadowsShippedUi),
    dsh_slot!(
        "sidebar.workspaces.directoryFlow",
        Single,
        Root,
        ShadowsShippedUi
    ),
    dsh_slot!("tool.call.toolview", Keyed, Session, ShadowsShippedUi),
    dsh_slot!("tool.view.cordis", Keyed, Session, None),
];

fn dsh_slot_descriptor(slot: &str) -> Option<DshSlotDescriptor> {
    DSH_SLOT_CATALOG
        .iter()
        .copied()
        .find(|descriptor| descriptor.name == slot)
}

fn validate_child_ids(children: &[String]) -> KfResult<()> {
    let mut unique = HashSet::new();
    for child in children {
        if !valid_identifier(child, 128) || !unique.insert(child) {
            return Err(LocalizedError::new("error.plugin_ui_id").arg("id", child));
        }
    }
    Ok(())
}

fn validate_component_extension(extension: &UiComponentExtension) -> KfResult<()> {
    for (name, value) in &extension.props {
        if !valid_identifier(name, 64) || value.to_string().len() > 16_384 {
            return Err(LocalizedError::new("error.plugin_ui_text").arg("field", "props"));
        }
    }
    for (property, value) in &extension.style {
        if !valid_css_property(property) || value.len() > 1024 || unsafe_css(value) {
            return Err(LocalizedError::new("error.plugin_ui_text").arg("field", "style"));
        }
    }
    for (property, binding) in &extension.bindings {
        if !valid_identifier(property, 64) || !valid_data_path(&binding.path) {
            return Err(LocalizedError::new("error.plugin_ui_text").arg("field", "bindings"));
        }
    }
    for (event, actions) in &extension.actions {
        if !valid_identifier(event, 64) || actions.is_empty() {
            return Err(LocalizedError::new("error.plugin_ui_command").arg("event", event));
        }
        for action in actions {
            validate_action(action)?;
        }
    }
    Ok(())
}

fn validate_action(action: &UiAction) -> KfResult<()> {
    match action {
        UiAction::Command { command, arguments } => {
            if !valid_identifier(command, 128) || arguments.to_string().len() > 16_384 {
                return Err(LocalizedError::new("error.plugin_ui_command"));
            }
        }
        UiAction::SetData { path, value } => {
            if !valid_data_path(path) || value.to_string().len() > 16_384 {
                return Err(LocalizedError::new("error.plugin_ui_command"));
            }
        }
        UiAction::ToggleData { path } => {
            if !valid_data_path(path) {
                return Err(LocalizedError::new("error.plugin_ui_command"));
            }
        }
        UiAction::Emit { event, payload } => {
            if !valid_identifier(event, 128) || payload.to_string().len() > 16_384 {
                return Err(LocalizedError::new("error.plugin_ui_command"));
            }
        }
    }
    Ok(())
}

fn valid_data_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 256
        && path
            .split('.')
            .all(|segment| valid_identifier(segment, 64) && !segment.starts_with('-'))
}

fn valid_css_property(property: &str) -> bool {
    !property.is_empty()
        && property.len() <= 64
        && property
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn unsafe_css(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "url(",
        "expression(",
        "javascript:",
        "@import",
        "behavior:",
        "-moz-binding",
    ]
    .iter()
    .any(|token| value.contains(token))
}

fn validate_text_block(value: &str, field: &str) -> KfResult<()> {
    if value.chars().count() > 262_144 {
        return Err(LocalizedError::new("error.plugin_ui_text").arg("field", field));
    }
    Ok(())
}

fn validate_image_source(source: &str) -> KfResult<()> {
    let lower = source.to_ascii_lowercase();
    if source.is_empty()
        || source.len() > 4096
        || lower.starts_with("javascript:")
        || lower.starts_with("file:")
        || lower.starts_with("http:")
    {
        return Err(LocalizedError::new("error.plugin_ui_text").arg("field", "imageSource"));
    }
    Ok(())
}

pub fn parse_manifest_json(input: &str, target: StudioTarget) -> KfResult<PluginManifest> {
    let manifest: PluginManifest = serde_json::from_str(input).map_err(|error| {
        LocalizedError::new("error.plugin_manifest_json")
            .arg("line", error.line())
            .arg("column", error.column())
    })?;
    validate_manifest(&manifest, target)?;
    Ok(manifest)
}

pub fn validate_manifest(manifest: &PluginManifest, target: StudioTarget) -> KfResult<()> {
    if manifest.protocol_version != PLUGIN_PROTOCOL_VERSION {
        return Err(
            LocalizedError::new("error.plugin_protocol").arg("expected", PLUGIN_PROTOCOL_VERSION)
        );
    }
    if !valid_identifier(&manifest.id, 128) {
        return Err(LocalizedError::new("error.plugin_id").arg("id", &manifest.id));
    }
    manifest.name.validate("name")?;
    Version::parse(&manifest.version).map_err(|_| {
        LocalizedError::new("error.plugin_version").arg("version", &manifest.version)
    })?;
    validate_entry_path(&manifest.entry)?;
    if !manifest.config_schema.is_object() {
        return Err(LocalizedError::new("error.plugin_config_schema"));
    }
    validate_unique_names(&manifest.inject, "inject")?;
    validate_unique_names(&manifest.provide, "provide")?;
    for name in manifest.intercept.keys() {
        validate_service_name(name, "intercept")?;
    }
    for (name, binding) in &manifest.isolate {
        validate_service_name(name, "isolate")?;
        binding.validate(name)?;
    }

    let mut tool_names = HashSet::new();
    for tool in &manifest.tools {
        if !valid_identifier(&tool.name, 128) || !tool_names.insert(tool.name.as_str()) {
            return Err(LocalizedError::new("error.plugin_tool_name").arg("name", &tool.name));
        }
        validate_text(&tool.description, "toolDescription")?;
        if !tool.input_schema.is_object()
            || tool
                .output_schema
                .as_ref()
                .is_some_and(|schema| !schema.is_object())
        {
            return Err(LocalizedError::new("error.plugin_tool_schema").arg("name", &tool.name));
        }
    }

    let mut permissions = HashSet::new();
    for permission in &manifest.permissions {
        if !permissions.insert(*permission) {
            return Err(LocalizedError::new("error.plugin_permission_duplicate"));
        }
    }

    validate_style_source(&manifest.style_css)?;
    if let Some(controller) = &manifest.controller {
        if !permissions.contains(&PluginPermission::UiController) {
            return Err(
                LocalizedError::new("error.plugin_permission_ui").arg("permission", "uiController")
            );
        }
        validate_controller_source(controller)?;
    }

    let mut component_ids = HashSet::new();
    for component in &manifest.ui {
        if !component_ids.insert(component.id()) {
            return Err(LocalizedError::new("error.plugin_ui_duplicate").arg("id", component.id()));
        }
        component.validate(target)?;
    }

    for component in &manifest.ui {
        for child in component.children() {
            if child == component.id() || !component_ids.contains(child.as_str()) {
                return Err(LocalizedError::new("error.plugin_ui_id").arg("id", child));
            }
        }
        if let UiContribution::Image(image) = component
            && image.source.to_ascii_lowercase().starts_with("https://")
            && !permissions.contains(&PluginPermission::Network)
        {
            return Err(
                LocalizedError::new("error.plugin_permission_ui").arg("permission", "network")
            );
        }
    }
    if !manifest.ui.is_empty() && !permissions.contains(&PluginPermission::Ui) {
        return Err(LocalizedError::new("error.plugin_permission_ui"));
    }
    Ok(())
}

fn validate_style_source(source: &str) -> KfResult<()> {
    if source.len() > 262_144 {
        return Err(LocalizedError::new("error.plugin_ui_text").arg("field", "styleCss"));
    }
    let lower = source.to_ascii_lowercase();
    if [
        "@import",
        "url(",
        "expression(",
        "javascript:",
        "behavior:",
        "-moz-binding",
        "html{",
        "html {",
        "body{",
        "body {",
        ":root",
        "*{",
        "* {",
    ]
    .iter()
    .any(|token| lower.contains(token))
    {
        return Err(LocalizedError::new("error.plugin_ui_text").arg("field", "styleCss"));
    }
    Ok(())
}

fn validate_controller_source(controller: &PluginControllerSource) -> KfResult<()> {
    let source = controller.source.trim();
    if source.is_empty() || source.len() > 262_144 {
        return Err(LocalizedError::new("error.plugin_ui_text").arg("field", "controller"));
    }
    let compact = source.to_ascii_lowercase();
    let forbidden = [
        "globalthis",
        "window",
        "document",
        "navigator",
        "location",
        "localstorage",
        "sessionstorage",
        "indexeddb",
        "xmlhttprequest",
        "websocket",
        "eventsource",
        "fetch(",
        "import(",
        "import ",
        "require(",
        "process.",
        "child_process",
        "node:",
        "deno.",
        "bun.",
        "eval(",
        "function(",
        "function (",
        ".constructor",
        "__proto__",
        ".prototype",
        "settimeout",
        "setinterval",
        "postmessage",
    ];
    if forbidden.iter().any(|token| compact.contains(token)) {
        return Err(LocalizedError::new("error.plugin_ui_command").arg("field", "controller"));
    }
    Ok(())
}

fn validate_entry_path(entry: &str) -> KfResult<()> {
    let path = Path::new(entry);
    let has_windows_prefix = entry.as_bytes().get(1) == Some(&b':');
    let has_root_prefix = entry.starts_with('/') || entry.starts_with('\\');
    let mut has_normal = false;
    let unsafe_component = path.components().any(|component| match component {
        Component::Normal(_) => {
            has_normal = true;
            false
        }
        Component::CurDir => false,
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => true,
    });
    if entry.is_empty()
        || entry.contains('\0')
        || path.is_absolute()
        || has_windows_prefix
        || has_root_prefix
        || unsafe_component
        || !has_normal
    {
        return Err(LocalizedError::new("error.plugin_entry"));
    }
    Ok(())
}

fn validate_unique_names(values: &[String], field: &str) -> KfResult<()> {
    let mut seen = HashSet::new();
    for value in values {
        validate_service_name(value, field)?;
        if !seen.insert(value.as_str()) {
            return Err(LocalizedError::new("error.plugin_service_duplicate")
                .arg("field", field)
                .arg("service", value));
        }
    }
    Ok(())
}

fn validate_service_name(value: &str, field: &str) -> KfResult<()> {
    if !valid_identifier(value, 128) {
        return Err(LocalizedError::new("error.plugin_service")
            .arg("field", field)
            .arg("service", value));
    }
    Ok(())
}

fn validate_command(command: &Option<String>) -> KfResult<()> {
    if command
        .as_ref()
        .is_some_and(|value| !valid_identifier(value, 128))
    {
        return Err(LocalizedError::new("error.plugin_ui_command"));
    }
    Ok(())
}

fn validate_text(value: &str, field: &str) -> KfResult<()> {
    if value.trim().is_empty() || value.chars().count() > 2048 {
        return Err(LocalizedError::new("error.plugin_text").arg("field", field));
    }
    Ok(())
}

fn validate_optional_text(value: &str, field: &str) -> KfResult<()> {
    if value.chars().count() > 2048 {
        return Err(LocalizedError::new("error.plugin_text").arg("field", field));
    }
    Ok(())
}

fn valid_identifier(value: &str, max: usize) -> bool {
    if value.is_empty() || value.len() > max {
        return false;
    }
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric() || first == '_')
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':' | '/')
        })
}

fn valid_locale(value: &str) -> bool {
    let mut segments = value.split('-');
    let Some(language) = segments.next() else {
        return false;
    };
    if !(2..=3).contains(&language.len())
        || !language.chars().all(|value| value.is_ascii_lowercase())
    {
        return false;
    }
    segments.all(|segment| {
        (2..=8).contains(&segment.len())
            && segment.chars().all(|value| value.is_ascii_alphanumeric())
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudioRequest {
    pub manifest_json: String,
    pub target: StudioTarget,
    #[serde(default)]
    pub viewport: PixelViewport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioPixelComponent {
    pub id: String,
    pub slot: String,
    pub kind: UiComponentKind,
    pub bounds: PixelBounds,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioValidation {
    pub valid: bool,
    pub target: StudioTarget,
    pub manifest: PluginManifest,
    pub layout: Vec<StudioPixelComponent>,
    pub capabilities: Vec<StudioCapability>,
    pub diagnostics: Vec<StudioDiagnostic>,
    pub dsh_slot_catalog: Vec<DshSlotDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityLevel {
    Native,
    Adapted,
    ExportOnly,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioCapability {
    pub feature: String,
    pub knightframe: CapabilityLevel,
    pub dsh: CapabilityLevel,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioDiagnostic {
    pub code: String,
    pub level: CapabilityLevel,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioExportPreview {
    pub target: StudioTarget,
    pub adapter_package: String,
    pub manifest_json: String,
    pub cordis_yaml: String,
    pub cordis_yaml_unavailable_reason: String,
    pub client_contribution_json: String,
    pub dsh_client_code: String,
    pub dsh_define_arguments_json: String,
    pub dsh_runtime: DshRuntimeRequirements,
    pub layout: Vec<StudioPixelComponent>,
    pub style_css: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_language: Option<PluginControllerLanguage>,
    pub capabilities: Vec<StudioCapability>,
    pub diagnostics: Vec<StudioDiagnostic>,
    pub dsh_slot_catalog: Vec<DshSlotDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshRuntimeRequirements {
    pub delivery: DshDelivery,
    pub define_tool: String,
    pub host_runner_package: String,
    pub client_runner_package: String,
    pub requires_client_approval: bool,
    pub process_local: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DshDelivery {
    CordisDefine,
}

fn validate_studio_request(request: StudioRequest) -> KfResult<StudioValidation> {
    request.viewport.validate()?;
    let manifest = parse_manifest_json(&request.manifest_json, request.target)?;
    let layout = manifest
        .ui
        .iter()
        .map(|component| {
            Ok(StudioPixelComponent {
                id: component.id().to_owned(),
                slot: component.slot().to_owned(),
                kind: component.kind(),
                bounds: component.bounds().to_pixels(request.viewport)?,
            })
        })
        .collect::<KfResult<Vec<_>>>()?;
    let capabilities = studio_capabilities();
    let diagnostics = studio_diagnostics(&manifest, request.target);
    Ok(StudioValidation {
        valid: true,
        target: request.target,
        manifest,
        layout,
        capabilities,
        diagnostics,
        dsh_slot_catalog: DSH_SLOT_CATALOG.to_vec(),
    })
}

fn studio_capabilities() -> Vec<StudioCapability> {
    vec![
        StudioCapability { feature: "declarativeUi".into(), knightframe: CapabilityLevel::Native, dsh: CapabilityLevel::Adapted, detail: "Shared IR lowers to native KnightFrame views or generated React.createElement source.".into() },
        StudioCapability { feature: "componentActions".into(), knightframe: CapabilityLevel::Native, dsh: CapabilityLevel::Adapted, detail: "DSH commands and private events require a generated host.call/harness.handle bridge.".into() },
        StudioCapability { feature: "styleCss".into(), knightframe: CapabilityLevel::Native, dsh: CapabilityLevel::Adapted, detail: "CSS is scoped to the plugin canvas and inserted through the host style service.".into() },
        StudioCapability { feature: "javascriptController".into(), knightframe: CapabilityLevel::Native, dsh: CapabilityLevel::Adapted, detail: "Controller source is permission-gated and runs only in a restricted target sandbox.".into() },
        StudioCapability { feature: "typescriptController".into(), knightframe: CapabilityLevel::Native, dsh: CapabilityLevel::ExportOnly, detail: "TypeScript is emitted for installable-package builds; dynamic cordis_define accepts JavaScript only.".into() },
        StudioCapability { feature: "browserPermission".into(), knightframe: CapabilityLevel::Native, dsh: CapabilityLevel::Unsupported, detail: "DSH dynamic Client forbids browser and network globals.".into() },
        StudioCapability { feature: "targetNativeEscapeHatch".into(), knightframe: CapabilityLevel::Native, dsh: CapabilityLevel::ExportOnly, detail: "Installable DSH ESM source may express owner props, selectors, stores, and child slots that shared IR cannot.".into() },
    ]
}

fn studio_diagnostics(manifest: &PluginManifest, target: StudioTarget) -> Vec<StudioDiagnostic> {
    let mut diagnostics = Vec::new();
    if target == StudioTarget::Dsh {
        for component in &manifest.ui {
            let runtime_slot = map_to_dsh_slot(target, component.slot());
            if runtime_slot != component.slot() {
                diagnostics.push(StudioDiagnostic {
                    code: "slot-adapted".into(),
                    level: CapabilityLevel::Adapted,
                    detail: format!("{} lowers to DSH slot {}", component.slot(), runtime_slot),
                    component_id: Some(component.id().into()),
                });
            }
            if dsh_slot_descriptor(&runtime_slot).is_none() {
                diagnostics.push(StudioDiagnostic {
                    code: "unknown-dsh-slot".into(),
                    level: CapabilityLevel::Unsupported,
                    detail: format!("{} is preserved in source but omitted from dynamic registration until the live DSH catalog confirms it", runtime_slot),
                    component_id: Some(component.id().into()),
                });
            }
        }
        if manifest
            .controller
            .as_ref()
            .is_some_and(|controller| controller.language == PluginControllerLanguage::TypeScript)
        {
            diagnostics.push(StudioDiagnostic {
                code: "typescript-dynamic-export-only".into(),
                level: CapabilityLevel::ExportOnly,
                detail:
                    "Dynamic DSH preview omits TypeScript until the installable package is built."
                        .into(),
                component_id: None,
            });
        }
        if manifest.permissions.contains(&PluginPermission::Browser) {
            diagnostics.push(StudioDiagnostic {
                code: "browser-permission-unsupported".into(),
                level: CapabilityLevel::Unsupported,
                detail:
                    "DSH Client exposes no browser/network global; use an approved Host service."
                        .into(),
                component_id: None,
            });
        }
    }
    diagnostics
}

#[tauri::command]
pub fn kf_plugin_studio_validate(request: StudioRequest) -> KfResult<StudioValidation> {
    validate_studio_request(request)
}

#[tauri::command]
pub fn kf_plugin_studio_export_preview(request: StudioRequest) -> KfResult<StudioExportPreview> {
    let validation = validate_studio_request(request)?;
    let manifest_json = serde_json::to_string_pretty(&validation.manifest)
        .map_err(|_| LocalizedError::new("error.plugin_manifest_encode"))?;
    let client_contribution_json =
        generate_dsh_client_contribution(&validation.manifest, validation.target)?;
    let dsh_client_code =
        generate_dsh_client_code(&client_contribution_json, &validation.manifest.style_css)?;
    let dsh_define_arguments_json =
        generate_dsh_define_arguments(&validation.manifest, &dsh_client_code)?;
    Ok(StudioExportPreview {
        target: validation.target,
        adapter_package: DSH_CLIENT_RUNNER_PACKAGE.into(),
        manifest_json,
        cordis_yaml: DSH_DYNAMIC_CORDIS_YAML.into(),
        cordis_yaml_unavailable_reason: "studio.dsh.dynamic_requires_cordis_define".into(),
        client_contribution_json,
        dsh_client_code,
        dsh_define_arguments_json,
        dsh_runtime: DshRuntimeRequirements {
            delivery: DshDelivery::CordisDefine,
            define_tool: DSH_DEFINE_TOOL.into(),
            host_runner_package: DSH_HOST_RUNNER_PACKAGE.into(),
            client_runner_package: DSH_CLIENT_RUNNER_PACKAGE.into(),
            requires_client_approval: true,
            process_local: true,
        },
        layout: validation.layout,
        style_css: validation.manifest.style_css.clone(),
        controller_source: validation
            .manifest
            .controller
            .as_ref()
            .map(|controller| controller.source.clone()),
        controller_language: validation
            .manifest
            .controller
            .as_ref()
            .map(|controller| controller.language),
        capabilities: validation.capabilities,
        diagnostics: validation.diagnostics,
        dsh_slot_catalog: validation.dsh_slot_catalog,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CordisEntry {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<YamlValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inject: Option<CordisInject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intercept: Option<BTreeMap<String, YamlValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolate: Option<BTreeMap<String, IsolationBinding>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CordisInject {
    Services(Vec<String>),
    Config(BTreeMap<String, YamlValue>),
}

pub fn parse_cordis_yaml(input: &str) -> KfResult<Vec<CordisEntry>> {
    let entries: Vec<CordisEntry> = serde_yaml::from_str(input).map_err(|error| {
        let mut localized = LocalizedError::new("error.plugin_cordis_yaml");
        if let Some(location) = error.location() {
            localized = localized
                .arg("line", location.line())
                .arg("column", location.column());
        }
        localized
    })?;
    validate_cordis_entries(&entries)?;
    Ok(entries)
}

pub fn validate_cordis_entries(entries: &[CordisEntry]) -> KfResult<()> {
    let mut ids = HashSet::new();
    for entry in entries {
        if !valid_identifier(&entry.id, 128) || !ids.insert(entry.id.as_str()) {
            return Err(LocalizedError::new("error.plugin_cordis_id").arg("id", &entry.id));
        }
        validate_text(&entry.name, "cordisName")?;
        if let Some(inject) = &entry.inject {
            match inject {
                CordisInject::Services(services) => validate_unique_names(services, "inject")?,
                CordisInject::Config(services) => {
                    for service in services.keys() {
                        validate_service_name(service, "inject")?;
                    }
                }
            }
        }
        if let Some(intercept) = &entry.intercept {
            for service in intercept.keys() {
                validate_service_name(service, "intercept")?;
            }
        }
        if let Some(isolate) = &entry.isolate {
            for (service, binding) in isolate {
                validate_service_name(service, "isolate")?;
                binding.validate(service)?;
            }
        }
    }
    Ok(())
}

pub fn encode_cordis_yaml(entries: &[CordisEntry]) -> KfResult<String> {
    validate_cordis_entries(entries)?;
    serde_yaml::to_string(entries).map_err(|_| LocalizedError::new("error.plugin_cordis_encode"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DshClientContribution {
    format: &'static str,
    delivery: DshDelivery,
    plugin_id: String,
    source_target: StudioTarget,
    registrations: Vec<DshSlotRegistration>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DshSlotRegistration {
    slot: String,
    options: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    contract: Option<DshSlotDescriptor>,
    register_options: Vec<&'static str>,
    supported: bool,
    components: Vec<UiContribution>,
}

fn generate_dsh_client_contribution(
    manifest: &PluginManifest,
    target: StudioTarget,
) -> KfResult<String> {
    let mut groups: BTreeMap<String, Vec<UiContribution>> = BTreeMap::new();
    for component in &manifest.ui {
        let slot = map_to_dsh_slot(target, component.slot());
        groups.entry(slot).or_default().push(component.clone());
    }
    let registrations = groups
        .into_iter()
        .map(|(slot, components)| {
            let contract = dsh_slot_descriptor(&slot);
            let options = match contract.map(|descriptor| descriptor.kind) {
                Some(DshSlotKind::List) => json!({
                    "id": manifest.id,
                    "order": 100,
                    "label": manifest.name.primary()
                }),
                Some(DshSlotKind::Keyed) => {
                    let key = if slot == "tool.view.cordis" {
                        "self"
                    } else {
                        components
                            .first()
                            .and_then(|component| component.extension().props.get("slotKey"))
                            .and_then(Value::as_str)
                            .unwrap_or(&manifest.id)
                    };
                    json!({ "key": key })
                }
                _ => json!({}),
            };
            let register_options = match contract.map(|descriptor| descriptor.kind) {
                Some(DshSlotKind::Single) => vec![],
                Some(DshSlotKind::List) => vec!["id", "order", "label"],
                Some(DshSlotKind::Keyed) => vec!["key"],
                Some(DshSlotKind::Chain) => vec!["select"],
                None => vec![],
            };
            let supported = contract.is_some_and(|descriptor| {
                descriptor.replace_risk == DshReplaceRisk::None
                    && matches!(descriptor.kind, DshSlotKind::List | DshSlotKind::Keyed)
            });
            DshSlotRegistration {
                slot,
                options,
                contract,
                register_options,
                supported,
                components,
            }
        })
        .collect();
    let contribution = DshClientContribution {
        format: DSH_ADAPTER_DATA_VERSION,
        delivery: DshDelivery::CordisDefine,
        plugin_id: manifest.id.clone(),
        source_target: target,
        registrations,
    };
    serde_json::to_string_pretty(&contribution)
        .map_err(|_| LocalizedError::new("error.plugin_client_encode"))
}

const DSH_ADAPTER_CSS: &str = r#"
.kf-dsh-canvas{position:relative;width:min(94vw,1280px);aspect-ratio:16/9;overflow:hidden;border:1px solid var(--dsw-alias-stroke-subtle,#2e3442);border-radius:12px;background:var(--dsw-alias-bg-layer-1,#14171e);color:var(--dsw-alias-label-primary,#e8ecf5);font-family:Georgia,'Times New Roman',serif;pointer-events:auto;box-sizing:border-box;container-type:inline-size}
.kf-dsh-stage{position:absolute;inset:0;font-size:0.9375cqw}
.kf-dsh-item{position:absolute;box-sizing:border-box;min-width:0;min-height:0;color:inherit;font:inherit}
.kf-dsh-button,.kf-dsh-input,.kf-dsh-select{width:100%;height:100%;border:1px solid var(--dsw-alias-stroke-subtle,#2e3442);border-radius:10px;background:var(--dsw-alias-bg-layer-2,#191d26);color:inherit;font:inherit}
.kf-dsh-button{cursor:pointer;transition:color .3s ease,border-color .3s ease,background-color .3s ease,box-shadow .45s ease,transform .28s cubic-bezier(.22,1,.36,1)}
.kf-dsh-button:hover:not(:disabled){color:#fff;border-color:#4d6bfe;background:#1f2534;box-shadow:0 0 18px rgba(77,107,254,.35);transform:translateY(-1px)}
.kf-dsh-button:active:not(:disabled){transform:translateY(0) scale(.965)}
.kf-dsh-button[aria-pressed=true]{box-shadow:0 0 0 1px #4d6bfe inset}
.kf-dsh-button:disabled,.kf-dsh-input:disabled,.kf-dsh-select:disabled{cursor:not-allowed;opacity:.38}
.kf-dsh-button:focus-visible,.kf-dsh-input:focus-visible,.kf-dsh-select:focus-visible{outline:1px solid #4d6bfe;outline-offset:2px}
.kf-dsh-field,.kf-dsh-toggle{display:flex;gap:6px;align-items:center}.kf-dsh-field{flex-direction:column;align-items:stretch}.kf-dsh-label{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:12px}
.kf-dsh-text{display:flex;align-items:center;overflow:auto}.kf-dsh-panel{padding:12px;border:1px solid var(--dsw-alias-stroke-subtle,#2e3442);border-radius:10px;background:var(--dsw-alias-bg-layer-2,#191d26);z-index:0}.kf-dsh-separator{background:var(--dsw-alias-stroke-subtle,#3a4152)}
"#;

const DSH_CLIENT_ADAPTER_BODY: &str = r#";
const css = __KF_CSS__;
const pluginCss = __KF_PLUGIN_CSS__;
function textOf(value) {
  if (typeof value === 'string') return value;
  if (!value || typeof value !== 'object' || !value.values) return '';
  return value.values[value.defaultLocale] || Object.values(value.values)[0] || '';
}
function itemStyle(item) {
  const b = item.bounds;
  return {
    left: `${b.x / 100}%`, top: `${b.y / 100}%`,
    width: `${b.width / 100}%`, height: `${b.height / 100}%`,
    zIndex: item.type === 'panel' ? 0 : 1,
  };
}
function faceStyle(item) {
  const style = item.style;
  if (!style || typeof style !== 'object') return undefined;
  const face = {};
  for (const key of Object.keys(style)) face[key] = style[key];
  return Object.keys(face).length ? face : undefined;
}
function baseProps(item, className) {
  return { className: `kf-plugin-component kf-dsh-item ${className}`, style: itemStyle(item), 'data-kf-id': item.id };
}
function ButtonView({ item }) {
  const [pressed, setPressed] = React.useState(false);
  return React.createElement('div', baseProps(item, 'kf-dsh-button-wrap'),
    React.createElement('button', {
      className: 'kf-dsh-button', type: 'button', disabled: item.disabled,
      style: faceStyle(item), 'aria-pressed': pressed, 'data-command': item.command || undefined,
      onClick: () => setPressed(value => !value),
    }, textOf(item.label)));
}
function renderItem(item) {
  if (item.type === 'button') return React.createElement(ButtonView, { key: item.id, item });
  if (item.type === 'toggle') return React.createElement('label', {
    ...baseProps(item, 'kf-dsh-toggle'), key: item.id,
  }, React.createElement('input', {
    type: 'checkbox', defaultChecked: item.value, disabled: item.disabled,
    'data-command': item.command || undefined,
  }), React.createElement('span', { style: faceStyle(item) }, textOf(item.label)));
  if (item.type === 'text') return React.createElement('div', {
    ...baseProps(item, 'kf-dsh-text'), key: item.id, style: { ...itemStyle(item), ...item.style },
  }, textOf(item.text));
  if (item.type === 'input') return React.createElement('label', {
    ...baseProps(item, 'kf-dsh-field'), key: item.id,
  }, item.label ? React.createElement('span', { className: 'kf-dsh-label' }, textOf(item.label)) : null,
  React.createElement('input', {
    className: 'kf-dsh-input', defaultValue: item.value || '', placeholder: item.placeholder || '',
    style: faceStyle(item), disabled: item.disabled, 'data-command': item.command || undefined,
  }));
  if (item.type === 'select') return React.createElement('label', {
    ...baseProps(item, 'kf-dsh-field'), key: item.id,
  }, item.label ? React.createElement('span', { className: 'kf-dsh-label' }, textOf(item.label)) : null,
  React.createElement('select', {
    className: 'kf-dsh-select', defaultValue: item.value, style: faceStyle(item),
    disabled: item.disabled, 'data-command': item.command || undefined,
  }, item.options.map(option => React.createElement('option', {
    key: option.value, value: option.value,
  }, textOf(option.label)))));
  if (item.type === 'separator') return React.createElement('div', {
    ...baseProps(item, 'kf-dsh-separator'), key: item.id, role: 'separator',
    style: { ...itemStyle(item), ...item.style },
    'aria-orientation': item.orientation || 'horizontal',
  });
  if (item.type === 'panel') return React.createElement('section', {
    ...baseProps(item, 'kf-dsh-panel'), key: item.id, style: { ...itemStyle(item), ...item.style },
  }, item.title ? textOf(item.title) : null);
  return null;
}
function Canvas({ registration }) {
  return React.createElement('div', {
    className: 'kf-dsh-canvas', 'data-kf-slot': registration.slot,
  }, React.createElement('div', { className: 'kf-plugin-surface kf-dsh-stage' },
    registration.components.map(renderItem)));
}
return {
  name: 'knightframe-studio-preview',
  inject: ['slots'],
  apply(ctx) {
    styles.insert(css);
    if (pluginCss) styles.insert(pluginCss);
    for (const registration of spec.registrations) {
      ctx.slots.inject(registration.slot, () => ctx.slots.register(
        Object.assign({ name: registration.slot }, registration.options),
        () => React.createElement(Canvas, { registration }),
      ));
    }
  },
}
"#;

fn js_json_literal(value: &Value) -> KfResult<String> {
    serde_json::to_string(value)
        .map(|encoded| {
            encoded
                .replace('\u{2028}', "\\u2028")
                .replace('\u{2029}', "\\u2029")
        })
        .map_err(|_| LocalizedError::new("error.plugin_client_encode"))
}

fn generate_dsh_client_code(contribution_json: &str, style_css: &str) -> KfResult<String> {
    let contribution: Value = serde_json::from_str(contribution_json)
        .map_err(|_| LocalizedError::new("error.plugin_client_encode"))?;
    if contribution.get("format").and_then(Value::as_str) != Some(DSH_ADAPTER_DATA_VERSION)
        || !contribution
            .get("registrations")
            .is_some_and(Value::is_array)
    {
        return Err(LocalizedError::new("error.plugin_client_format"));
    }
    let spec = js_json_literal(&contribution)?;
    let css = js_json_literal(&Value::String(DSH_ADAPTER_CSS.trim().into()))?;
    // 工坊为 DSH 目标设计的皮肤（DEFAULT_STUDIO_STYLE_DSH 或用户自定义）随包
    // 下发：适配器先插结构样式，再插插件皮肤，宿主里长出来的就是工坊里的样子。
    let plugin_css = js_json_literal(&Value::String(style_css.trim().into()))?;
    Ok(format!(
        "const spec = {spec}{}",
        DSH_CLIENT_ADAPTER_BODY
            .replace("__KF_CSS__", &css)
            .replace("__KF_PLUGIN_CSS__", &plugin_css)
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DshDefineArguments {
    plugin: DshNewPlugin,
    name: String,
    purpose: String,
    code: DshDefineCode,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DshNewPlugin {
    kind: &'static str,
    id_prefix: String,
}

#[derive(Debug, Serialize)]
struct DshDefineCode {
    client: String,
}

fn dsh_id_prefix(plugin_id: &str) -> String {
    let prefix: String = plugin_id
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .map(|character| character.to_ascii_lowercase())
        .take(6)
        .collect();
    if prefix.len() >= 3 {
        prefix
    } else {
        "kfp".into()
    }
}

fn generate_dsh_define_arguments(manifest: &PluginManifest, client_code: &str) -> KfResult<String> {
    let arguments = DshDefineArguments {
        plugin: DshNewPlugin {
            kind: "new",
            id_prefix: dsh_id_prefix(&manifest.id),
        },
        name: manifest.name.primary().into(),
        purpose: format!(
            "Preview the declarative UI exported by KnightFrame plugin {}.",
            manifest.id
        ),
        code: DshDefineCode {
            client: client_code.into(),
        },
    };
    serde_json::to_string_pretty(&arguments)
        .map_err(|_| LocalizedError::new("error.plugin_dsh_define_encode"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcId {
    String(String),
    Integer(i64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcFrame {
    Request(RpcRequest),
    Notification(RpcNotification),
    Success(RpcSuccess),
    Error(RpcErrorResponse),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: RpcId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcSuccess {
    pub jsonrpc: String,
    pub id: RpcId,
    pub result: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcErrorResponse {
    pub jsonrpc: String,
    pub id: RpcId,
    pub error: RpcErrorObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcFrame {
    pub fn validate(&self) -> KfResult<()> {
        match self {
            Self::Request(value) => {
                validate_rpc_version(&value.jsonrpc)?;
                validate_rpc_id(&value.id)?;
                validate_rpc_method(&value.method)?;
                validate_rpc_params(&value.params)
            }
            Self::Notification(value) => {
                validate_rpc_version(&value.jsonrpc)?;
                validate_rpc_method(&value.method)?;
                validate_rpc_params(&value.params)
            }
            Self::Success(value) => {
                validate_rpc_version(&value.jsonrpc)?;
                validate_rpc_id(&value.id)
            }
            Self::Error(value) => {
                validate_rpc_version(&value.jsonrpc)?;
                validate_rpc_id(&value.id)?;
                if value.error.message.trim().is_empty() || value.error.message.len() > 256 {
                    return Err(LocalizedError::new("error.plugin_rpc_error"));
                }
                Ok(())
            }
        }
    }
}

fn validate_rpc_version(version: &str) -> KfResult<()> {
    if version != "2.0" {
        return Err(LocalizedError::new("error.plugin_rpc_version"));
    }
    Ok(())
}

fn validate_rpc_id(id: &RpcId) -> KfResult<()> {
    if matches!(id, RpcId::String(value) if value.is_empty() || value.len() > 128) {
        return Err(LocalizedError::new("error.plugin_rpc_id"));
    }
    Ok(())
}

fn validate_rpc_method(method: &str) -> KfResult<()> {
    if !valid_identifier(method, 128) || method.starts_with("rpc.") {
        return Err(LocalizedError::new("error.plugin_rpc_method"));
    }
    Ok(())
}

fn validate_rpc_params(params: &Option<Value>) -> KfResult<()> {
    if params
        .as_ref()
        .is_some_and(|value| !value.is_object() && !value.is_array())
    {
        return Err(LocalizedError::new("error.plugin_rpc_params"));
    }
    Ok(())
}

pub fn parse_jsonl_frame(bytes: &[u8]) -> KfResult<RpcFrame> {
    if bytes.is_empty() || bytes.len() > MAX_JSONL_FRAME_BYTES {
        return Err(
            LocalizedError::new("error.plugin_rpc_frame_size").arg("limit", MAX_JSONL_FRAME_BYTES)
        );
    }
    let mut line = bytes;
    if let Some(value) = line.strip_suffix(b"\n") {
        line = value;
        if let Some(value) = line.strip_suffix(b"\r") {
            line = value;
        }
    }
    if line.is_empty() || line.contains(&b'\n') || line.contains(&b'\r') {
        return Err(LocalizedError::new("error.plugin_rpc_jsonl"));
    }
    std::str::from_utf8(line).map_err(|_| LocalizedError::new("error.plugin_rpc_utf8"))?;
    let frame: RpcFrame = serde_json::from_slice(line).map_err(|error| {
        LocalizedError::new("error.plugin_rpc_json")
            .arg("line", error.line())
            .arg("column", error.column())
    })?;
    frame.validate()?;
    Ok(frame)
}

pub fn encode_jsonl_frame(frame: &RpcFrame) -> KfResult<Vec<u8>> {
    frame.validate()?;
    let mut bytes =
        serde_json::to_vec(frame).map_err(|_| LocalizedError::new("error.plugin_rpc_encode"))?;
    if bytes.len().saturating_add(1) > MAX_JSONL_FRAME_BYTES {
        return Err(
            LocalizedError::new("error.plugin_rpc_frame_size").arg("limit", MAX_JSONL_FRAME_BYTES)
        );
    }
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MANIFEST: &str = include_str!("../tests/fixtures/plugins/valid-plugin.json");
    const VALID_CORDIS: &str = include_str!("../tests/fixtures/plugins/valid-cordis.yml");

    fn studio_request(target: StudioTarget) -> StudioRequest {
        StudioRequest {
            manifest_json: VALID_MANIFEST.into(),
            target,
            viewport: PixelViewport {
                width: 1600,
                height: 900,
            },
        }
    }

    #[test]
    fn strict_manifest_fixture_covers_every_component() {
        let manifest =
            parse_manifest_json(VALID_MANIFEST, StudioTarget::Dsh).expect("valid manifest fixture");
        assert_eq!(manifest.ui.len(), 7);
        assert_eq!(manifest.tools.len(), 1);
        assert!(matches!(manifest.runtime, PluginRuntime::Rust));
    }

    #[test]
    fn manifest_rejects_unknown_fields_and_non_object_schema() {
        let unknown = VALID_MANIFEST.replacen(
            "\"protocolVersion\"",
            "\"unexpected\":true,\"protocolVersion\"",
            1,
        );
        assert_eq!(
            parse_manifest_json(&unknown, StudioTarget::Dsh)
                .expect_err("unknown fields must fail")
                .key,
            "error.plugin_manifest_json"
        );

        let schema = VALID_MANIFEST.replacen(
            "\"configSchema\": { \"type\": \"object\" }",
            "\"configSchema\": []",
            1,
        );
        assert_eq!(
            parse_manifest_json(&schema, StudioTarget::Dsh)
                .expect_err("config schema root must be an object")
                .key,
            "error.plugin_config_schema"
        );
    }

    #[test]
    fn manifest_rejects_unsafe_entry_duplicates_and_unknown_slots() {
        let unsafe_path =
            VALID_MANIFEST.replacen("bin/sample-counter.exe", "../sample-counter.exe", 1);
        assert_eq!(
            parse_manifest_json(&unsafe_path, StudioTarget::Dsh)
                .expect_err("parent traversal must fail")
                .key,
            "error.plugin_entry"
        );

        let duplicate = VALID_MANIFEST.replacen("toggle-mode", "run-action", 1);
        assert_eq!(
            parse_manifest_json(&duplicate, StudioTarget::Dsh)
                .expect_err("duplicate component id must fail")
                .key,
            "error.plugin_ui_duplicate"
        );

        let unknown_slot = VALID_MANIFEST.replacen("tool.view.cordis", "sidebar", 1);
        assert_eq!(
            parse_manifest_json(&unknown_slot, StudioTarget::Dsh)
                .expect_err("occupied DSH slots must fail")
                .key,
            "error.plugin_ui_slot"
        );
    }

    #[test]
    fn knightframe_slot_maps_to_the_safe_dsh_tool_view() {
        let source = VALID_MANIFEST.replace("tool.view.cordis", "tool.view.plugin");
        let request = StudioRequest {
            manifest_json: source,
            ..studio_request(StudioTarget::Knightframe)
        };
        let preview = kf_plugin_studio_export_preview(request).expect("export preview");
        let contribution: Value = serde_json::from_str(&preview.client_contribution_json)
            .expect("generated client contribution JSON");
        assert_eq!(contribution["registrations"][0]["slot"], "tool.view.cordis");
        assert_eq!(contribution["registrations"][0]["options"]["key"], "self");
    }

    #[test]
    fn canvas_conversion_is_stable_clamped_and_round_trips() {
        let viewport = PixelViewport {
            width: 1600,
            height: 900,
        };
        let source = CanvasBounds {
            x: 1250,
            y: 2500,
            width: 5000,
            height: 3750,
        };
        let pixels = source.to_pixels(viewport).expect("pixel conversion");
        assert_eq!(
            pixels,
            PixelBounds {
                x: 200,
                y: 225,
                width: 800,
                height: 338,
            }
        );
        let restored = pixels.to_canvas(viewport).expect("canvas conversion");
        assert!(source.x.abs_diff(restored.x) <= 4);
        assert!(source.y.abs_diff(restored.y) <= 6);
        assert!(source.width.abs_diff(restored.width) <= 7);
        assert!(source.height.abs_diff(restored.height) <= 12);

        assert_eq!(
            CanvasBounds {
                x: 20_000,
                y: 20_000,
                width: 0,
                height: 0,
            }
            .clamped(),
            CanvasBounds {
                x: 9999,
                y: 9999,
                width: 1,
                height: 1,
            }
        );
        assert_eq!(
            PixelBounds {
                x: 5000,
                y: 5000,
                width: 0,
                height: 0,
            }
            .to_canvas(viewport)
            .expect("pixel bounds clamp"),
            CanvasBounds {
                x: 9994,
                y: 9989,
                width: 6,
                height: 11,
            }
        );
    }

    #[test]
    fn cordis_fixture_and_generated_entry_are_strict_and_round_trip() {
        let entries = parse_cordis_yaml(VALID_CORDIS).expect("valid Cordis fixture");
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0].inject, Some(CordisInject::Services(_))));
        assert!(matches!(entries[1].inject, Some(CordisInject::Config(_))));

        let invalid = VALID_CORDIS.replacen("name:", "unknown: true\n  name:", 1);
        assert_eq!(
            parse_cordis_yaml(&invalid)
                .expect_err("unknown Cordis fields must fail")
                .key,
            "error.plugin_cordis_yaml"
        );

        let encoded = encode_cordis_yaml(&entries).expect("encode validated Cordis entries");
        assert_eq!(
            parse_cordis_yaml(&encoded).expect("round-trip Cordis YAML"),
            entries
        );

        let preview = kf_plugin_studio_export_preview(studio_request(StudioTarget::Dsh))
            .expect("export preview");
        assert!(
            parse_cordis_yaml(&preview.cordis_yaml)
                .expect("dynamic no-op Cordis YAML")
                .is_empty()
        );
        assert!(preview.cordis_yaml.contains(DSH_DEFINE_TOOL));
        assert_eq!(
            preview.cordis_yaml_unavailable_reason,
            "studio.dsh.dynamic_requires_cordis_define"
        );
    }

    #[test]
    fn rpc_frames_reject_bad_shapes_and_embedded_lines() {
        let request =
            b"{\"jsonrpc\":\"2.0\",\"id\":\"call-1\",\"method\":\"plugin.health\",\"params\":{}}\n";
        assert!(matches!(
            parse_jsonl_frame(request).expect("valid request"),
            RpcFrame::Request(_)
        ));
        assert_eq!(
            parse_jsonl_frame(
                b"{\"jsonrpc\":\"2.0\",\"method\":\"plugin.health\",\"params\":\"bad\"}\n"
            )
            .expect_err("scalar params must fail")
            .key,
            "error.plugin_rpc_params"
        );
        assert_eq!(
            parse_jsonl_frame(b"{\"jsonrpc\":\"2.0\",\"method\":\"plugin.health\"}\n{}\n")
                .expect_err("multiple lines must fail")
                .key,
            "error.plugin_rpc_jsonl"
        );
        assert_eq!(
            parse_jsonl_frame(
                b"{\"jsonrpc\":\"2.0\",\"method\":\"plugin.health\",\"extra\":true}\n"
            )
            .expect_err("unknown frame fields must fail")
            .key,
            "error.plugin_rpc_json"
        );
    }

    #[test]
    fn jsonl_limit_counts_utf8_bytes_and_terminal_newline() {
        let prefix = br#"{"jsonrpc":"2.0","method":"plugin.health","params":{"text":""#;
        let suffix = b"\"}}\n";
        let available = MAX_JSONL_FRAME_BYTES - prefix.len() - suffix.len();
        let mut text = "é".repeat(available / 2);
        if available % 2 == 1 {
            text.push('a');
        }
        let mut exact = Vec::with_capacity(MAX_JSONL_FRAME_BYTES);
        exact.extend_from_slice(prefix);
        exact.extend_from_slice(text.as_bytes());
        exact.extend_from_slice(suffix);
        assert_eq!(exact.len(), MAX_JSONL_FRAME_BYTES);
        parse_jsonl_frame(&exact).expect("exact byte limit must pass");

        let mut oversized = exact[..exact.len() - suffix.len()].to_vec();
        oversized.extend_from_slice("é".as_bytes());
        oversized.extend_from_slice(suffix);
        assert_eq!(
            parse_jsonl_frame(&oversized)
                .expect_err("multibyte overflow must fail")
                .key,
            "error.plugin_rpc_frame_size"
        );
    }

    #[test]
    fn studio_export_contains_real_cordis_define_client_source() {
        let validation = kf_plugin_studio_validate(studio_request(StudioTarget::Dsh))
            .expect("Studio validation");
        assert!(validation.valid);
        assert_eq!(validation.layout.len(), 7);

        let preview = kf_plugin_studio_export_preview(studio_request(StudioTarget::Dsh))
            .expect("Studio export preview");
        let manifest: PluginManifest =
            serde_json::from_str(&preview.manifest_json).expect("normalized manifest JSON");
        validate_manifest(&manifest, StudioTarget::Dsh).expect("normalized manifest validation");
        let contribution: Value = serde_json::from_str(&preview.client_contribution_json)
            .expect("client contribution JSON");
        assert_eq!(contribution["format"], DSH_ADAPTER_DATA_VERSION);
        assert_eq!(contribution["delivery"], "cordisDefine");
        assert!(contribution.get("code").is_none());

        let define: Value = serde_json::from_str(&preview.dsh_define_arguments_json)
            .expect("cordis_define arguments JSON");
        assert_eq!(define.as_object().expect("define object").len(), 4);
        assert_eq!(define["plugin"]["kind"], "new");
        let prefix = define["plugin"]["idPrefix"]
            .as_str()
            .expect("semantic id prefix");
        assert!((3..=6).contains(&prefix.len()));
        assert!(prefix.chars().all(|value| value.is_ascii_lowercase()));
        assert_eq!(define["code"]["client"], preview.dsh_client_code);
        assert!(define["code"].get("host").is_none());

        assert!(preview.dsh_client_code.contains("inject: ['slots']"));
        assert!(preview.dsh_client_code.contains("ctx.slots.inject("));
        assert!(preview.dsh_client_code.contains("ctx.slots.register("));
        assert!(preview.dsh_client_code.contains("React.createElement("));
        assert!(preview.dsh_client_code.contains("styles.insert(css)"));
        for forbidden in [
            "import ",
            "require(",
            "window.",
            "document.",
            "fetch(",
            "setTimeout(",
            "setInterval(",
        ] {
            assert!(
                !preview.dsh_client_code.contains(forbidden),
                "generated DSH client source contains forbidden token {forbidden}"
            );
        }
        assert_eq!(preview.dsh_runtime.delivery, DshDelivery::CordisDefine);
        assert_eq!(preview.dsh_runtime.define_tool, DSH_DEFINE_TOOL);
        assert_eq!(
            preview.dsh_runtime.host_runner_package,
            DSH_HOST_RUNNER_PACKAGE
        );
        assert_eq!(
            preview.dsh_runtime.client_runner_package,
            DSH_CLIENT_RUNNER_PACKAGE
        );
        assert!(preview.dsh_runtime.requires_client_approval);
        assert!(preview.dsh_runtime.process_local);
    }

    #[test]
    fn dsh_client_code_carries_the_studio_skin_and_item_styles() {
        let manifest_json = VALID_MANIFEST.replacen(
            "\"protocolVersion\"",
            "\"styleCss\":\".kf-plugin-surface{--dsh-accent:#4d6bfe}\",\"protocolVersion\"",
            1,
        );
        let request = StudioRequest {
            manifest_json,
            target: StudioTarget::Dsh,
            viewport: PixelViewport {
                width: 1280,
                height: 720,
            },
        };
        let preview =
            kf_plugin_studio_export_preview(request).expect("Studio export preview with skin");
        assert!(preview.dsh_client_code.contains("const pluginCss ="));
        assert!(
            preview
                .dsh_client_code
                .contains("if (pluginCss) styles.insert(pluginCss)")
        );
        assert!(preview.dsh_client_code.contains("--dsh-accent:#4d6bfe"));
        // 组件皮肤样式直通到控件 face（React 内联样式），画布/条目挂插件类名
        assert!(preview.dsh_client_code.contains("function faceStyle(item)"));
        assert!(
            preview
                .dsh_client_code
                .contains("'kf-plugin-surface kf-dsh-stage'")
        );
        assert!(
            preview
                .dsh_client_code
                .contains("`kf-plugin-component kf-dsh-item")
        );
        // 16:9 参考画布：与工坊画布同比例，导出后不再缩小
        assert!(preview.dsh_client_code.contains("aspect-ratio:16/9"));
        assert!(preview.dsh_client_code.contains("width:min(94vw,1280px)"));
    }

    #[test]
    fn generated_dsh_client_source_passes_the_v8_function_body_precheck_when_node_exists() {
        use std::{io::Write, process::Stdio};

        let preview = kf_plugin_studio_export_preview(studio_request(StudioTarget::Dsh))
            .expect("Studio export preview");
        let mut child = match std::process::Command::new("node")
            .args(["--check", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("failed to start Node syntax check: {error}"),
        };
        let wrapped = format!("(async () => {{\n{}\n}})()", preview.dsh_client_code);
        child
            .stdin
            .take()
            .expect("Node stdin")
            .write_all(wrapped.as_bytes())
            .expect("write generated source");
        let output = child
            .wait_with_output()
            .expect("wait for Node syntax check");
        assert!(
            output.status.success(),
            "generated client source failed Node syntax check: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
