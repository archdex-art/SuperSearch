//! Extension manifest — the declarative metadata every extension ships with.
//!
//! An extension is a directory containing a `manifest.toml` plus an entrypoint
//! (a script for `kind = "script"`, a `.wasm` module for `kind = "wasm"`).
//! The manifest declares identity, the entrypoint, optional trigger keywords,
//! and the capabilities the extension needs — each with a human-readable
//! justification shown to the user during the install consent flow.

use serde::{Deserialize, Serialize};

use crate::capability::token::Permission;

/// Parsed `manifest.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    /// Stable unique id (also the on-disk directory name). e.g. `weather`.
    pub id: String,
    /// Human-readable name shown in the manager UI.
    pub name: String,
    /// Semantic version string (e.g. "1.0.0").
    pub version: String,
    /// Optional author / publisher.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional one-line description.
    #[serde(default)]
    pub description: Option<String>,
    /// Execution model.
    pub kind: ExtensionKind,
    /// Entrypoint path, relative to the extension directory
    /// (e.g. `run.sh` or `plugin.wasm`).
    pub entrypoint: String,
    /// Optional keywords that route a query to this extension. Empty = the
    /// extension is consulted for every query.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Capabilities requested, each with a justification for consent.
    #[serde(default)]
    pub permissions: Vec<PermissionRequest>,
    /// The list of commands exposed by this extension (Phase 13 AI Tools).
    #[serde(default)]
    pub commands: Vec<ExtensionCommand>,
}

/// A specific command exposed by an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionCommand {
    /// The unique name of the command.
    pub name: String,
    /// Human-readable title.
    pub title: String,
    /// Execution mode: "view" (opens UI) or "no-view" (headless, can be run by AI).
    pub mode: String,
    /// Optional arguments defining the JSON schema for this command.
    #[serde(default)]
    pub arguments: Vec<CommandArgument>,
}

/// An argument expected by a command, mapped to an MCP tool parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandArgument {
    pub name: String,
    pub r#type: String, // e.g., "string", "number", "boolean"
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

/// How an extension is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionKind {
    /// A native script (shell/python/node) run as a subprocess. v1.
    Script,
    /// A sandboxed WebAssembly module. v2 (legacy exploration).
    Wasm,
    /// A sandboxed V8 JavaScript/TypeScript module. v3 (Current Architecture).
    Js,
}

/// A single requested capability plus why it is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// The capability being requested.
    pub permission: Permission,
    /// Why the extension needs it — shown in the consent dialog.
    pub justification: String,
}

/// Validation errors for a manifest.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("manifest field `{0}` is empty")]
    EmptyField(&'static str),
    #[error("extension id `{0}` is not a safe directory name")]
    UnsafeId(String),
    #[error("entrypoint `{0}` escapes the extension directory")]
    UnsafeEntrypoint(String),
}

impl ExtensionManifest {
    /// Parse a manifest from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Parse a manifest from JSON text.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Reject malformed or unsafe manifests before they are trusted.
    ///
    /// The id must be a plain slug (it becomes a directory name) and the
    /// entrypoint must stay inside the extension directory — no `..`, no
    /// absolute paths — so a manifest can never point execution elsewhere.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.trim().is_empty() {
            return Err(ManifestError::EmptyField("id"));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::EmptyField("name"));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::EmptyField("version"));
        }
        if self.entrypoint.trim().is_empty() {
            return Err(ManifestError::EmptyField("entrypoint"));
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ManifestError::UnsafeId(self.id.clone()));
        }
        let ep = std::path::Path::new(&self.entrypoint);
        if ep.is_absolute() || self.entrypoint.contains("..") || ep.components().count() == 0 {
            return Err(ManifestError::UnsafeEntrypoint(self.entrypoint.clone()));
        }
        Ok(())
    }

    /// The permissions this extension requests, as a plain list.
    pub fn requested_permissions(&self) -> Vec<Permission> {
        self.permissions.iter().map(|p| p.permission).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        id = "weather"
        name = "Weather"
        version = "1.0.0"
        author = "ACME"
        description = "Current conditions"
        kind = "script"
        entrypoint = "run.sh"
        keywords = ["weather", "forecast"]

        [[permissions]]
        permission = "NetworkConnect"
        justification = "Fetch the forecast"
    "#;

    #[test]
    fn parses_and_validates() {
        let m = ExtensionManifest::from_toml(SAMPLE).expect("parse");
        assert_eq!(m.id, "weather");
        assert_eq!(m.kind, ExtensionKind::Script);
        assert_eq!(m.keywords, vec!["weather", "forecast"]);
        assert_eq!(m.requested_permissions(), vec![Permission::NetworkConnect]);
        assert!(m.validate().is_ok());
    }

    #[test]
    fn rejects_directory_traversal_entrypoint() {
        let mut m = ExtensionManifest::from_toml(SAMPLE).unwrap();
        m.entrypoint = "../../etc/passwd".into();
        assert_eq!(
            m.validate(),
            Err(ManifestError::UnsafeEntrypoint("../../etc/passwd".into()))
        );
    }

    #[test]
    fn rejects_unsafe_id() {
        let mut m = ExtensionManifest::from_toml(SAMPLE).unwrap();
        m.id = "../evil".into();
        assert!(matches!(m.validate(), Err(ManifestError::UnsafeId(_))));
    }
}
