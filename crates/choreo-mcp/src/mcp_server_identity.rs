const DEFAULT_SERVER_NAME: &str = "underpass-choreo-mcp";
const DEFAULT_SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Identity advertised by the MCP server during initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct McpServerIdentity {
    name: &'static str,
    version: &'static str,
}

impl McpServerIdentity {
    /// Create a host-owned MCP server identity.
    #[must_use]
    pub const fn new(name: &'static str, version: &'static str) -> Self {
        Self { name, version }
    }

    /// MCP `serverInfo.name`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// MCP `serverInfo.version`.
    #[must_use]
    pub const fn version(self) -> &'static str {
        self.version
    }
}

impl Default for McpServerIdentity {
    fn default() -> Self {
        Self::new(DEFAULT_SERVER_NAME, DEFAULT_SERVER_VERSION)
    }
}
