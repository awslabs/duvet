use crate::{
    annotation::{self, AnnotationSet, AnnotationType},
    project::Project,
    reference::{self, Reference},
    target::TargetSet,
};
use duvet_core::{diagnostic::IntoDiagnostic, path::Path};
use rmcp::{
    Error, ServerHandler,
    model::{CallToolResult, Content},
    serde_json::{Map, Value, json},
    tool,
};
use std::{collections::HashSet, sync::Arc};

const PROMPT: &str = include_str!("./system_prompt.md");

#[derive(Clone)]
pub struct Server {
    project: Project,
    peer: Arc<tokio::sync::Mutex<Option<rmcp::Peer<rmcp::RoleServer>>>>,
}

impl Server {
    pub async fn new(project: Project) -> std::result::Result<Self, duvet_core::diagnostic::Error> {
        Ok(Self {
            project,
            peer: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    async fn annotations(&self) -> std::result::Result<AnnotationSet, rmcp::Error> {
        let project_sources = match self.project.sources().await {
            Ok(s) => s,
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to get project sources",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };
        let project_sources = Arc::new(project_sources);

        let annotations = match annotation::query(project_sources).await {
            Ok(a) => a,
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to query annotations",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        Ok(annotations)
    }

    async fn references(&self) -> std::result::Result<Arc<[Reference]>, rmcp::Error> {
        let annotations = match self.annotations().await {
            Ok(a) => a,
            Err(e) => return Err(e),
        };

        let download_path = match self.project.download_path().await {
            Ok(p) => p.clone(),
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to get download path",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        let mut set = TargetSet::new();
        for anno in annotations.iter() {
            let target = match anno.target() {
                Ok(t) => t,
                Err(e) => {
                    return Err(Error::internal_error(
                        "failed to get target",
                        Some(json!({"message": e.to_string()})),
                    ));
                }
            };
            set.insert(target);
        }

        let specifications = match crate::target::query(&set, download_path).await {
            Ok(s) => s,
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to get specifications",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        let reference_map = match annotation::reference_map(annotations).await {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to create reference map",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        let references = match reference::query(reference_map, specifications).await {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to query references",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        Ok(references)
    }
}

#[tool(tool_box)]
impl Server {
    #[tool(description = "Count all of the annotations in the project sources")]
    async fn count_annotations(&self) -> std::result::Result<CallToolResult, rmcp::Error> {
        let annotations = self.annotations().await?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "There are {} annotations across all of the scanned sources in the codebase",
            annotations.len()
        ))]))
    }

    #[tool(
        description = "Validate a citation string to ensure it references an existing specification, section, and requirement"
    )]
    async fn validate_citation(
        &self,
        args: Map<String, Value>,
    ) -> std::result::Result<CallToolResult, rmcp::Error> {
        let citation = args
            .get("citation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::invalid_params(
                    "missing citation parameter",
                    Some(Value::String("citation".to_string())),
                )
            })?;

        // Parse the citation to extract URL and section
        let parts: Vec<&str> = citation.split('#').collect();
        if parts.len() != 2 {
            return Ok(CallToolResult::success(vec![Content::text(
                json!({"valid": false, "error": "Invalid URL format - missing section anchor"})
                    .to_string(),
            )]));
        }

        let url = parts[0].trim();
        let section = parts[1].trim();

        // Get all specifications
        let download_path = match self.project.download_path().await {
            Ok(p) => p.clone(),
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to get download path",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        let annotations = self.annotations().await?;

        let mut set = TargetSet::new();
        for anno in annotations.iter() {
            let target = match anno.target() {
                Ok(t) => t,
                Err(e) => {
                    return Err(Error::internal_error(
                        "failed to get target",
                        Some(json!({"message": e.to_string()})),
                    ));
                }
            };
            set.insert(target);
        }

        let specifications = match crate::target::query(&set, download_path).await {
            Ok(s) => s,
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to get specifications",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        // Check if specification exists
        let spec = specifications.iter().find(|s| s.0.path.to_string() == url);
        if spec.is_none() {
            return Ok(CallToolResult::success(vec![Content::text(
                json!({"valid": false, "error": "Specification not found"}).to_string(),
            )]));
        }

        // Check if section exists in specification
        let spec = spec.unwrap();
        let section_exists = spec.1.sections.contains_key(section);
        if !section_exists {
            return Ok(CallToolResult::success(vec![Content::text(
                json!({"valid": false, "error": "Section not found"}).to_string(),
            )]));
        }

        Ok(CallToolResult::success(vec![Content::text(
            json!({"valid": true}).to_string(),
        )]))
    }

    #[tool(
        description = "Search for requirements across all specifications using keywords or phrases"
    )]
    async fn search_requirements(
        &self,
        args: Map<String, Value>,
    ) -> std::result::Result<CallToolResult, rmcp::Error> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            Error::invalid_params(
                "missing query parameter",
                Some(Value::String("query".to_string())),
            )
        })?;

        let references = self.references().await?;

        // Filter references that match the query
        let matches: Vec<_> = references
            .iter()
            .filter(|r| {
                r.text
                    .as_ref()
                    .to_lowercase()
                    .contains(&query.to_lowercase())
            })
            .map(|r| {
                json!({
                    "identifier": r.annotation.id.to_string(),
                    "full_path": format!("{}", r.target.path),
                    "text": r.text.as_ref().to_string()
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            json!(matches).to_string(),
        )]))
    }

    #[tool(description = "Get the status of a specific requirement")]
    async fn get_requirement_status(
        &self,
        args: Map<String, Value>,
    ) -> std::result::Result<CallToolResult, rmcp::Error> {
        let req_identifier = args
            .get("req_identifier")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::invalid_params(
                    "missing req_identifier parameter",
                    Some(Value::String("req_identifier".to_string())),
                )
            })?;

        let references = self.references().await?;

        // Find the requirement
        let requirement = references
            .iter()
            .find(|r| r.annotation.id.to_string() == req_identifier);
        if requirement.is_none() {
            return Ok(CallToolResult::success(vec![Content::text(
                json!({"error": "Requirement not found"}).to_string(),
            )]));
        }

        // For now, just return a simple status
        // TODO: Implement actual status tracking
        Ok(CallToolResult::success(vec![Content::text(
            json!({"status": "not started"}).to_string(),
        )]))
    }

    #[tool(description = "List all requirements without citations in the codebase")]
    async fn list_uncited_requirements(&self) -> std::result::Result<CallToolResult, rmcp::Error> {
        let references = self.references().await?;

        // Filter references that have no citations
        let uncited: Vec<_> = references
            .iter()
            .filter(|r| r.annotation.anno != AnnotationType::Citation)
            .map(|r| {
                json!({
                    "identifier": r.annotation.id.to_string(),
                    "full_path": format!("{}", r.target.path),
                    "text": r.text.as_ref().to_string()
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![Content::text(
            json!(uncited).to_string(),
        )]))
    }

    #[tool(description = "List all invalid citations in the codebase")]
    async fn list_invalid_citations(&self) -> std::result::Result<CallToolResult, rmcp::Error> {
        let annotations = self.annotations().await?;

        // For now, return an empty list
        // TODO: Implement actual citation validation
        let invalid_citations: Vec<Value> = Vec::new();

        Ok(CallToolResult::success(vec![Content::text(
            json!(invalid_citations).to_string(),
        )]))
    }

    #[tool(description = "Get the code context surrounding a specific citation")]
    async fn get_citation_context(
        &self,
        args: Map<String, Value>,
    ) -> std::result::Result<CallToolResult, rmcp::Error> {
        let citation_id = args
            .get("citation_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::invalid_params(
                    "missing citation_id parameter",
                    Some(Value::String("citation_id".to_string())),
                )
            })?;

        let context_lines = args
            .get("context_lines")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                Error::invalid_params(
                    "missing context_lines parameter",
                    Some(Value::String("context_lines".to_string())),
                )
            })?;

        // Parse citation_id to get file path and line number
        let parts: Vec<&str> = citation_id.split(':').collect();
        if parts.len() != 2 {
            return Ok(CallToolResult::success(vec![Content::text(
                json!({"error": "Invalid citation ID format"}).to_string(),
            )]));
        }

        let file_path = parts[0];
        let line_number: i32 = parts[1].parse().map_err(|_| {
            Error::internal_error(
                "encountered an error",
                Some(json!({"message": "Invalid line number"})),
            )
        })?;

        // For now, return a simple context
        // TODO: Implement actual file reading and context extraction
        Ok(CallToolResult::success(vec![Content::text(
            json!({
                "file_path": file_path,
                "line_number": line_number,
                "context": ["// Example context line"]
            })
            .to_string(),
        )]))
    }

    #[tool(description = "Resolve a specification ID from a given URL")]
    async fn resolve_spec_id(
        &self,
        args: Map<String, Value>,
    ) -> std::result::Result<CallToolResult, rmcp::Error> {
        let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
            Error::invalid_params(
                "missing url parameter",
                Some(Value::String("url".to_string())),
            )
        })?;

        let download_path = match self.project.download_path().await {
            Ok(p) => p.clone(),
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to get download path",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        let annotations = self.annotations().await?;

        let mut set = TargetSet::new();
        for anno in annotations.iter() {
            let target = match anno.target() {
                Ok(t) => t,
                Err(e) => {
                    return Err(Error::internal_error(
                        "failed to get target",
                        Some(json!({"message": e.to_string()})),
                    ));
                }
            };
            set.insert(target);
        }

        let specifications = match crate::target::query(&set, download_path).await {
            Ok(s) => s,
            Err(e) => {
                return Err(Error::internal_error(
                    "failed to get specifications",
                    Some(json!({"message": e.to_string()})),
                ));
            }
        };

        // Find specification with matching URL
        let spec = specifications.iter().find(|s| s.0.path.to_string() == url);
        if spec.is_none() {
            return Ok(CallToolResult::success(vec![Content::text(
                json!({"error": "Specification not found"}).to_string(),
            )]));
        }

        Ok(CallToolResult::success(vec![Content::text(
            json!({"spec_id": spec.unwrap().0.path.to_string()}).to_string(),
        )]))
    }

    #[tool(description = "Get a list of all requirements ordered by priority")]
    async fn get_prioritized_requirements(
        &self,
    ) -> std::result::Result<CallToolResult, rmcp::Error> {
        let references = self.references().await?;

        // Convert references to prioritized list
        let mut requirements: Vec<_> = references
            .iter()
            .map(|r| {
                json!({
                    "full_path": format!("{}", r.target.path),
                    "level": r.annotation.level.to_string(),
                    "status": "not_started", // TODO: Calculate actual status
                    "todo_count": 0 // TODO: Count actual TODOs
                })
            })
            .collect();

        // Sort by priority (MUST > SHOULD > MAY)
        requirements.sort_by(|a, b| {
            let level_a = a["level"].as_str().unwrap();
            let level_b = b["level"].as_str().unwrap();
            level_b.cmp(level_a)
        });

        Ok(CallToolResult::success(vec![Content::text(
            json!(requirements).to_string(),
        )]))
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
