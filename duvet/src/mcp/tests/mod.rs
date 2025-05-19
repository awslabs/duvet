// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Testing infrastructure for the MCP server implementation.

use crate::{Result, mcp::server::Server, project::Project};
use duvet_core::{env, error, path::Path};
use rmcp::{
    RoleClient, ServiceExt,
    service::{QuitReason, RunningService},
};
use std::{io, ops, sync::Arc};
use tempfile::TempDir;
use tokio::io::duplex;

mod section_3;
mod section_5;

/// Manages a temporary test environment
pub struct TestContext {
    /// Root directory for this test
    root: Path,
    /// Temporary directory that will be cleaned up
    _temp_dir: TempDir,
}

impl TestContext {
    /// Create a new test context with a temporary directory
    pub fn new() -> io::Result<Self> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path().into();

        Ok(Self {
            root,
            _temp_dir: temp_dir,
        })
    }

    /// Get the root directory path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a file with given content
    pub fn file(&self, path: &str, content: &str) -> io::Result<Path> {
        let full_path = self.root.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full_path, content)?;
        Ok(full_path)
    }
}

/// Manages communication with an MCP server
pub struct Test {
    /// The client side of the connection
    client: RunningService<RoleClient, ()>,
}

impl Test {
    /// Create a new MCP server instance with a duplex connection
    pub async fn start(ctx: Arc<TestContext>) -> Result<Self> {
        let (client, stream) = duplex(1 << 17);

        // Create a project with default configuration
        let project = Project::default();
        let server = Server::new(project).await.unwrap();
        env::set_current_dir(ctx.root.clone());

        // Start the server
        tokio::spawn(async move {
            let server = server.serve(stream).await.unwrap();
            server.waiting().await.unwrap();
        });

        // Initialize the client
        let client = ().serve(client).await?;

        Ok(Self { client })
    }

    pub async fn cancel(self) -> Result<QuitReason> {
        Ok(self
            .client
            .cancel()
            .await
            .map_err(|err| error!("cancel error: {err:?}"))?)
    }
}

impl ops::Deref for Test {
    type Target = RunningService<RoleClient, ()>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl ops::DerefMut for Test {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}

/// Assert that a JSON value matches an expected pattern
#[macro_export]
macro_rules! assert_json_matches {
    ($actual:expr, $pattern:expr) => {
        let actual = &$actual;
        let pattern = &$pattern;

        // TODO: Implement pattern matching
        // For now just ensure actual matches pattern exactly
        assert_eq!(actual, pattern);
    };
}
