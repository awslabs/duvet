//! MCP server implementation for Duvet requirements traceability.
//!
//! This module implements a Model Context Protocol (MCP) server that provides
//! access to project specifications, requirements, and code citations through
//! a standardized API.
//!
//! This will start the server using stdio transport, allowing AI models to interact
//! with the project's specifications, requirements, and citations.

use crate::{project::Project, Result};
use clap::Parser;
use duvet_core::error;
use rmcp::{transport::stdio, ServiceExt};

pub mod server;

#[cfg(test)]
mod tests;

/// Start the MCP server for AI model interaction
#[derive(Debug, Parser)]
pub struct Mcp {
    #[clap(flatten)]
    project: Project,
}

impl Mcp {
    /// Executes the MCP server command
    pub async fn execute(&self) -> Result<()> {
        // Create a server
        let service = server::Server::new(self.project.clone())
            .await?
            .serve(stdio())
            .await?;
        service
            .waiting()
            .await
            .map_err(|error| error!("server error: {error}"))?;
        Ok(())
    }
}
