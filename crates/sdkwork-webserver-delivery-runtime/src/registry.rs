use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use futures_util::{stream, StreamExt, TryStreamExt};
use sdkwork_webserver_contract::provider::{
    ValidateWebsiteResourceRequest, ValidatedWebsiteResource, WebsiteProviderError,
    WebsiteProviderErrorKind, WebsiteProviderPurpose, WebsiteProviderRuntimeContext,
    WebsiteStaticContentProvider, WebsiteWikiProvider,
};
use sdkwork_webserver_core::website_runtime::{
    CompiledWebsiteRuntimeSet, ProviderResourceReference, WebsiteBinding, WebsiteBindingAction,
    WebsiteHandler, WebsiteMount, WebsiteProviderType, WebsiteResource,
    WebsiteResourceCapabilities, WebsiteRuntimeDescriptor,
};
use tokio::time::timeout;

use crate::{WebsiteProviderRegistryError, WebsiteRuntimeProviderValidationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebsiteRuntimeProviderValidationReport {
    pub validated_resources: usize,
}

#[derive(Default)]
pub struct WebsiteProviderRegistry {
    static_providers: HashMap<WebsiteProviderType, Arc<dyn WebsiteStaticContentProvider>>,
    wiki_providers: HashMap<WebsiteProviderType, Arc<dyn WebsiteWikiProvider>>,
}

impl WebsiteProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_static(
        &mut self,
        provider_type: WebsiteProviderType,
        provider: Arc<dyn WebsiteStaticContentProvider>,
    ) -> Result<(), WebsiteProviderRegistryError> {
        if self.static_providers.contains_key(&provider_type) {
            return Err(WebsiteProviderRegistryError::DuplicateProvider {
                provider_type,
                capability: "static-content",
            });
        }
        self.static_providers.insert(provider_type, provider);
        Ok(())
    }

    pub fn register_wiki(
        &mut self,
        provider_type: WebsiteProviderType,
        provider: Arc<dyn WebsiteWikiProvider>,
    ) -> Result<(), WebsiteProviderRegistryError> {
        if self.wiki_providers.contains_key(&provider_type) {
            return Err(WebsiteProviderRegistryError::DuplicateProvider {
                provider_type,
                capability: "wiki",
            });
        }
        self.wiki_providers.insert(provider_type, provider);
        Ok(())
    }

    pub fn supports_wiki(&self, provider_type: WebsiteProviderType) -> bool {
        self.wiki_providers.contains_key(&provider_type)
    }

    pub fn supports_static(&self, provider_type: WebsiteProviderType) -> bool {
        self.static_providers.contains_key(&provider_type)
    }

    /// Validates one provider-backed application-config resource (Drive or
    /// Knowledgebase) against its registered provider before the data plane
    /// starts serving it. The provider must be registered, must validate the
    /// reference within the deadline, and must return consistent generation
    /// tokens and the required capabilities; any mismatch fails closed.
    pub async fn validate_provider_resource(
        &self,
        provider: &ProviderResourceReference,
        required_capabilities: &WebsiteResourceCapabilities,
        context: WebsiteProviderRuntimeContext,
    ) -> Result<ValidatedWebsiteResource, WebsiteRuntimeProviderValidationError> {
        let provider_type = provider.provider_type;
        let provider_resource_uuid = provider.provider_resource_uuid.clone();
        let capability = if required_capabilities.wiki_routes {
            ValidationCapability::Wiki
        } else {
            ValidationCapability::Static
        };
        let registered = match capability {
            ValidationCapability::Static => self
                .static_providers
                .get(&provider_type)
                .cloned()
                .map(ValidationProvider::Static),
            ValidationCapability::Wiki => self
                .wiki_providers
                .get(&provider_type)
                .cloned()
                .map(ValidationProvider::Wiki),
        }
        .ok_or(
            WebsiteRuntimeProviderValidationError::ProviderNotRegistered {
                provider_type,
                capability: capability.label(),
            },
        )?;
        let request = ValidateWebsiteResourceRequest {
            context: context.clone(),
            provider: provider.clone(),
            required_capabilities: required_capabilities.clone(),
        };
        let validated = timeout(
            Duration::from_millis(context.deadline_ms),
            registered.validate_resource(&request),
        )
        .await
        .map_err(|_| WebsiteRuntimeProviderValidationError::Provider {
            provider_type,
            provider_resource_uuid: provider_resource_uuid.clone(),
            kind: WebsiteProviderErrorKind::DeadlineExceeded,
        })?
        .map_err(|error| WebsiteRuntimeProviderValidationError::Provider {
            provider_type,
            provider_resource_uuid: provider_resource_uuid.clone(),
            kind: error.kind,
        })?;
        if validated.provider_resource_uuid != provider_resource_uuid
            || !valid_generation_token(&validated.provider_generation)
            || !valid_generation_token(&validated.public_generation)
            || !capabilities_include(&validated.capabilities, required_capabilities)
        {
            return Err(WebsiteRuntimeProviderValidationError::Provider {
                provider_type,
                provider_resource_uuid,
                kind: WebsiteProviderErrorKind::ContractMismatch,
            });
        }
        Ok(validated)
    }

    pub async fn validate_runtime_set(
        &self,
        runtime_set: &CompiledWebsiteRuntimeSet,
        maximum_concurrency: usize,
    ) -> Result<WebsiteRuntimeProviderValidationReport, WebsiteRuntimeProviderValidationError> {
        if maximum_concurrency == 0 {
            return Err(WebsiteRuntimeProviderValidationError::InvalidConcurrency);
        }
        let jobs = self.validation_jobs(runtime_set)?;
        let validated_resources = jobs.len();
        stream::iter(jobs)
            .map(validate_job)
            .buffer_unordered(maximum_concurrency)
            .try_collect::<Vec<_>>()
            .await?;
        Ok(WebsiteRuntimeProviderValidationReport {
            validated_resources,
        })
    }

    fn validation_jobs<'a>(
        &self,
        runtime_set: &'a CompiledWebsiteRuntimeSet,
    ) -> Result<Vec<ResourceValidationJob<'a>>, WebsiteRuntimeProviderValidationError> {
        let mut jobs = Vec::new();
        for compiled in runtime_set.descriptors() {
            let descriptor = compiled.descriptor();
            let binding = activation_binding(descriptor)?;
            let mut scheduled = HashSet::new();
            for mount in &descriptor.mounts {
                let resource = descriptor
                    .resources
                    .iter()
                    .find(|resource| resource.resource_uuid == mount.resource_uuid)
                    .ok_or(WebsiteRuntimeProviderValidationError::CompiledContractViolation)?;
                let capability = ValidationCapability::for_handler(mount.handler);
                if !scheduled.insert((resource.resource_uuid.as_str(), capability)) {
                    continue;
                }
                let provider = match capability {
                    ValidationCapability::Static => self
                        .static_providers
                        .get(&resource.provider.provider_type)
                        .cloned()
                        .map(ValidationProvider::Static),
                    ValidationCapability::Wiki => self
                        .wiki_providers
                        .get(&resource.provider.provider_type)
                        .cloned()
                        .map(ValidationProvider::Wiki),
                }
                .ok_or(
                    WebsiteRuntimeProviderValidationError::ProviderNotRegistered {
                        provider_type: resource.provider.provider_type,
                        capability: capability.label(),
                    },
                )?;
                jobs.push(ResourceValidationJob {
                    ordinal: jobs.len(),
                    runtime_set_generation: runtime_set.generation(),
                    descriptor,
                    binding,
                    mount,
                    resource,
                    provider,
                });
            }
        }
        Ok(jobs)
    }

    pub(crate) fn static_provider(
        &self,
        provider_type: WebsiteProviderType,
    ) -> Option<Arc<dyn WebsiteStaticContentProvider>> {
        self.static_providers.get(&provider_type).cloned()
    }

    pub(crate) fn wiki_provider(
        &self,
        provider_type: WebsiteProviderType,
    ) -> Option<Arc<dyn WebsiteWikiProvider>> {
        self.wiki_providers.get(&provider_type).cloned()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ValidationCapability {
    Static,
    Wiki,
}

impl ValidationCapability {
    fn for_handler(handler: WebsiteHandler) -> Self {
        match handler {
            WebsiteHandler::Static | WebsiteHandler::Spa => Self::Static,
            WebsiteHandler::Wiki => Self::Wiki,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Static => "static-content",
            Self::Wiki => "wiki",
        }
    }
}

enum ValidationProvider {
    Static(Arc<dyn WebsiteStaticContentProvider>),
    Wiki(Arc<dyn WebsiteWikiProvider>),
}

impl ValidationProvider {
    fn maximum_content_bytes(&self) -> u64 {
        match self {
            Self::Static(provider) => provider.maximum_content_bytes(),
            Self::Wiki(provider) => provider.maximum_content_bytes(),
        }
    }

    async fn validate_resource(
        &self,
        request: &ValidateWebsiteResourceRequest,
    ) -> Result<sdkwork_webserver_contract::provider::ValidatedWebsiteResource, WebsiteProviderError>
    {
        match self {
            Self::Static(provider) => provider.validate_resource(request).await,
            Self::Wiki(provider) => provider.validate_resource(request).await,
        }
    }
}

struct ResourceValidationJob<'a> {
    ordinal: usize,
    runtime_set_generation: u64,
    descriptor: &'a WebsiteRuntimeDescriptor,
    binding: &'a WebsiteBinding,
    mount: &'a WebsiteMount,
    resource: &'a WebsiteResource,
    provider: ValidationProvider,
}

async fn validate_job(
    job: ResourceValidationJob<'_>,
) -> Result<(), WebsiteRuntimeProviderValidationError> {
    let provider_type = job.resource.provider.provider_type;
    let provider_resource_uuid = job.resource.provider.provider_resource_uuid.clone();
    let requested_bytes = job.descriptor.delivery_policy.maximum_object_bytes;
    let maximum_bytes = job.provider.maximum_content_bytes();
    if requested_bytes > maximum_bytes {
        return Err(
            WebsiteRuntimeProviderValidationError::ObjectLimitUnsupported {
                provider_type,
                provider_resource_uuid,
                requested_bytes,
                maximum_bytes,
            },
        );
    }
    let request_identity = format!(
        "website-activation-{}-{}",
        job.runtime_set_generation, job.ordinal
    );
    let request = ValidateWebsiteResourceRequest {
        context: WebsiteProviderRuntimeContext {
            tenant_scope_hash: job.descriptor.tenant_scope_hash.clone(),
            app_uuid: job.descriptor.app_uuid.clone(),
            binding_uuid: job.binding.binding_uuid.clone(),
            variant_uuid: job.mount.variant_uuid.clone(),
            mount_uuid: job.mount.mount_uuid.clone(),
            resource_uuid: job.resource.resource_uuid.clone(),
            request_id: request_identity.clone(),
            trace_id: request_identity,
            deadline_ms: job.descriptor.delivery_policy.provider_timeout_ms,
            purpose: WebsiteProviderPurpose::Activation,
        },
        provider: job.resource.provider.clone(),
        required_capabilities: job.resource.capabilities.clone(),
    };
    let validated = timeout(
        Duration::from_millis(request.context.deadline_ms),
        job.provider.validate_resource(&request),
    )
    .await
    .map_err(|_| WebsiteRuntimeProviderValidationError::Provider {
        provider_type,
        provider_resource_uuid: provider_resource_uuid.clone(),
        kind: WebsiteProviderErrorKind::DeadlineExceeded,
    })?
    .map_err(|error| WebsiteRuntimeProviderValidationError::Provider {
        provider_type,
        provider_resource_uuid: provider_resource_uuid.clone(),
        kind: error.kind,
    })?;
    if validated.provider_resource_uuid != provider_resource_uuid
        || !valid_generation_token(&validated.provider_generation)
        || !valid_generation_token(&validated.public_generation)
        || !capabilities_include(&validated.capabilities, &request.required_capabilities)
    {
        return Err(WebsiteRuntimeProviderValidationError::Provider {
            provider_type,
            provider_resource_uuid,
            kind: WebsiteProviderErrorKind::ContractMismatch,
        });
    }
    Ok(())
}

fn activation_binding(
    descriptor: &WebsiteRuntimeDescriptor,
) -> Result<&WebsiteBinding, WebsiteRuntimeProviderValidationError> {
    descriptor
        .bindings
        .iter()
        .find(|binding| matches!(binding.action, WebsiteBindingAction::Serve { .. }))
        .or_else(|| descriptor.bindings.first())
        .ok_or(WebsiteRuntimeProviderValidationError::CompiledContractViolation)
}

fn valid_generation_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

fn capabilities_include(
    actual: &sdkwork_webserver_core::website_runtime::WebsiteResourceCapabilities,
    required: &sdkwork_webserver_core::website_runtime::WebsiteResourceCapabilities,
) -> bool {
    (!required.static_content || actual.static_content)
        && (!required.wiki_routes || actual.wiki_routes)
        && (!required.wiki_search || actual.wiki_search)
        && (!required.range_requests || actual.range_requests)
}
