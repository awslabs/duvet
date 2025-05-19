use crate::{
    Result,
    annotation::{self, Annotation, AnnotationSet},
    project::Project,
    reference::{self, Reference},
    source::SourceFile,
    target::SpecificationMap,
};
use rmcp::{
    Error, ServerHandler,
    model::{CallToolResult, Content},
    serde_json::json,
    tool,
};
use std::sync::Arc;

const PROMPT: &str = include_str!("./system_prompt.md");

#[derive(Clone)]
pub struct Server {
    project: Project,
    peer: Arc<tokio::sync::Mutex<Option<rmcp::Peer<rmcp::RoleServer>>>>,
}

impl Server {
    pub async fn new(project: Project) -> Result<Self> {
        Ok(Self {
            project,
            peer: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    async fn annotations(&self) -> Result<AnnotationSet> {
        let project_sources = self.project.sources().await?;
        let project_sources = Arc::new(project_sources);

        let annotations = annotation::query(project_sources.clone()).await?;

        Ok(annotations)
    }

    async fn references(&self) -> Result<Arc<[Reference]>> {
        let annotations = self.annotations().await?;

        let download_path = self.project.download_path().await?;
        let specifications =
            annotation::specifications(annotations.clone(), download_path.clone()).await?;

        let reference_map = annotation::reference_map(annotations.clone()).await?;

        let references = reference::query(reference_map.clone(), specifications.clone()).await?;

        Ok(references)
    }
}

#[tool(tool_box)]
impl Server {
    #[tool(description = "Count all of the annotations in the project sources")]
    async fn count_annotations(&self) -> Result<CallToolResult, rmcp::Error> {
        let annotations = self.annotations().await.map_err(|err| {
            Error::internal_error(
                "encountered an error",
                Some(json!({ "message": err.to_string() })),
            )
        })?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "There are {} annotations across all of the scanned sources in the codebase",
            annotations.len()
        ))]))
    }
}

#[tool(tool_box)]
impl ServerHandler for Server {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_03_26,
            capabilities: rmcp::model::ServerCapabilities::builder()
                .enable_resources()
                .enable_tools()
                .build(),
            server_info: rmcp::model::Implementation::from_build_env(),
            instructions: Some(PROMPT.to_string()),
        }
    }

    fn get_peer(&self) -> Option<rmcp::Peer<rmcp::RoleServer>> {
        self.peer.blocking_lock().clone()
    }

    fn set_peer(&mut self, peer: rmcp::Peer<rmcp::RoleServer>) {
        *self.peer.blocking_lock() = Some(peer);
    }

    async fn on_initialized(&self) {
        tracing::info!("client initialized");
    }

    async fn on_cancelled(&self, _notification: rmcp::model::CancelledNotificationParam) {
        tracing::info!("client cancelled");
    }

    async fn on_progress(&self, _notification: rmcp::model::ProgressNotificationParam) {
        tracing::info!("progress update");
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<rmcp::model::ListResourcesResult, rmcp::Error> {
        Ok(rmcp::model::ListResourcesResult {
            resources: vec![rmcp::model::Annotated::new(
                rmcp::model::RawResource {
                    name: "specifications".into(),
                    uri: "/specifications".into(),
                    description: Some("Specifications directory".into()),
                    mime_type: Some("inode/directory".into()),
                    size: None,
                },
                None,
            )],
            next_cursor: None,
        })
    }
}
