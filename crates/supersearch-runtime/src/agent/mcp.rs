//! # Model Context Protocol (MCP) Translation
//!
//! Transforms the declarative `ExtensionCommand` definitions found in the manifest
//! into a Provider-Neutral Intermediate Representation (IR). This IR is then validated
//! and translated into dialect-specific schemas (e.g., OpenAI, Anthropic) via Adapters.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::LazyLock;
use thiserror::Error;

use crate::extension::manifest::{ExtensionCommand, ExtensionManifest};

static TOOL_NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]{1,64}$").unwrap());

/// Errors encountered during the compilation of Extension Manifests to MCP Tool Schemas.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum McpCompilerError {
    #[error("Tool name '{0}' contains invalid characters or exceeds 64 characters.")]
    InvalidToolName(String),
    #[error("Tool '{0}' is missing a required title/description.")]
    MissingDescription(String),
    #[error("Tool '{0}' defines a duplicate argument name '{1}'.")]
    DuplicateArgument(String, String),
    #[error("Tool '{0}' specifies an unsupported argument type '{1}'.")]
    UnsupportedType(String, String),
    #[error("Duplicate tool name '{0}' detected in manifest.")]
    DuplicateToolName(String),
}

/// The Provider-Neutral Intermediate Representation (IR) of an AI Tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalTool {
    pub name: String,
    pub description: String,
    pub arguments: Vec<InternalArgument>,
    pub is_idempotent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalArgument {
    pub name: String,
    pub r#type: String,
    pub description: String,
    pub required: bool,
}

/// The compilation engine that parses Manifests into `InternalTool`s.
pub struct McpCompiler;

impl McpCompiler {
    /// Compiles a complete manifest, ensuring no duplicate tool names and enforcing strict schema validation.
    pub fn compile_manifest(
        manifest: &ExtensionManifest,
    ) -> Result<Vec<InternalTool>, McpCompilerError> {
        let mut tools = Vec::new();
        let mut seen_names = HashSet::new();

        for cmd in &manifest.commands {
            if cmd.mode != "no-view" {
                continue; // Only background commands are exposed to the LLM.
            }

            let tool_name = format!("ext_{}_{}", manifest.id, cmd.name);

            if !seen_names.insert(tool_name.clone()) {
                return Err(McpCompilerError::DuplicateToolName(tool_name));
            }

            let tool = Self::compile_command(&tool_name, cmd)?;
            tools.push(tool);
        }

        Ok(tools)
    }

    fn compile_command(
        tool_name: &str,
        command: &ExtensionCommand,
    ) -> Result<InternalTool, McpCompilerError> {
        if !TOOL_NAME_REGEX.is_match(tool_name) {
            return Err(McpCompilerError::InvalidToolName(tool_name.to_string()));
        }

        if command.title.trim().is_empty() {
            return Err(McpCompilerError::MissingDescription(tool_name.to_string()));
        }

        let mut arguments = Vec::new();
        let mut seen_args = HashSet::new();

        for arg in &command.arguments {
            if !seen_args.insert(arg.name.clone()) {
                return Err(McpCompilerError::DuplicateArgument(
                    tool_name.to_string(),
                    arg.name.clone(),
                ));
            }

            // Strict allowlist of JSON schema primitives
            if !matches!(
                arg.r#type.as_str(),
                "string" | "number" | "boolean" | "integer"
            ) {
                return Err(McpCompilerError::UnsupportedType(
                    tool_name.to_string(),
                    arg.r#type.clone(),
                ));
            }

            arguments.push(InternalArgument {
                name: arg.name.clone(),
                r#type: arg.r#type.clone(),
                description: arg.description.clone().unwrap_or_default(),
                required: arg.required,
            });
        }

        // Phase 13 §5: Idempotency Flags. Safe execution parsing.
        // We will default to false (mutative) if not specified via custom manifest attributes.
        let is_idempotent = false; // In a full implementation, we'd read `command.idempotent`.

        Ok(InternalTool {
            name: tool_name.to_string(),
            description: command.title.clone(),
            arguments,
            is_idempotent,
        })
    }
}

/// Adapts the `InternalTool` IR into Provider-Specific JSON schemas.
pub struct ProviderAdapter;

impl ProviderAdapter {
    /// Transforms the IR into an OpenAI-compatible JSON Schema.
    pub fn to_openai_schema(tool: &InternalTool, manifest_version: &str) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for arg in &tool.arguments {
            properties.insert(
                arg.name.clone(),
                json!({
                    "type": arg.r#type,
                    "description": arg.description,
                }),
            );
            if arg.required {
                required.push(Value::String(arg.name.clone()));
            }
        }

        json!({
            "type": "function",
            "function": {
                "name": tool.name,
                "description": tool.description,
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required
                }
            },
            "_meta": {
                "schema_version": "v1",
                "manifest_version": manifest_version
            }
        })
    }

    /// Transforms the IR into an Anthropic-compatible XML/JSON Tool Schema.
    pub fn to_anthropic_schema(tool: &InternalTool, _manifest_version: &str) -> Value {
        // Anthropic tool schemas are structurally similar but omit the nested `function` wrapper.
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for arg in &tool.arguments {
            properties.insert(
                arg.name.clone(),
                json!({
                    "type": arg.r#type,
                    "description": arg.description,
                }),
            );
            if arg.required {
                required.push(Value::String(arg.name.clone()));
            }
        }

        json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::manifest::{CommandArgument, ExtensionCommand, ExtensionKind};

    fn base_manifest() -> ExtensionManifest {
        ExtensionManifest {
            id: "linear".into(),
            name: "Linear".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            kind: ExtensionKind::Js,
            entrypoint: "bundle.js".into(),
            keywords: vec![],
            permissions: vec![],
            commands: vec![],
        }
    }

    #[test]
    fn test_valid_ir_compilation() {
        let mut manifest = base_manifest();
        manifest.commands.push(ExtensionCommand {
            name: "create-issue".into(),
            title: "Create Linear Issue".into(),
            mode: "no-view".into(),
            arguments: vec![CommandArgument {
                name: "title".into(),
                r#type: "string".into(),
                description: Some("The issue title".into()),
                required: true,
            }],
        });

        let tools = McpCompiler::compile_manifest(&manifest).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ext_linear_create-issue");

        let openai = ProviderAdapter::to_openai_schema(&tools[0], &manifest.version);
        assert_eq!(openai["function"]["name"], "ext_linear_create-issue");
        assert_eq!(openai["_meta"]["manifest_version"], "1.0.0");
    }

    #[test]
    fn test_negative_invalid_tool_name() {
        let mut manifest = base_manifest();
        manifest.commands.push(ExtensionCommand {
            name: "invalid name with spaces".into(),
            title: "Bad Tool".into(),
            mode: "no-view".into(),
            arguments: vec![],
        });

        let res = McpCompiler::compile_manifest(&manifest);
        assert!(matches!(res, Err(McpCompilerError::InvalidToolName(_))));
    }

    #[test]
    fn test_negative_unsupported_type() {
        let mut manifest = base_manifest();
        manifest.commands.push(ExtensionCommand {
            name: "test".into(),
            title: "Test".into(),
            mode: "no-view".into(),
            arguments: vec![CommandArgument {
                name: "data".into(),
                r#type: "buffer".into(), // Unsupported type
                description: None,
                required: false,
            }],
        });

        let res = McpCompiler::compile_manifest(&manifest);
        assert!(matches!(res, Err(McpCompilerError::UnsupportedType(_, _))));
    }

    #[test]
    fn test_negative_duplicate_argument() {
        let mut manifest = base_manifest();
        manifest.commands.push(ExtensionCommand {
            name: "test".into(),
            title: "Test".into(),
            mode: "no-view".into(),
            arguments: vec![
                CommandArgument {
                    name: "arg1".into(),
                    r#type: "string".into(),
                    description: None,
                    required: true,
                },
                CommandArgument {
                    name: "arg1".into(),
                    r#type: "number".into(),
                    description: None,
                    required: true,
                },
            ],
        });

        let res = McpCompiler::compile_manifest(&manifest);
        assert!(matches!(
            res,
            Err(McpCompilerError::DuplicateArgument(_, _))
        ));
    }

    #[test]
    fn test_provider_conformance_golden() {
        // Ensures identical IR produces exactly expected Anthropic and OpenAI schemas.
        let ir = InternalTool {
            name: "ext_weather_get".into(),
            description: "Get weather".into(),
            is_idempotent: true,
            arguments: vec![InternalArgument {
                name: "location".into(),
                r#type: "string".into(),
                description: "City name".into(),
                required: true,
            }],
        };

        let openai = ProviderAdapter::to_openai_schema(&ir, "1.0.0");
        assert_eq!(openai["type"], "function");
        assert_eq!(openai["function"]["name"], "ext_weather_get");
        assert_eq!(openai["function"]["parameters"]["required"][0], "location");
        assert_eq!(openai["_meta"]["schema_version"], "v1");

        let anthropic = ProviderAdapter::to_anthropic_schema(&ir, "1.0.0");
        assert_eq!(anthropic["name"], "ext_weather_get");
        assert_eq!(anthropic["input_schema"]["type"], "object");
        assert_eq!(anthropic["input_schema"]["required"][0], "location");
        assert!(anthropic.get("_meta").is_none()); // Anthropic drops meta wrapper natively
    }
}
