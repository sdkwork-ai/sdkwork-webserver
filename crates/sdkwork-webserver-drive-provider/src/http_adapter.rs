//! Split-deployment HTTP transport for the Drive website provider.
//!
//! MODULE_SPEC.md §4.2.3 allows a deliberate split deployment (the website data
//! plane running as its own container, without the Drive module in-process) to
//! keep calling the Drive Internal API. That is the only transport this module
//! implements, it is never selected by default, and the gateway selects it only
//! through the explicit `SDKWORK_WEBSERVER_DRIVE_PROVIDER_MODE=http` opt-in
//! whose endpoints are validated at startup.

use async_trait::async_trait;
use sdkwork_drive_internal_sdk::{
    api::DriveInternalPublishingApi,
    http::BinaryResponseStream,
    models::{DriveResourceResolution, ResolveDriveResourceRequest, WebsiteRoot},
    SdkworkError,
};

use crate::sdk::{DriveContentChunkStream, DriveWebsiteSdkClient};

/// Adapter that forwards the generated SDK's `BinaryResponseStream` chunk by
/// chunk, preserving its byte budget enforcement.
pub(crate) struct SdkBinaryChunkStream {
    source: Option<BinaryResponseStream>,
}

impl SdkBinaryChunkStream {
    pub(crate) fn new(source: BinaryResponseStream) -> Self {
        Self {
            source: Some(source),
        }
    }
}

#[async_trait]
impl DriveContentChunkStream for SdkBinaryChunkStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
        let Some(source) = self.source.as_mut() else {
            return Ok(None);
        };
        let chunk = source.next_chunk().await?;
        if chunk.is_none() {
            self.source = None;
        }
        Ok(chunk)
    }
}

#[async_trait]
impl DriveWebsiteSdkClient for DriveInternalPublishingApi {
    async fn retrieve_website_root(
        &self,
        website_root_uuid: &str,
    ) -> Result<WebsiteRoot, SdkworkError> {
        self.website_roots_retrieve(website_root_uuid).await
    }

    async fn resolve_resource(
        &self,
        request: &ResolveDriveResourceRequest,
    ) -> Result<DriveResourceResolution, SdkworkError> {
        self.drive_resources_resolve(request).await
    }

    async fn retrieve_content(
        &self,
        node_version_id: &str,
        scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        range: Option<&str>,
        if_match: Option<&str>,
        if_none_match: Option<&str>,
        if_range: Option<&str>,
        if_modified_since: Option<&str>,
        if_unmodified_since: Option<&str>,
    ) -> Result<Vec<u8>, SdkworkError> {
        self.drive_resource_content_retrieve(
            node_version_id,
            scope_type,
            scope_uuid,
            relative_path,
            pinned_generation,
            range,
            if_match,
            if_none_match,
            if_range,
            if_modified_since,
            if_unmodified_since,
        )
        .await
    }

    async fn retrieve_content_stream(
        &self,
        node_version_id: &str,
        scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        range: Option<&str>,
        if_match: Option<&str>,
        if_none_match: Option<&str>,
        if_range: Option<&str>,
        if_modified_since: Option<&str>,
        if_unmodified_since: Option<&str>,
    ) -> Result<Box<dyn DriveContentChunkStream>, SdkworkError> {
        let stream = self
            .drive_resource_content_retrieve_stream(
                node_version_id,
                scope_type,
                scope_uuid,
                relative_path,
                pinned_generation,
                range,
                if_match,
                if_none_match,
                if_range,
                if_modified_since,
                if_unmodified_since,
            )
            .await?;
        Ok(Box::new(SdkBinaryChunkStream::new(stream)))
    }
}
