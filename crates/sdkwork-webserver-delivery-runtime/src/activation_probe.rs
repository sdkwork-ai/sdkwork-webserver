use std::{collections::HashSet, sync::Arc, time::Duration};

use futures_util::{stream, StreamExt, TryStreamExt};
use sdkwork_webserver_contract::provider::WebsiteRequestConditions;
use sdkwork_webserver_core::website_runtime::{
    CompiledWebsiteRuntimeSet, WebsiteBinding, WebsiteBindingAction, WebsiteHandler, WebsiteMount,
    WebsiteRuntimeRegistry, WebsiteRuntimeSetError,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    WebsiteDeliveryError, WebsiteDeliveryExecutor, WebsiteDeliveryMethod, WebsiteDeliveryOutcome,
    WebsiteDeliveryRedirect, WebsiteDeliveryRequest, WebsiteDeliveryRoutingContext,
    WebsiteDeliveryScheme, WebsiteProviderRegistry,
};

const MAXIMUM_ACTIVATION_PROBES: usize = 65_536;
const MAXIMUM_ACTIVATION_PROBE_DURATION: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebsiteRuntimeActivationProbeReport {
    pub probed_bindings: usize,
    pub probed_variants: usize,
    pub probed_routes: usize,
}

#[derive(Debug, Error)]
pub enum WebsiteRuntimeActivationProbeError {
    #[error("website runtime activation probe concurrency must be greater than zero")]
    InvalidConcurrency,
    #[error("website runtime set requires {actual} activation probes; maximum is {maximum}")]
    TooManyProbes { actual: usize, maximum: usize },
    #[error("website runtime set cannot be staged for activation: {0}")]
    RuntimeSet(#[from] WebsiteRuntimeSetError),
    #[error(
        "Site {app_uuid} Variant {variant_uuid} has no content-resolvable activation entrypoint"
    )]
    MissingEntrypoint {
        app_uuid: String,
        variant_uuid: String,
    },
    #[error(
        "activation probe delivery failed for Site {app_uuid} Binding {binding_uuid}: {source}"
    )]
    Delivery {
        app_uuid: String,
        binding_uuid: String,
        #[source]
        source: WebsiteDeliveryError,
    },
    #[error(
        "activation probe did not resolve the expected route for Site {app_uuid} Binding {binding_uuid}"
    )]
    RouteNotResolved {
        app_uuid: String,
        binding_uuid: String,
    },
    #[error("website runtime activation probes exceeded the bounded execution deadline")]
    DeadlineExceeded,
}

pub async fn probe_website_runtime_set_activation(
    runtime_set: Arc<CompiledWebsiteRuntimeSet>,
    providers: Arc<WebsiteProviderRegistry>,
    maximum_concurrency: usize,
) -> Result<WebsiteRuntimeActivationProbeReport, WebsiteRuntimeActivationProbeError> {
    if maximum_concurrency == 0 {
        return Err(WebsiteRuntimeActivationProbeError::InvalidConcurrency);
    }

    let staged_registry = Arc::new(WebsiteRuntimeRegistry::new(
        runtime_set.node_uuid(),
        runtime_set.environment(),
    ));
    staged_registry.activate(Arc::clone(&runtime_set))?;
    let executor = Arc::new(WebsiteDeliveryExecutor::new(staged_registry, providers));
    let (jobs, probed_bindings, probed_variants) = activation_probe_jobs(&runtime_set)?;
    let probed_routes = jobs.len();
    if jobs.is_empty() {
        return Ok(WebsiteRuntimeActivationProbeReport {
            probed_bindings,
            probed_variants,
            probed_routes,
        });
    }

    let probes = stream::iter(jobs)
        .map(|job| {
            let executor = Arc::clone(&executor);
            async move { execute_activation_probe(executor, job).await }
        })
        .buffer_unordered(maximum_concurrency)
        .try_collect::<Vec<_>>();
    timeout(MAXIMUM_ACTIVATION_PROBE_DURATION, probes)
        .await
        .map_err(|_| WebsiteRuntimeActivationProbeError::DeadlineExceeded)??;

    Ok(WebsiteRuntimeActivationProbeReport {
        probed_bindings,
        probed_variants,
        probed_routes,
    })
}

#[derive(Clone, Copy)]
enum ProbeExpectation {
    BindingRedirect,
    ServedRoute,
}

struct ActivationProbeJob {
    app_uuid: String,
    binding_uuid: String,
    authority: String,
    path: String,
    variant_uuid: Option<String>,
    spa_fallback_eligible: bool,
    expectation: ProbeExpectation,
    ordinal: usize,
    generation: u64,
}

fn activation_probe_jobs(
    runtime_set: &CompiledWebsiteRuntimeSet,
) -> Result<(Vec<ActivationProbeJob>, usize, usize), WebsiteRuntimeActivationProbeError> {
    let mut jobs = Vec::new();
    let mut probed_variants = HashSet::new();
    let mut probed_bindings = 0;

    for compiled in runtime_set.descriptors() {
        let descriptor = compiled.descriptor();
        let mut covered_variants = HashSet::new();
        let mut preference_binding = None;
        for binding in &descriptor.bindings {
            probed_bindings += 1;
            match &binding.action {
                WebsiteBindingAction::Redirect { .. } => push_probe_job(
                    &mut jobs,
                    ActivationProbeJob {
                        app_uuid: descriptor.app_uuid.clone(),
                        binding_uuid: binding.binding_uuid.clone(),
                        authority: probe_authority(&binding.hostname),
                        path: binding.path_prefix.clone(),
                        variant_uuid: None,
                        spa_fallback_eligible: false,
                        expectation: ProbeExpectation::BindingRedirect,
                        ordinal: 0,
                        generation: runtime_set.generation(),
                    },
                )?,
                WebsiteBindingAction::Serve {
                    default_variant_uuid,
                    forced_variant_uuid,
                } => {
                    let variant_uuid = forced_variant_uuid
                        .as_deref()
                        .or(default_variant_uuid.as_deref())
                        .unwrap_or(&descriptor.app_default_variant_uuid);
                    push_serve_probe(
                        &mut jobs,
                        descriptor,
                        binding,
                        variant_uuid,
                        runtime_set.generation(),
                    )?;
                    covered_variants.insert(variant_uuid.to_owned());
                    probed_variants.insert((descriptor.app_uuid.clone(), variant_uuid.to_owned()));
                    if forced_variant_uuid.is_none() && preference_binding.is_none() {
                        preference_binding = Some(binding);
                    }
                }
            }
        }

        if let Some(binding) = preference_binding {
            for variant in &descriptor.variants {
                if covered_variants.insert(variant.variant_uuid.clone()) {
                    push_serve_probe(
                        &mut jobs,
                        descriptor,
                        binding,
                        &variant.variant_uuid,
                        runtime_set.generation(),
                    )?;
                    probed_variants
                        .insert((descriptor.app_uuid.clone(), variant.variant_uuid.clone()));
                }
            }
        }
    }

    Ok((jobs, probed_bindings, probed_variants.len()))
}

fn push_serve_probe(
    jobs: &mut Vec<ActivationProbeJob>,
    descriptor: &sdkwork_webserver_core::website_runtime::WebsiteRuntimeDescriptor,
    binding: &WebsiteBinding,
    variant_uuid: &str,
    generation: u64,
) -> Result<(), WebsiteRuntimeActivationProbeError> {
    let mount = select_activation_mount(descriptor, variant_uuid).ok_or_else(|| {
        WebsiteRuntimeActivationProbeError::MissingEntrypoint {
            app_uuid: descriptor.app_uuid.clone(),
            variant_uuid: variant_uuid.to_owned(),
        }
    })?;
    push_probe_job(
        jobs,
        ActivationProbeJob {
            app_uuid: descriptor.app_uuid.clone(),
            binding_uuid: binding.binding_uuid.clone(),
            authority: probe_authority(&binding.hostname),
            path: activation_path(&binding.path_prefix, &mount.path_prefix),
            variant_uuid: Some(variant_uuid.to_owned()),
            spa_fallback_eligible: mount.handler == WebsiteHandler::Spa,
            expectation: ProbeExpectation::ServedRoute,
            ordinal: 0,
            generation,
        },
    )
}

fn push_probe_job(
    jobs: &mut Vec<ActivationProbeJob>,
    mut job: ActivationProbeJob,
) -> Result<(), WebsiteRuntimeActivationProbeError> {
    let actual = jobs.len().saturating_add(1);
    if actual > MAXIMUM_ACTIVATION_PROBES {
        return Err(WebsiteRuntimeActivationProbeError::TooManyProbes {
            actual,
            maximum: MAXIMUM_ACTIVATION_PROBES,
        });
    }
    job.ordinal = jobs.len();
    jobs.push(job);
    Ok(())
}

fn select_activation_mount<'a>(
    descriptor: &'a sdkwork_webserver_core::website_runtime::WebsiteRuntimeDescriptor,
    variant_uuid: &str,
) -> Option<&'a WebsiteMount> {
    descriptor
        .mounts
        .iter()
        .filter(|mount| mount.variant_uuid == variant_uuid && mount_is_probeable(mount))
        .min_by(|left, right| {
            (
                left.path_prefix != "/",
                left.path_prefix.len(),
                left.path_prefix.as_str(),
                left.mount_uuid.as_str(),
            )
                .cmp(&(
                    right.path_prefix != "/",
                    right.path_prefix.len(),
                    right.path_prefix.as_str(),
                    right.mount_uuid.as_str(),
                ))
        })
}

fn mount_is_probeable(mount: &WebsiteMount) -> bool {
    match mount.handler {
        WebsiteHandler::Wiki => true,
        WebsiteHandler::Spa => !mount.index_files.is_empty() || mount.spa_fallback.is_some(),
        WebsiteHandler::Static => !mount.index_files.is_empty(),
    }
}

fn probe_authority(hostname: &str) -> String {
    hostname
        .strip_prefix("*.")
        .map_or_else(|| hostname.to_owned(), |suffix| format!("a.{suffix}"))
}

fn activation_path(binding_path: &str, mount_path: &str) -> String {
    let joined = match (binding_path, mount_path) {
        ("/", mount) => mount.to_owned(),
        (binding, "/") => binding.to_owned(),
        (binding, mount) => format!("{binding}{mount}"),
    };
    if joined == "/" || joined.ends_with('/') {
        joined
    } else {
        format!("{joined}/")
    }
}

async fn execute_activation_probe(
    executor: Arc<WebsiteDeliveryExecutor>,
    job: ActivationProbeJob,
) -> Result<(), WebsiteRuntimeActivationProbeError> {
    let identity = format!(
        "website-activation-probe-{}-{}",
        job.generation, job.ordinal
    );
    let outcome = executor
        .execute_activation_probe(WebsiteDeliveryRequest {
            authority: job.authority,
            path: job.path,
            scheme: WebsiteDeliveryScheme::Https,
            method: WebsiteDeliveryMethod::Head,
            request_id: identity.clone(),
            trace_id: identity,
            routing: WebsiteDeliveryRoutingContext {
                verified_preferred_variant_uuid: job.variant_uuid.clone(),
                ..WebsiteDeliveryRoutingContext::default()
            },
            conditions: WebsiteRequestConditions::default(),
            range: None,
            locale: None,
            spa_fallback_eligible: job.spa_fallback_eligible,
        })
        .await
        .map_err(|source| WebsiteRuntimeActivationProbeError::Delivery {
            app_uuid: job.app_uuid.clone(),
            binding_uuid: job.binding_uuid.clone(),
            source,
        })?;

    let resolved = match (job.expectation, outcome) {
        (
            ProbeExpectation::BindingRedirect,
            WebsiteDeliveryOutcome::Redirect(WebsiteDeliveryRedirect::Binding { .. }),
        ) => true,
        (ProbeExpectation::ServedRoute, WebsiteDeliveryOutcome::Content(content)) => {
            content.route.binding_uuid == job.binding_uuid
                && job
                    .variant_uuid
                    .as_deref()
                    .is_some_and(|variant_uuid| content.route.variant_uuid == variant_uuid)
        }
        (
            ProbeExpectation::ServedRoute,
            WebsiteDeliveryOutcome::Redirect(WebsiteDeliveryRedirect::Wiki { route, .. }),
        ) => {
            route.binding_uuid == job.binding_uuid
                && job
                    .variant_uuid
                    .as_deref()
                    .is_some_and(|variant_uuid| route.variant_uuid == variant_uuid)
        }
        _ => false,
    };
    if resolved {
        Ok(())
    } else {
        Err(WebsiteRuntimeActivationProbeError::RouteNotResolved {
            app_uuid: job.app_uuid,
            binding_uuid: job.binding_uuid,
        })
    }
}
