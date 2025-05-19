use crate::{
    Result,
    annotation::{self, Annotation, AnnotationSet},
    project::Project,
    reference::{self, Reference},
    source::SourceFile,
    target::SpecificationMap,
};
use rmcp::ServerHandler;
use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};

pub struct Server {
    project_sources: Arc<HashSet<SourceFile>>,
    annotations: AnnotationSet,
    specifications: SpecificationMap,
    references: Arc<[Reference]>,
}

impl Server {
    pub async fn new(project: Project) -> Result<Self> {
        let project_sources = project.sources().await?;
        let project_sources = Arc::new(project_sources);

        let annotations = annotation::query(project_sources.clone()).await?;

        let download_path = project.download_path().await?;
        let specifications =
            annotation::specifications(annotations.clone(), download_path.clone()).await?;

        let reference_map = annotation::reference_map(annotations.clone()).await?;

        let references = reference::query(reference_map.clone(), specifications.clone()).await?;

        Ok(Self {
            project_sources,
            annotations,
            specifications,
            references,
        })
    }
}

impl ServerHandler for Server {
    fn ping(
        &self,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<(), rmcp::Error>> + Send + '_ {
        std::future::ready(Ok(()))
    }

    fn initialize(
        &self,
        request: rmcp::model::InitializeRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<rmcp::model::InitializeResult, rmcp::Error>> + Send + '_
    {
        std::future::ready(Ok(self.get_info()))
    }

    fn complete(
        &self,
        request: rmcp::model::CompleteRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<rmcp::model::CompleteResult, rmcp::Error>> + Send + '_
    {
        std::future::ready(Err(rmcp::Error::method_not_found::<
            rmcp::model::CompleteRequestMethod,
        >()))
    }

    fn set_level(
        &self,
        request: rmcp::model::SetLevelRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<(), rmcp::Error>> + Send + '_ {
        std::future::ready(Err(rmcp::Error::method_not_found::<
            rmcp::model::SetLevelRequestMethod,
        >()))
    }

    fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<rmcp::model::GetPromptResult, rmcp::Error>> + Send + '_
    {
        std::future::ready(Err(rmcp::Error::method_not_found::<
            rmcp::model::GetPromptRequestMethod,
        >()))
    }

    fn list_prompts(
        &self,
        request: Option<rmcp::model::PaginatedRequestParam>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<rmcp::model::ListPromptsResult, rmcp::Error>> + Send + '_
    {
        std::future::ready(Ok(rmcp::model::ListPromptsResult::default()))
    }

    async fn list_resources(
        &self,
        request: Option<rmcp::model::PaginatedRequestParam>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<rmcp::model::ListResourcesResult, rmcp::Error> {
        tracing::info!("list_resources called with request: {:?}", request);
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

    fn list_resource_templates(
        &self,
        request: Option<rmcp::model::PaginatedRequestParam>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<
        Output = std::result::Result<rmcp::model::ListResourceTemplatesResult, rmcp::Error>,
    > + Send
    + '_ {
        std::future::ready(Ok(rmcp::model::ListResourceTemplatesResult::default()))
    }

    fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<rmcp::model::ReadResourceResult, rmcp::Error>>
    + Send
    + '_ {
        std::future::ready(Err(rmcp::Error::method_not_found::<
            rmcp::model::ReadResourceRequestMethod,
        >()))
    }

    fn subscribe(
        &self,
        request: rmcp::model::SubscribeRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<(), rmcp::Error>> + Send + '_ {
        std::future::ready(Err(rmcp::Error::method_not_found::<
            rmcp::model::SubscribeRequestMethod,
        >()))
    }

    fn unsubscribe(
        &self,
        request: rmcp::model::UnsubscribeRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<(), rmcp::Error>> + Send + '_ {
        std::future::ready(Err(rmcp::Error::method_not_found::<
            rmcp::model::UnsubscribeRequestMethod,
        >()))
    }

    fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParam,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<rmcp::model::CallToolResult, rmcp::Error>> + Send + '_
    {
        std::future::ready(Err(rmcp::Error::method_not_found::<
            rmcp::model::CallToolRequestMethod,
        >()))
    }

    fn list_tools(
        &self,
        request: Option<rmcp::model::PaginatedRequestParam>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl Future<Output = std::result::Result<rmcp::model::ListToolsResult, rmcp::Error>> + Send + '_
    {
        std::future::ready(Ok(rmcp::model::ListToolsResult::default()))
    }

    fn on_cancelled(
        &self,
        notification: rmcp::model::CancelledNotificationParam,
    ) -> impl Future<Output = ()> + Send + '_ {
        std::future::ready(())
    }

    fn on_progress(
        &self,
        notification: rmcp::model::ProgressNotificationParam,
    ) -> impl Future<Output = ()> + Send + '_ {
        std::future::ready(())
    }

    fn on_initialized(&self) -> impl Future<Output = ()> + Send + '_ {
        tracing::info!("client initialized");
        std::future::ready(())
    }

    fn on_roots_list_changed(&self) -> impl Future<Output = ()> + Send + '_ {
        std::future::ready(())
    }

    fn get_peer(&self) -> Option<rmcp::Peer<rmcp::RoleServer>> {
        None
    }

    fn set_peer(&mut self, peer: rmcp::Peer<rmcp::RoleServer>) {
        drop(peer);
    }

    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::default()
    }
}
