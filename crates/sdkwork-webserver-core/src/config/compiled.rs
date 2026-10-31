use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use regex::Regex;

use super::{
    acme_webroot::{AcmeWebrootNotice, AcmeWebrootReport},
    parse_server_name,
    validate::normalize_server_name,
    CertificateConfig, CertificateSource, ConfigDiagnostic, ListenerConfig, MailConfig,
    MailServerConfig, ResourceConfig, RouteConfig, RoutePathType, ServerNameForm,
    StreamServerConfig, TlsPolicyConfig, UpstreamConfig, VirtualHostConfig, WebServerAppConfig,
    WebServerConfigError,
};

#[derive(Debug)]
pub struct CompiledWebServerApp {
    config: WebServerAppConfig,
    base_directory: PathBuf,
    listeners: HashMap<String, usize>,
    resources: HashMap<String, usize>,
    upstreams: HashMap<String, usize>,
    certificates: HashMap<String, usize>,
    tls_policies: HashMap<String, usize>,
    compiled_listeners: HashMap<String, CompiledListener>,
    compiled_hosts: Vec<CompiledVirtualHost>,
    static_roots: HashMap<String, PathBuf>,
    static_h5_roots: HashMap<String, PathBuf>,
    certificate_paths: HashMap<String, (PathBuf, PathBuf)>,
    /// Resolved optional `chainFile` (nginx `ssl_trusted_certificate` /
    /// client-auth trust chain) per certificate id.
    certificate_chain_paths: HashMap<String, PathBuf>,
    /// Resolved optional static OCSP response (`ssl_stapling_file`) per
    /// certificate id.
    certificate_ocsp_paths: HashMap<String, PathBuf>,
    upstream_tls_paths: HashMap<String, CompiledUpstreamTlsPaths>,
    /// Resolved `clientAuth.caCertificateFiles` per TLS policy (relative
    /// paths chrooted to the configuration directory).
    client_auth_ca_paths: HashMap<String, Vec<PathBuf>>,
    /// Resolved stream TLS terminate `clientAuth` CA files per stream id.
    stream_client_auth_ca_paths: HashMap<String, Vec<PathBuf>>,
    /// Resolved stream `proxy_ssl*` upstream TLS material per stream id.
    stream_upstream_tls_paths: HashMap<String, CompiledUpstreamTlsPaths>,
    acme_webroots: HashMap<String, PathBuf>,
}

#[derive(Debug)]
struct CompiledUpstreamTlsPaths {
    ca_certificates: Vec<PathBuf>,
    client_identity: Option<(PathBuf, PathBuf)>,
}

#[derive(Debug)]
struct CompiledListener {
    /// Exact names, including the exact half of a `.suffix` name.
    exact_hosts: HashMap<String, usize>,
    /// Leading wildcards (`*.suffix`) keyed by their suffix: a request host
    /// matches when it is a proper subdomain of that suffix (`host` ends with
    /// `.suffix`), which is the nginx any-depth leading-wildcard semantics;
    /// the longest matching suffix wins.
    leading_wildcard_hosts: HashMap<String, usize>,
    /// Trailing wildcards (`prefix.*`) keyed by their prefix: a request host
    /// matches when it starts with `prefix.` and has at least one further
    /// character; the longest matching prefix wins.
    trailing_wildcard_hosts: HashMap<String, usize>,
    /// Regex server names in authored order: virtual-host declaration order,
    /// then each `server_name` argument's order within that host.
    regex_hosts: Vec<(Regex, usize)>,
    default_host: Option<usize>,
}

/// Server-name tables accumulated for one listener while compiling.
#[derive(Default)]
struct ListenerHostTables {
    exact: HashMap<String, usize>,
    leading: HashMap<String, usize>,
    trailing: HashMap<String, usize>,
    regex: Vec<(Regex, usize)>,
}

#[derive(Debug)]
struct CompiledVirtualHost {
    config_index: usize,
    exact_routes: HashMap<String, Vec<usize>>,
    prefix_routes: PrefixMatcher,
    /// nginx regex locations in declaration order (`~` / `~*`).
    regex_routes: Vec<(Regex, usize)>,
    /// Named locations (`location @name`) indexed by their `@name` path.
    /// They are never part of request-path selection; only the bounded
    /// internal redirect resolves them.
    named_routes: HashMap<String, usize>,
}

#[derive(Debug, Default)]
struct PrefixMatcher {
    nodes: Vec<PrefixNode>,
}

#[derive(Debug, Default)]
struct PrefixNode {
    children: HashMap<u8, usize>,
    routes: Vec<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct SelectedRoute<'a> {
    pub virtual_host: &'a VirtualHostConfig,
    pub route: &'a RouteConfig,
    pub resource: &'a ResourceConfig,
}

impl CompiledWebServerApp {
    pub(crate) fn compile(
        config: WebServerAppConfig,
        base_directory: &Path,
    ) -> Result<Self, WebServerConfigError> {
        let base_directory = canonical_directory(base_directory_or_current(base_directory), "/")?;
        let listeners = index_by_id(config.listeners.iter().map(|item| item.id.as_str()));
        let resources = index_by_id(config.resources.iter().map(ResourceConfig::id));
        let upstreams = index_by_id(config.upstreams.iter().map(|item| item.id.as_str()));
        let certificates = index_by_id(config.certificates.iter().map(|item| item.id.as_str()));
        let tls_policies = index_by_id(config.tls_policies.iter().map(|item| item.id.as_str()));

        // Stock nginx does not stat `root` at configuration load: a missing
        // document root is simply a 404 at request time. The nginx plane keeps
        // that behavior; typed configurations still require their roots to
        // exist.
        let tolerate_missing_roots = config.nginx.native_compatibility;
        let mut static_roots = HashMap::new();
        let mut static_h5_roots = HashMap::new();
        for (index, resource) in config.resources.iter().enumerate() {
            if let ResourceConfig::Static {
                id, root, h5_root, ..
            } = resource
            {
                // Absolute roots (nginx `root` compatibility) are used
                // directly; relative roots stay chrooted to the
                // configuration directory (server.toml semantics).
                let resolved = resolve_static_root(
                    Path::new(root),
                    &base_directory,
                    &format!("/resources/{index}/root"),
                    tolerate_missing_roots,
                )?;
                static_roots.insert(id.clone(), resolved);
                if let Some(h5_root) = h5_root {
                    let resolved_h5 = resolve_static_root(
                        Path::new(h5_root),
                        &base_directory,
                        &format!("/resources/{index}/h5Root"),
                        tolerate_missing_roots,
                    )?;
                    static_h5_roots.insert(id.clone(), resolved_h5);
                }
            }
        }

        let mut certificate_paths = HashMap::new();
        let mut certificate_chain_paths = HashMap::new();
        let mut certificate_ocsp_paths = HashMap::new();
        for (index, certificate) in config.certificates.iter().enumerate() {
            let CertificateSource::ProtectedFile {
                certificate_file,
                private_key_file,
                chain_file,
                ocsp_stapling_file,
            } = &certificate.source;
            if let Some(chain_file) = chain_file {
                let chain_path = resolve_material_file(
                    &base_directory,
                    chain_file,
                    &format!("/certificates/{index}/source/chainFile"),
                    tolerate_missing_roots,
                )?;
                certificate_chain_paths.insert(certificate.id.clone(), chain_path);
            }
            if let Some(ocsp_file) = ocsp_stapling_file {
                let ocsp_path = resolve_material_file(
                    &base_directory,
                    ocsp_file,
                    &format!("/certificates/{index}/source/ocspStaplingFile"),
                    tolerate_missing_roots,
                )?;
                certificate_ocsp_paths.insert(certificate.id.clone(), ocsp_path);
            }
            let certificate_path = resolve_material_file(
                &base_directory,
                certificate_file,
                &format!("/certificates/{index}/source/certificateFile"),
                tolerate_missing_roots,
            )?;
            let private_key_path = resolve_material_file(
                &base_directory,
                private_key_file,
                &format!("/certificates/{index}/source/privateKeyFile"),
                tolerate_missing_roots,
            )?;
            certificate_paths.insert(certificate.id.clone(), (certificate_path, private_key_path));
        }

        let mut upstream_tls_paths = HashMap::new();
        for (index, upstream) in config.upstreams.iter().enumerate() {
            let Some(tls) = &upstream.tls else {
                continue;
            };
            let ca_certificates = tls
                .ca_certificate_files
                .iter()
                .enumerate()
                .map(|(file_index, configured)| {
                    resolve_protected_relative_file(
                        &base_directory,
                        configured,
                        &format!("/upstreams/{index}/tls/caCertificateFiles/{file_index}"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let client_identity = tls
                .client_certificate_file
                .as_deref()
                .zip(tls.client_private_key_file.as_deref())
                .map(|(certificate, private_key)| {
                    Ok::<_, WebServerConfigError>((
                        resolve_protected_relative_file(
                            &base_directory,
                            certificate,
                            &format!("/upstreams/{index}/tls/clientCertificateFile"),
                        )?,
                        resolve_protected_relative_file(
                            &base_directory,
                            private_key,
                            &format!("/upstreams/{index}/tls/clientPrivateKeyFile"),
                        )?,
                    ))
                })
                .transpose()?;
            upstream_tls_paths.insert(
                upstream.id.clone(),
                CompiledUpstreamTlsPaths {
                    ca_certificates,
                    client_identity,
                },
            );
        }

        let mut client_auth_ca_paths = HashMap::new();
        for (index, policy) in config.tls_policies.iter().enumerate() {
            let Some(auth) = &policy.client_auth else {
                continue;
            };
            let resolved = auth
                .ca_certificate_files
                .iter()
                .enumerate()
                .map(|(file_index, configured)| {
                    resolve_protected_relative_file(
                        &base_directory,
                        configured,
                        &format!("/tlsPolicies/{index}/clientAuth/caCertificateFiles/{file_index}"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            client_auth_ca_paths.insert(policy.id.clone(), resolved);
        }
        let mut stream_client_auth_ca_paths = HashMap::new();
        for (index, stream) in config.streams.iter().enumerate() {
            let Some(crate::config::model::StreamTlsMode::Terminate {
                client_auth: Some(auth),
                ..
            }) = &stream.tls
            else {
                continue;
            };
            let resolved = auth
                .ca_certificate_files
                .iter()
                .enumerate()
                .map(|(file_index, configured)| {
                    resolve_protected_relative_file(
                        &base_directory,
                        configured,
                        &format!("/streams/{index}/tls/clientAuth/caCertificateFiles/{file_index}"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            stream_client_auth_ca_paths.insert(stream.id.clone(), resolved);
        }
        let mut stream_upstream_tls_paths = HashMap::new();
        for (index, stream) in config.streams.iter().enumerate() {
            let Some(tls) = &stream.upstream_tls else {
                continue;
            };
            let ca_certificates = tls
                .ca_certificate_files
                .iter()
                .enumerate()
                .map(|(file_index, configured)| {
                    resolve_protected_relative_file(
                        &base_directory,
                        configured,
                        &format!("/streams/{index}/upstreamTls/caCertificateFiles/{file_index}"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let client_identity = tls
                .client_certificate_file
                .as_deref()
                .zip(tls.client_private_key_file.as_deref())
                .map(|(certificate, private_key)| {
                    Ok::<_, WebServerConfigError>((
                        resolve_protected_relative_file(
                            &base_directory,
                            certificate,
                            &format!("/streams/{index}/upstreamTls/clientCertificateFile"),
                        )?,
                        resolve_protected_relative_file(
                            &base_directory,
                            private_key,
                            &format!("/streams/{index}/upstreamTls/clientPrivateKeyFile"),
                        )?,
                    ))
                })
                .transpose()?;
            // The gateway builds the stream TLS client config through the
            // shared upstream TLS builder, keyed by a synthetic upstream id, so
            // the same resolved-material map and sandbox rules apply.
            let paths = CompiledUpstreamTlsPaths {
                ca_certificates: ca_certificates.clone(),
                client_identity: client_identity.clone(),
            };
            stream_upstream_tls_paths.insert(stream.id.clone(), paths);
            upstream_tls_paths.insert(
                format!("stream-tls:{}", stream.id),
                CompiledUpstreamTlsPaths {
                    ca_certificates,
                    client_identity,
                },
            );
        }
        let mut acme_webroots = HashMap::new();
        for (index, listener) in config.listeners.iter().enumerate() {
            let Some(acme) = &listener.acme_http_01 else {
                continue;
            };
            let resolved = resolve_acme_webroot(
                &base_directory,
                &acme.webroot,
                &format!("/listeners/{index}/acmeHttp01/webroot"),
                tolerate_missing_roots,
            )?;
            acme_webroots.insert(listener.id.clone(), resolved);
        }

        let compiled_hosts = config
            .virtual_hosts
            .iter()
            .enumerate()
            .map(|(config_index, virtual_host)| {
                CompiledVirtualHost::compile(config_index, virtual_host)
            })
            .collect::<Vec<_>>();

        let host_indexes = config
            .virtual_hosts
            .iter()
            .enumerate()
            .map(|(index, host)| (host.id.as_str(), index))
            .collect::<HashMap<_, _>>();
        // Single pass over virtual hosts instead of an O(listeners x hosts)
        // nested scan: each host contributes its server names once to every
        // listener it references, keeping compilation linear in the total
        // declared server-name count. The regex table is filled in the same
        // pass so its order is the nginx authored order.
        let mut listener_hosts: HashMap<&str, ListenerHostTables> = HashMap::new();
        for (host_index, virtual_host) in config.virtual_hosts.iter().enumerate() {
            for listener_ref in &virtual_host.listener_refs {
                let tables = listener_hosts.entry(listener_ref.as_str()).or_default();
                for server_name in &virtual_host.server_names {
                    let form = parse_server_name(server_name).ok_or_else(|| {
                        validation_error(
                            format!("/virtualHosts/{host_index}/serverNames"),
                            format!(
                                "server name {server_name} was not accepted after semantic validation"
                            ),
                        )
                    })?;
                    let name_path = format!("/virtualHosts/{host_index}/serverNames");
                    match form {
                        ServerNameForm::Regex(regex) => {
                            let compiled =
                                Regex::new(&regex.compiled_pattern()).map_err(|error| {
                                    validation_error(
                                        &name_path,
                                        format!("invalid server_name regex: {error}"),
                                    )
                                })?;
                            tables.regex.push((compiled, host_index));
                        }
                        ServerNameForm::Exact(name) => {
                            insert_server_name(&mut tables.exact, name, host_index, &name_path)?;
                        }
                        ServerNameForm::LeadingWildcard(suffix) => {
                            insert_server_name(&mut tables.leading, suffix, host_index, &name_path)?
                        }
                        ServerNameForm::TrailingWildcard(prefix) => insert_server_name(
                            &mut tables.trailing,
                            prefix,
                            host_index,
                            &name_path,
                        )?,
                        ServerNameForm::DotPrefix(suffix) => {
                            // nginx `.example.com` claims the exact name and
                            // every subdomain, so it fills both tables.
                            insert_server_name(
                                &mut tables.exact,
                                suffix.clone(),
                                host_index,
                                &name_path,
                            )?;
                            insert_server_name(
                                &mut tables.leading,
                                suffix,
                                host_index,
                                &name_path,
                            )?;
                        }
                    }
                }
            }
        }
        let mut compiled_listeners = HashMap::new();
        for listener in &config.listeners {
            let tables = listener_hosts
                .remove(listener.id.as_str())
                .unwrap_or_default();
            let default_host = listener
                .default_virtual_host_ref
                .as_deref()
                .and_then(|id| host_indexes.get(id).copied());
            compiled_listeners.insert(
                listener.id.clone(),
                CompiledListener {
                    exact_hosts: tables.exact,
                    leading_wildcard_hosts: tables.leading,
                    trailing_wildcard_hosts: tables.trailing,
                    regex_hosts: tables.regex,
                    default_host,
                },
            );
        }

        let app = Self {
            config,
            base_directory,
            listeners,
            resources,
            upstreams,
            certificates,
            tls_policies,
            compiled_listeners,
            compiled_hosts,
            static_roots,
            static_h5_roots,
            certificate_paths,
            certificate_chain_paths,
            certificate_ocsp_paths,
            upstream_tls_paths,
            client_auth_ca_paths,
            stream_client_auth_ca_paths,
            stream_upstream_tls_paths,
            acme_webroots,
        };
        app.verify_acme_webroot_rendezvous()?;
        Ok(app)
    }

    /// Compare the directory this data plane serves ACME HTTP-01 challenges
    /// from with the directory the certificate worker writes them into.
    ///
    /// The two halves are configured in different files by different tools and
    /// were never compared, so a deployment could start healthy while every
    /// order failed at the CA with a bare 404. See [`super::acme_webroot`].
    ///
    /// Only a *verified* disagreement is fatal, and only in production-like
    /// environments: the unverifiable case (this process does not know the
    /// worker's write target) is legitimate in Kubernetes, where the worker
    /// carries that setting alone.
    fn verify_acme_webroot_rendezvous(&self) -> Result<(), WebServerConfigError> {
        let written = std::env::var(crate::runtime_env::ACME_WEBROOT_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let report = AcmeWebrootReport::collect(self, written);
        let production_like = crate::runtime_env::web_is_production_like_environment();
        match report.enforce(production_like) {
            Ok(None) => Ok(()),
            // The two notices deserve different volume: a disagreement means
            // HTTP-01 cannot work, while an unverifiable rendezvous is the
            // normal shape of a split-process deployment.
            Ok(Some(AcmeWebrootNotice::Disagreement(message))) => {
                tracing::warn!("{message}");
                Ok(())
            }
            Ok(Some(AcmeWebrootNotice::WriteTargetUnknown(message))) => {
                tracing::debug!("{message}");
                Ok(())
            }
            Err(message) => Err(validation_error("/listeners[].acmeHttp01/webroot", message)),
        }
    }
    pub fn config(&self) -> &WebServerAppConfig {
        &self.config
    }

    pub fn base_directory(&self) -> &Path {
        &self.base_directory
    }

    pub fn listeners(&self) -> impl Iterator<Item = &ListenerConfig> {
        self.config.listeners.iter()
    }

    pub fn streams(&self) -> &[StreamServerConfig] {
        &self.config.streams
    }

    /// The mail proxy plane (`nginx mail {}` listeners).
    pub fn mail(&self) -> &MailConfig {
        &self.config.mail
    }

    /// Configured mail listeners in authored order.
    pub fn mail_servers(&self) -> &[MailServerConfig] {
        &self.config.mail.servers
    }

    pub fn listener(&self, id: &str) -> Option<&ListenerConfig> {
        self.listeners
            .get(id)
            .map(|index| &self.config.listeners[*index])
    }

    /// Resolved ACME HTTP-01 webroot for a listener, when the listener
    /// configured `acmeHttp01`. The path is canonical and confined to the
    /// configuration directory at compile time.
    pub fn acme_webroot(&self, listener_id: &str) -> Option<&Path> {
        self.acme_webroots.get(listener_id).map(PathBuf::as_path)
    }

    pub fn resource(&self, id: &str) -> Option<&ResourceConfig> {
        self.resources
            .get(id)
            .map(|index| &self.config.resources[*index])
    }

    pub fn upstream(&self, id: &str) -> Option<&UpstreamConfig> {
        self.upstreams
            .get(id)
            .map(|index| &self.config.upstreams[*index])
    }

    pub fn tls_policy(&self, id: &str) -> Option<&TlsPolicyConfig> {
        self.tls_policies
            .get(id)
            .map(|index| &self.config.tls_policies[*index])
    }

    pub fn certificate(&self, id: &str) -> Option<&CertificateConfig> {
        self.certificates
            .get(id)
            .map(|index| &self.config.certificates[*index])
    }

    pub fn static_root(&self, resource_id: &str) -> Option<&Path> {
        self.static_roots.get(resource_id).map(PathBuf::as_path)
    }

    pub fn static_h5_root(&self, resource_id: &str) -> Option<&Path> {
        self.static_h5_roots.get(resource_id).map(PathBuf::as_path)
    }

    /// Every provider-backed resource (Drive or Knowledgebase) in this
    /// configuration, in declaration order. The data-plane bootstrap uses
    /// this to assemble provider connections and to fail closed when a
    /// referenced provider is unavailable.
    pub fn provider_resources(&self) -> impl Iterator<Item = &ResourceConfig> {
        self.config
            .resources
            .iter()
            .filter(|resource| resource.provider_type().is_some())
    }

    /// Resolved `clientAuth.caCertificateFiles` for a TLS policy.
    pub fn client_auth_ca_paths(&self, policy_id: &str) -> Option<&[PathBuf]> {
        self.client_auth_ca_paths.get(policy_id).map(Vec::as_slice)
    }

    /// Resolved stream TLS terminate `clientAuth` CA files for a stream.
    pub fn stream_client_auth_ca_paths(&self, stream_id: &str) -> Option<&[PathBuf]> {
        self.stream_client_auth_ca_paths
            .get(stream_id)
            .map(Vec::as_slice)
    }

    pub fn certificate_paths(&self, certificate_id: &str) -> Option<(&Path, &Path)> {
        self.certificate_paths
            .get(certificate_id)
            .map(|(certificate, private_key)| (certificate.as_path(), private_key.as_path()))
    }

    /// Optional nginx `ssl_trusted_certificate` / typed-TOML `chainFile` PEM
    /// chain for one certificate (downstream client-auth trust chain).
    pub fn certificate_chain_file(&self, certificate_id: &str) -> Option<&Path> {
        self.certificate_chain_paths
            .get(certificate_id)
            .map(PathBuf::as_path)
    }

    /// Resolved nginx `ssl_stapling_file` (static OCSP response) for one
    /// certificate, if declared.
    pub fn certificate_ocsp_stapling_file(&self, certificate_id: &str) -> Option<&Path> {
        self.certificate_ocsp_paths
            .get(certificate_id)
            .map(PathBuf::as_path)
    }

    /// Resolved stream `proxy_ssl*` CA bundle paths for one stream server id.
    pub fn stream_upstream_tls_ca_certificate_paths(&self, stream_id: &str) -> Option<&[PathBuf]> {
        self.stream_upstream_tls_paths
            .get(stream_id)
            .map(|paths| paths.ca_certificates.as_slice())
    }

    /// Resolved stream `proxy_ssl*` client identity (certificate, key) for one
    /// stream server id.
    pub fn stream_upstream_tls_client_identity_paths(
        &self,
        stream_id: &str,
    ) -> Option<(&Path, &Path)> {
        self.stream_upstream_tls_paths
            .get(stream_id)
            .and_then(|paths| paths.client_identity.as_ref())
            .map(|(certificate, private_key)| (certificate.as_path(), private_key.as_path()))
    }

    pub fn upstream_tls_ca_certificate_paths(&self, upstream_id: &str) -> Option<&[PathBuf]> {
        self.upstream_tls_paths
            .get(upstream_id)
            .map(|paths| paths.ca_certificates.as_slice())
    }

    pub fn upstream_tls_client_identity_paths(&self, upstream_id: &str) -> Option<(&Path, &Path)> {
        self.upstream_tls_paths
            .get(upstream_id)
            .and_then(|paths| paths.client_identity.as_ref())
            .map(|(certificate, private_key)| (certificate.as_path(), private_key.as_path()))
    }

    /// Resolve the virtual host selected by `authority` on `listener_id`,
    /// without selecting a route.
    ///
    /// The data plane needs the host-level response policy for a request that
    /// selects no route at all: nginx still applies `add_header` from the
    /// matched server context to its generated 404 (the headers filter runs for
    /// every response, not only for those produced by a location).
    pub fn select_virtual_host(
        &self,
        listener_id: &str,
        authority: &str,
    ) -> Option<&VirtualHostConfig> {
        let host_index = self.select_host_index(listener_id, authority)?;
        let compiled_host = self.compiled_hosts.get(host_index)?;
        self.config.virtual_hosts.get(compiled_host.config_index)
    }

    /// Resolve a request path inside the virtual host selected by `authority`.
    ///
    /// Host selection is nginx's order (`ngx_hash_find_combined`): exact,
    /// longest leading wildcard, longest trailing wildcard, then regexes in
    /// authored order, then the listener default server.
    pub fn select_route(
        &self,
        listener_id: &str,
        authority: &str,
        path: &str,
        method: &str,
    ) -> Option<SelectedRoute<'_>> {
        let host_index = self.select_host_index(listener_id, authority)?;
        let compiled_host = self.compiled_hosts.get(host_index)?;
        let virtual_host = self.config.virtual_hosts.get(compiled_host.config_index)?;
        let route_index = compiled_host.select_route(virtual_host, path, method)?;
        self.selected_route(virtual_host, route_index)
    }

    /// Resolve one named location (`@name`) inside the virtual host selected
    /// by `authority`.
    ///
    /// This is the bounded internal-redirect target of `try_files … @name`
    /// and `error_page … @name`. It is deliberately *not* part of
    /// [`Self::select_route`]: a request path can never select a named
    /// location.
    pub fn select_named_route(
        &self,
        listener_id: &str,
        authority: &str,
        name: &str,
        method: &str,
    ) -> Option<SelectedRoute<'_>> {
        let host_index = self.select_host_index(listener_id, authority)?;
        let compiled_host = self.compiled_hosts.get(host_index)?;
        let virtual_host = self.config.virtual_hosts.get(compiled_host.config_index)?;
        let route_index = compiled_host.select_named_route(virtual_host, name, method)?;
        self.selected_route(virtual_host, route_index)
    }

    fn selected_route<'a>(
        &'a self,
        virtual_host: &'a VirtualHostConfig,
        route_index: usize,
    ) -> Option<SelectedRoute<'a>> {
        let route = virtual_host.routes.get(route_index)?;
        let resource = self.resource(&route.resource_ref)?;
        Some(SelectedRoute {
            virtual_host,
            route,
            resource,
        })
    }

    fn select_host_index(&self, listener_id: &str, authority: &str) -> Option<usize> {
        let listener = self.compiled_listeners.get(listener_id)?;
        let normalized_host = normalize_authority_host(authority);
        if let Some(host) = normalized_host.as_deref() {
            if let Some(index) = listener.exact_hosts.get(host) {
                return Some(*index);
            }
            // nginx leading wildcard (`*.suffix`): any-depth subdomain
            // matches, the bare suffix domain does not, and the longest
            // matching suffix wins. Walk the host's dot boundaries left to
            // right (O(labels) hash lookups; the first hit is the longest
            // suffix) instead of scanning every wildcard per request.
            let mut candidate = host;
            while let Some((_, rest)) = candidate.split_once('.') {
                candidate = rest;
                if let Some(index) = listener.leading_wildcard_hosts.get(candidate) {
                    return Some(*index);
                }
            }
            // nginx trailing wildcard (`prefix.*`): the longest matching
            // prefix wins. Walk the host's dots right to left so the first
            // lookup is the longest prefix.
            let mut end = host.len();
            while let Some(position) = host[..end].rfind('.') {
                if position > 0 && host.len() > position + 1 {
                    if let Some(index) = listener.trailing_wildcard_hosts.get(&host[..position]) {
                        return Some(*index);
                    }
                }
                if position == 0 {
                    break;
                }
                end = position;
            }
        }
        // nginx evaluates server-name regexes against the lower-cased host
        // with the port and one trailing dot removed, after every wildcard
        // table missed.
        if let Some(host) = normalize_host_for_regex(authority) {
            for (regex, index) in &listener.regex_hosts {
                if regex.is_match(&host) {
                    return Some(*index);
                }
            }
        }
        listener.default_host
    }
}

fn insert_server_name(
    table: &mut HashMap<String, usize>,
    name: String,
    host_index: usize,
    path: &str,
) -> Result<(), WebServerConfigError> {
    match table.insert(name.clone(), host_index) {
        Some(previous) if previous != host_index => Err(validation_error(
            path,
            format!("server name {name} is already owned by another virtual host on this listener"),
        )),
        _ => Ok(()),
    }
}

/// The host string nginx matches server-name regexes against: lower-cased,
/// port removed, one trailing dot removed. `ngx_http_validate_host` also
/// rejects malformed hosts with a `400` before selection runs; that strictness
/// belongs to request parsing, so this helper only performs the normalization
/// the regex evaluation observes.
fn normalize_host_for_regex(authority: &str) -> Option<String> {
    let authority = authority.trim();
    if authority.is_empty() {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        return Some(format!("[{}]", rest[..end].to_ascii_lowercase()));
    }
    let host = match authority.rsplit_once(':') {
        Some((host, port))
            if !host.contains(':') && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            host
        }
        _ => authority,
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

impl CompiledVirtualHost {
    fn compile(config_index: usize, virtual_host: &VirtualHostConfig) -> Self {
        let mut exact_routes: HashMap<String, Vec<usize>> = HashMap::new();
        let mut prefix_routes = PrefixMatcher::new();
        let mut regex_routes = Vec::new();
        let mut named_routes = HashMap::new();
        for (route_index, route) in virtual_host.routes.iter().enumerate() {
            // `@name` exact routes are named locations: reachable only by the
            // bounded internal redirect, never by request-path selection.
            if route.route_match.path.starts_with('@') {
                named_routes.insert(route.route_match.path.clone(), route_index);
                continue;
            }
            match route.route_match.path_type {
                RoutePathType::Exact => exact_routes
                    .entry(route.route_match.path.clone())
                    .or_default()
                    .push(route_index),
                RoutePathType::Prefix | RoutePathType::PrefixExclusive => {
                    prefix_routes.insert(route.route_match.path.as_bytes(), route_index)
                }
                RoutePathType::Regex | RoutePathType::RegexIgnoreCase => {
                    let pattern =
                        if matches!(route.route_match.path_type, RoutePathType::RegexIgnoreCase) {
                            format!("(?i){}", route.route_match.path)
                        } else {
                            route.route_match.path.clone()
                        };
                    // Patterns are validated before compile; panic is not used —
                    // invalid patterns are skipped only if validation was bypassed.
                    if let Ok(regex) = Regex::new(&pattern) {
                        regex_routes.push((regex, route_index));
                    }
                }
            }
        }
        Self {
            config_index,
            exact_routes,
            prefix_routes,
            regex_routes,
            named_routes,
        }
    }

    fn select_named_route(
        &self,
        virtual_host: &VirtualHostConfig,
        name: &str,
        method: &str,
    ) -> Option<usize> {
        let index = *self.named_routes.get(name)?;
        route_matches_method(&virtual_host.routes[index], method).then_some(index)
    }

    fn select_route(
        &self,
        virtual_host: &VirtualHostConfig,
        path: &str,
        method: &str,
    ) -> Option<usize> {
        if let Some(route) = self.exact_routes.get(path).and_then(|routes| {
            routes
                .iter()
                .copied()
                .find(|index| route_matches_method(&virtual_host.routes[*index], method))
        }) {
            return Some(route);
        }

        let prefix = self.prefix_routes.select(path.as_bytes(), |index| {
            route_matches_method(&virtual_host.routes[index], method)
        });
        if let Some(index) = prefix {
            if matches!(
                virtual_host.routes[index].route_match.path_type,
                RoutePathType::PrefixExclusive
            ) {
                // nginx `^~`: longest exclusive prefix suppresses regex.
                return Some(index);
            }
        }

        for (regex, index) in &self.regex_routes {
            if regex.is_match(path) && route_matches_method(&virtual_host.routes[*index], method) {
                return Some(*index);
            }
        }

        prefix
    }
}

impl PrefixMatcher {
    fn new() -> Self {
        Self {
            nodes: vec![PrefixNode::default()],
        }
    }

    fn insert(&mut self, prefix: &[u8], route_index: usize) {
        let mut node_index = 0usize;
        for byte in prefix {
            let next = if let Some(index) = self.nodes[node_index].children.get(byte) {
                *index
            } else {
                let index = self.nodes.len();
                self.nodes.push(PrefixNode::default());
                self.nodes[node_index].children.insert(*byte, index);
                index
            };
            node_index = next;
        }
        self.nodes[node_index].routes.push(route_index);
    }

    fn select(&self, path: &[u8], matches: impl Fn(usize) -> bool) -> Option<usize> {
        let mut node_index = 0usize;
        let mut selected = self.nodes[0]
            .routes
            .iter()
            .copied()
            .find(|index| matches(*index));
        for byte in path {
            let Some(next) = self.nodes[node_index].children.get(byte).copied() else {
                break;
            };
            node_index = next;
            if let Some(route) = self.nodes[node_index]
                .routes
                .iter()
                .copied()
                .find(|index| matches(*index))
            {
                selected = Some(route);
            }
        }
        selected
    }
}

fn index_by_id<'a>(ids: impl Iterator<Item = &'a str>) -> HashMap<String, usize> {
    ids.enumerate()
        .map(|(index, id)| (id.to_owned(), index))
        .collect()
}

/// The directory a configuration file's relative roots are anchored to.
///
/// `Path::new("config.json").parent()` is `Some("")`, not `None`, so a caller
/// that passes `path.parent()` for a bare file name hands over an *empty* path.
/// Canonicalizing it fails with "directory  is unavailable" — a diagnostic with
/// no directory in it, produced by the most ordinary invocation there is
/// (`validate config.json` from the directory holding the file). An empty base
/// directory means the current directory, which is what the operator meant.
fn base_directory_or_current(directory: &Path) -> &Path {
    if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    }
}

fn canonical_directory(
    path: &Path,
    diagnostic_path: &str,
) -> Result<PathBuf, WebServerConfigError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        validation_error(
            diagnostic_path,
            format!("directory {} is unavailable: {error}", path.display()),
        )
    })?;
    if !canonical.is_dir() {
        return Err(validation_error(
            diagnostic_path,
            format!("{} is not a directory", canonical.display()),
        ));
    }
    Ok(canonical)
}

/// Resolve a required material file, tolerating an absent absolute path for
/// the nginx plane (nginx reads certificate material at startup; the runtime
/// reports an unavailable certificate at handshake instead of refusing to
/// validate the whole configuration file, which is what makes a relocated
/// production configuration validatable).
fn resolve_material_file(
    base_directory: &Path,
    configured: &str,
    diagnostic_path: &str,
    tolerate_missing: bool,
) -> Result<PathBuf, WebServerConfigError> {
    let configured_path = Path::new(configured);
    if tolerate_missing && configured_path.is_absolute() && !configured_path.exists() {
        Ok(configured_path.to_path_buf())
    } else {
        resolve_required_file(base_directory, configured, diagnostic_path)
    }
}

fn resolve_static_root(
    root: &Path,
    base_directory: &Path,
    diagnostic_path: &str,
    tolerate_missing: bool,
) -> Result<PathBuf, WebServerConfigError> {
    if root.is_absolute() {
        if tolerate_missing && !root.exists() {
            return Ok(root.to_path_buf());
        }
        return canonical_directory(root, diagnostic_path);
    }
    let resolved = canonical_directory(&base_directory.join(root), diagnostic_path)?;
    if !resolved.starts_with(base_directory) {
        return Err(validation_error(
            diagnostic_path,
            "static root escapes the configuration directory",
        ));
    }
    Ok(resolved)
}

/// Resolve one listener's `acmeHttp01.webroot`.
///
/// Same two rules as a static root — the directory must exist, and a relative
/// value must stay inside the configuration directory — because the request
/// path reads it through the same confined file opener. The one deliberate
/// difference is the absolute case: the canonical deployment value lives
/// *outside* the configuration directory, because the webroot is a rendezvous
/// between two processes and two planes. The certificate worker writes
/// challenge tokens into the directory named by
/// `SDKWORK_WEBSERVER_ACME_WEBROOT`
/// (`/var/lib/sdkwork/webserver/acme-webroot` on Linux and in the container,
/// `%ProgramData%\sdkwork\webserver\acme-webroot` on Windows, or the shared PVC
/// path in Kubernetes), and this listener serves them back to the CA. Confining
/// the webroot to the configuration directory — as this code did until the
/// absolute path was handled — makes the documented Linux and Kubernetes
/// layouts impossible to configure, so the listener can never serve a
/// challenge and issuance can never succeed.
fn resolve_acme_webroot(
    base_directory: &Path,
    configured: &str,
    diagnostic_path: &str,
    tolerate_missing: bool,
) -> Result<PathBuf, WebServerConfigError> {
    let configured = Path::new(configured);
    if configured.is_absolute() {
        if tolerate_missing && !configured.exists() {
            return Ok(configured.to_path_buf());
        }
        return canonical_directory(configured, diagnostic_path);
    }
    let resolved = canonical_directory(&base_directory.join(configured), diagnostic_path)?;
    if !resolved.starts_with(base_directory) {
        return Err(validation_error(
            diagnostic_path,
            "ACME webroot escapes the configuration directory",
        ));
    }
    Ok(resolved)
}

fn resolve_required_file(
    base_directory: &Path,
    configured: &str,
    diagnostic_path: &str,
) -> Result<PathBuf, WebServerConfigError> {
    // `certs://<domain>/…` references the canonical certificate inventory
    // (`/etc/sdkwork/certs/<domain>/`); the domain directory is the
    // operator/ACME-managed certificate home.
    if let Some(rest) = configured.strip_prefix(crate::config_paths::CERTS_URI_SCHEME) {
        let (domain, file) = rest.split_once('/').ok_or_else(|| {
            validation_error(
                diagnostic_path,
                format!("`{configured}` must be certs://<domain>/<file>"),
            )
        })?;
        let directory = crate::config_paths::canonical_certificate_domain_directory(domain)
            .map_err(|message| validation_error(diagnostic_path, message))?;
        let candidate = directory.join(file);
        let canonical = fs::canonicalize(&candidate).map_err(|error| {
            validation_error(
                diagnostic_path,
                format!(
                    "certificate file {} is unavailable: {error}",
                    candidate.display()
                ),
            )
        })?;
        if !canonical.is_file() {
            return Err(validation_error(
                diagnostic_path,
                format!("{} is not a regular file", canonical.display()),
            ));
        }
        return Ok(canonical);
    }
    let configured = Path::new(configured);
    let candidate = if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        base_directory.join(configured)
    };
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        validation_error(
            diagnostic_path,
            format!("file {} is unavailable: {error}", candidate.display()),
        )
    })?;
    if !canonical.is_file() {
        return Err(validation_error(
            diagnostic_path,
            format!("{} is not a regular file", canonical.display()),
        ));
    }
    Ok(canonical)
}

fn resolve_protected_relative_file(
    base_directory: &Path,
    configured: &str,
    diagnostic_path: &str,
) -> Result<PathBuf, WebServerConfigError> {
    let resolved = resolve_required_file(base_directory, configured, diagnostic_path)?;
    if !resolved.starts_with(base_directory) {
        return Err(validation_error(
            diagnostic_path,
            "protected TLS file escapes the configuration directory",
        ));
    }
    Ok(resolved)
}

fn validation_error(path: impl Into<String>, message: impl Into<String>) -> WebServerConfigError {
    WebServerConfigError::Validation {
        diagnostics: vec![ConfigDiagnostic::new(path, message)],
    }
}

pub fn normalize_authority_host(authority: &str) -> Option<String> {
    let authority = authority.trim();
    if authority.is_empty() {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        return Some(rest[..end].to_ascii_lowercase());
    }
    let host = match authority.rsplit_once(':') {
        Some((host, port))
            if !host.contains(':') && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            host
        }
        _ => authority,
    };
    normalize_server_name(host)
}

fn route_matches_method(route: &RouteConfig, method: &str) -> bool {
    route
        .route_match
        .methods
        .as_ref()
        .is_none_or(|methods| methods.iter().any(|configured| configured == method))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_base_directory_means_the_current_directory() {
        // `Path::new("config.json").parent()` is `Some("")`, not `None`, so the
        // empty case reaches `compile` from the most ordinary invocation there
        // is. Canonicalizing an empty path fails, so it has to be read as `.`.
        assert_eq!(base_directory_or_current(Path::new("")), Path::new("."));
        assert_eq!(
            base_directory_or_current(Path::new("etc/webserver")),
            Path::new("etc/webserver")
        );
    }
}
