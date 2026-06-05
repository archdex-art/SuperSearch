//! Namespace isolation for capability scoping.
//!
//! Every capability token is bound to a namespace that constrains where it
//! can be used. Namespaces are hierarchical (dot-separated) and support
//! prefix matching for delegation.
//!
//! ## Examples
//! - `plugin.chatgpt.filesystem.~/Documents` — ChatGPT adapter can access ~/Documents
//! - `plugin.vscode.process.spawn` — VSCode adapter can spawn processes
//! - `kernel.automation.window` — Kernel-level window automation

use serde::{Serialize, Deserialize};

/// A hierarchical, dot-separated namespace.
///
/// Namespaces enforce isolation boundaries:
/// - A capability in namespace `plugin.chatgpt.fs` CANNOT be used for
///   operations in namespace `plugin.vscode.fs`.
/// - A capability in namespace `plugin.chatgpt` CAN be used for
///   `plugin.chatgpt.fs` (prefix containment).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Namespace {
    /// The full dot-separated namespace string.
    path: String,
    /// Pre-computed segment count for fast depth comparison.
    depth: usize,
}

impl Namespace {
    /// Create a new namespace from a dot-separated path.
    ///
    /// # Panics
    /// Panics if the path is empty or contains invalid characters.
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        assert!(!path.is_empty(), "Namespace path must not be empty");
        assert!(
            path.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '~' || c == '/'),
            "Invalid namespace character in '{}'", path
        );
        let depth = path.split('.').count();
        Self { path, depth }
    }

    /// The root kernel namespace. Only the kernel itself holds capabilities here.
    pub fn kernel() -> Self { Self::new("kernel") }

    /// Create a plugin-scoped namespace.
    pub fn plugin(plugin_name: &str) -> Self {
        Self::new(format!("plugin.{}", plugin_name))
    }

    /// Create a sub-namespace under this namespace.
    pub fn child(&self, segment: &str) -> Self {
        Self::new(format!("{}.{}", self.path, segment))
    }

    /// Check if `other` is contained within (or equal to) this namespace.
    ///
    /// A namespace `A` contains `B` if `B`'s path starts with `A`'s path
    /// followed by a dot separator (or is exactly equal).
    ///
    /// ```text
    /// "plugin.chatgpt".contains("plugin.chatgpt.fs")     → true
    /// "plugin.chatgpt".contains("plugin.chatgpt")        → true
    /// "plugin.chatgpt".contains("plugin.vscode")         → false
    /// "plugin.chatgpt".contains("plugin.chatgpt_extra")  → false (no dot boundary)
    /// ```
    #[inline]
    pub fn contains(&self, other: &Namespace) -> bool {
        if self.path == other.path {
            return true;
        }
        other.path.starts_with(&self.path) && other.path.as_bytes().get(self.path.len()) == Some(&b'.')
    }

    /// Check if two namespaces are in completely disjoint trees.
    #[inline]
    pub fn is_disjoint(&self, other: &Namespace) -> bool {
        !self.contains(other) && !other.contains(self)
    }

    /// Returns the full namespace path.
    #[inline]
    pub fn as_str(&self) -> &str { &self.path }

    /// Returns the depth (number of segments).
    #[inline]
    pub fn depth(&self) -> usize { self.depth }

    /// Returns the parent namespace, or None if this is a root namespace.
    pub fn parent(&self) -> Option<Namespace> {
        self.path.rfind('.').map(|idx| Namespace::new(&self.path[..idx]))
    }
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment_semantics() {
        let parent = Namespace::plugin("chatgpt");
        let child = parent.child("filesystem");
        let grandchild = child.child("read");
        let sibling = Namespace::plugin("vscode");

        assert!(parent.contains(&child));
        assert!(parent.contains(&grandchild));
        assert!(child.contains(&grandchild));
        assert!(!child.contains(&parent));
        assert!(!parent.contains(&sibling));
        assert!(parent.is_disjoint(&sibling));
    }

    #[test]
    fn no_false_prefix_match() {
        let ns = Namespace::new("plugin.chat");
        let not_child = Namespace::new("plugin.chatgpt");
        // "plugin.chat" should NOT contain "plugin.chatgpt" because
        // "gpt" doesn't start with a dot separator.
        assert!(!ns.contains(&not_child));
    }

    #[test]
    fn parent_navigation() {
        let ns = Namespace::new("plugin.chatgpt.filesystem.read");
        assert_eq!(ns.parent().unwrap().as_str(), "plugin.chatgpt.filesystem");
        assert_eq!(ns.parent().unwrap().parent().unwrap().as_str(), "plugin.chatgpt");
    }
}
