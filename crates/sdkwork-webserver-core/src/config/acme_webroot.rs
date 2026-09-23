//! Cross-plane agreement on the ACME HTTP-01 webroot.
//!
//! HTTP-01 is a rendezvous between two processes that never talk to each other:
//!
//! - the **certificate worker** writes `/.well-known/acme-challenge/<token>`
//!   files into the directory named by `SDKWORK_WEBSERVER_ACME_WEBROOT`
//!   (see `sdkwork-webserver-acme-service`), and
//! - the **data plane** serves those files to the CA from the directory a
//!   listener declares as `acmeHttp01.webroot`.
//!
//! The two values live in different files, are written by different tools, and
//! were until now never compared. When they disagree the deployment looks
//! healthy — both processes start, both are configured — and every order fails
//! at the CA with an authorization error that says nothing about the cause,
//! because the CA only ever sees a 404. The check below turns that into a
//! startup diagnostic that names both surfaces.
//!
//! `ADR-20260623-acme-certificate-authority.md` states the invariant:
//! "HTTP-01 要求自建数据面监听器配置 `acmeHttp01.webroot`，且 worker 的
//! `SDKWORK_WEBSERVER_ACME_WEBROOT` 指向同一目录".

use std::path::{Path, PathBuf};

use super::compiled::CompiledWebServerApp;

/// What the data plane serves and what the control plane says it writes.
///
/// Collected here as plain data so the decision is a pure function of two
/// values, testable without a process environment or a running worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcmeWebrootReport {
    /// `(listener_id, webroot)` for every listener serving the challenge
    /// namespace, in configuration order.
    pub served: Vec<(String, PathBuf)>,
    /// The certificate worker's write target, when this process knows it.
    pub written: Option<PathBuf>,
}

/// The four possible states of the rendezvous.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcmeWebrootVerdict {
    /// No listener serves the challenge namespace. HTTP-01 is not configured
    /// here; DNS-01 deployments legitimately look like this.
    NotServed,
    /// At least one listener serves exactly the directory the worker writes.
    Agreed,
    /// The data plane serves a directory the worker does not write to. HTTP-01
    /// cannot succeed: every challenge file lands where nothing serves it.
    Disagreed,
    /// The data plane serves a challenge directory, but this process does not
    /// know the worker's write target. Not an error — in Kubernetes the write
    /// target is configured on the worker alone, and both share one mounted
    /// volume — but it is the state an operator should be told about, because
    /// nothing here can confirm the rendezvous.
    WriteTargetUnknown,
}

/// A non-fatal observation, kept as a distinct kind so the caller can log the
/// unverifiable state quietly and the broken state loudly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcmeWebrootNotice {
    /// The data plane serves a challenge directory, but this process does not
    /// know the worker's write target.
    WriteTargetUnknown(String),
    /// Both sides are known and they differ; fatal in production-like
    /// environments only.
    Disagreement(String),
}

impl AcmeWebrootNotice {
    pub fn message(&self) -> &str {
        match self {
            Self::WriteTargetUnknown(message) | Self::Disagreement(message) => message,
        }
    }
}

impl AcmeWebrootReport {
    /// Collect the data plane's challenge webroots plus the known write target.
    pub fn collect(app: &CompiledWebServerApp, written: Option<PathBuf>) -> Self {
        let served = app
            .config()
            .listeners
            .iter()
            .filter(|listener| listener.acme_http_01.is_some())
            .filter_map(|listener| {
                app.acme_webroot(&listener.id)
                    .map(|webroot| (listener.id.clone(), webroot.to_path_buf()))
            })
            .collect();
        Self { served, written }
    }

    pub fn verdict(&self) -> AcmeWebrootVerdict {
        if self.served.is_empty() {
            return AcmeWebrootVerdict::NotServed;
        }
        let Some(written) = self.written.as_deref() else {
            return AcmeWebrootVerdict::WriteTargetUnknown;
        };
        let written = comparable(written);
        if self
            .served
            .iter()
            .any(|(_, webroot)| comparable(webroot) == written)
        {
            AcmeWebrootVerdict::Agreed
        } else {
            AcmeWebrootVerdict::Disagreed
        }
    }

    /// Turn the verdict into a deploy-time decision.
    ///
    /// A disagreement is fatal in production-like environments and a warning
    /// elsewhere: in a production-like environment the deployment would ship
    /// with certificate issuance deterministically broken, and the operator
    /// would find out from the CA's error text rather than from the process
    /// that knows both halves. A development environment is allowed to run so
    /// the SPA and the management plane stay reachable while the operator
    /// finishes the wiring.
    pub fn enforce(&self, production_like: bool) -> Result<Option<AcmeWebrootNotice>, String> {
        match self.verdict() {
            AcmeWebrootVerdict::NotServed | AcmeWebrootVerdict::Agreed => Ok(None),
            AcmeWebrootVerdict::WriteTargetUnknown => Ok(Some(
                AcmeWebrootNotice::WriteTargetUnknown(self.describe_write_target_unknown()),
            )),
            AcmeWebrootVerdict::Disagreed => {
                let message = self.describe_disagreement();
                if production_like {
                    Err(message)
                } else {
                    Ok(Some(AcmeWebrootNotice::Disagreement(message)))
                }
            }
        }
    }

    fn describe_disagreement(&self) -> String {
        let written = self
            .written
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let served = self
            .served
            .iter()
            .map(|(listener_id, webroot)| format!("{listener_id}={}", webroot.display()))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "ACME HTTP-01 webroot mismatch: the certificate worker writes challenge tokens to \
             `{written}` ({ACME_WEBROOT_ENV}), but the data plane serves them from {served} \
             (listeners[].acmeHttp01.webroot). The CA will always receive 404 for \
             /.well-known/acme-challenge/<token> and issuance cannot succeed. Point both at the \
             same directory: set listeners[].acmeHttp01.webroot to `{written}`, or set \
             {ACME_WEBROOT_ENV} to the served directory. If this listener declares acmeHttp01 \
             only to opt in to public plaintext HTTP, declare allowPlaintextHttp instead.",
            ACME_WEBROOT_ENV = crate::runtime_env::ACME_WEBROOT_ENV,
        )
    }

    fn describe_write_target_unknown(&self) -> String {
        let served = self
            .served
            .iter()
            .map(|(listener_id, webroot)| format!("{listener_id}={}", webroot.display()))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "ACME HTTP-01 challenge serving is configured for {served}, but \
             {ACME_WEBROOT_ENV} is not set in this process, so the certificate worker's write \
             target could not be verified. Set it — or, when the worker runs elsewhere (for \
             example a Kubernetes StatefulSet sharing one volume), make sure the worker's value \
             names this same directory.",
            ACME_WEBROOT_ENV = crate::runtime_env::ACME_WEBROOT_ENV,
        )
    }
}

/// Compare two webroots as the filesystem would.
///
/// Two platform behaviours make a plain string comparison wrong in the ways
/// that matter on a deployed server:
///
/// - Windows extends absolute paths to the verbatim form `\\?\C:\...` when they
///   are canonicalized, so the data plane's resolved path never equals the
///   operator's `C:\...` value even when they name one directory.
/// - Windows paths are case-insensitive, while Linux paths are not. Lowercasing
///   only on Windows keeps a legitimate `Webroot`/`webroot` pair distinct where
///   the filesystem keeps them distinct.
///
/// Symlinks are resolved when the directory exists, because that is what the
/// data plane itself did; when it does not exist yet, a lexical clean still
/// compares `/var/lib/../lib/x` with `/var/lib/x`.
fn comparable(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| lexical_clean(path));
    normalize_text(&resolved.to_string_lossy())
}

fn normalize_text(text: &str) -> String {
    // Backslashes to slashes keeps the two platforms comparable in the messages
    // and in the equality test; the verbatim prefix Windows adds during
    // canonicalization carries no meaning for this comparison.
    let text = text.replace('\\', "/");
    // `\\?\UNC\server\share` is the extended-length spelling of
    // `\\server\share`, not of a directory named `UNC`; mapping it back keeps a
    // network share comparable with the form an operator writes.
    let text = match text
        .strip_prefix("//?/UNC/")
        .or_else(|| text.strip_prefix("//?/unc/"))
    {
        Some(rest) => format!("//{rest}"),
        None => text
            .strip_prefix("//?/")
            .map(str::to_owned)
            .unwrap_or(text),
    };
    let text = if text.len() > 1 {
        text.trim_end_matches('/').to_owned()
    } else {
        text
    };
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

/// Best-effort lexical clean for a path that does not exist.
fn lexical_clean(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !cleaned.pop() {
                    // Nothing left to pop: `/../etc` is `/etc`, and a `..` that
                    // cannot climb higher is dropped rather than kept. A
                    // relative prefix has no root to protect, so its `..` stays.
                    //
                    // `has_root` rather than `is_absolute`: on Windows `/etc`
                    // has a root but is not absolute (it needs a drive prefix),
                    // and the root is exactly what must not be climbed past.
                    if !cleaned.has_root() {
                        cleaned.push("..");
                    }
                }
            }
            other => cleaned.push(other.as_os_str()),
        }
    }
    if cleaned.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(served: &[(&str, &str)], written: Option<&str>) -> AcmeWebrootReport {
        AcmeWebrootReport {
            served: served
                .iter()
                .map(|(id, path)| ((*id).to_owned(), PathBuf::from(path)))
                .collect(),
            written: written.map(PathBuf::from),
        }
    }

    #[test]
    fn a_deployment_without_a_challenge_listener_has_nothing_to_agree_on() {
        assert_eq!(report(&[], None).verdict(), AcmeWebrootVerdict::NotServed);
        assert_eq!(
            report(&[], Some("/var/lib/acme")).verdict(),
            AcmeWebrootVerdict::NotServed
        );
        assert!(report(&[], None).enforce(true).is_ok());
    }

    #[test]
    fn one_matching_listener_is_enough_to_agree() {
        assert_eq!(
            report(&[("http", "/var/lib/acme")], Some("/var/lib/acme")).verdict(),
            AcmeWebrootVerdict::Agreed
        );
        // A second listener pointing somewhere else does not undo the match:
        // the challenge the CA fetches only has to land in one served place.
        assert_eq!(
            report(
                &[("http", "/var/lib/acme"), ("alt", "/srv/other")],
                Some("/var/lib/acme")
            )
            .verdict(),
            AcmeWebrootVerdict::Agreed
        );
        assert!(report(&[("http", "/var/lib/acme")], Some("/var/lib/acme"))
            .enforce(true)
            .expect("agreement is not a failure")
            .is_none());
    }

    #[test]
    fn a_disagreement_is_fatal_only_in_production_like_environments() {
        let report = report(&[("http", "/srv/served")], Some("/var/lib/written"));
        assert_eq!(report.verdict(), AcmeWebrootVerdict::Disagreed);

        let error = report
            .enforce(true)
            .expect_err("production must fail closed on a broken rendezvous");
        // The message has to name both sides and the variable, or the operator
        // cannot act on it.
        assert!(error.contains("/var/lib/written"), "{error}");
        assert!(error.contains("http=/srv/served"), "{error}");
        assert!(error.contains(ACME_WEBROOT_ENV_NAME), "{error}");

        let notice = report
            .enforce(false)
            .expect("development keeps running")
            .expect("the operator still needs the warning");
        assert!(
            matches!(notice, AcmeWebrootNotice::Disagreement(_)),
            "{notice:?}"
        );
        assert!(notice.message().contains("/var/lib/written"), "{notice:?}");
    }

    #[test]
    fn an_unknown_write_target_warns_but_never_blocks_startup() {
        let report = report(&[("http", "/var/lib/acme")], None);
        assert_eq!(report.verdict(), AcmeWebrootVerdict::WriteTargetUnknown);
        for production_like in [true, false] {
            let notice = report
                .enforce(production_like)
                .expect("Kubernetes configures the write target on the worker alone")
                .expect("the operator should still be told");
            assert!(
                matches!(notice, AcmeWebrootNotice::WriteTargetUnknown(_)),
                "{notice:?}"
            );
            assert!(notice.message().contains("http=/var/lib/acme"), "{notice:?}");
            assert!(notice.message().contains(ACME_WEBROOT_ENV_NAME), "{notice:?}");
        }
    }

    #[test]
    fn equivalent_spellings_of_one_directory_agree() {
        // Trailing separator, `.` segments and a non-canonical `..` are common
        // in operator-supplied values and must not read as a mismatch.
        assert_eq!(
            report(&[("http", "/var/lib/acme")], Some("/var/lib/acme/")).verdict(),
            AcmeWebrootVerdict::Agreed
        );
        assert_eq!(
            report(&[("http", "/var/lib/acme")], Some("/var/lib/other/../acme")).verdict(),
            AcmeWebrootVerdict::Agreed
        );
        assert_eq!(
            report(&[("http", "/var/lib/acme")], Some("/var/lib/./acme")).verdict(),
            AcmeWebrootVerdict::Agreed
        );
    }

    #[test]
    fn windows_verbatim_prefixes_do_not_read_as_a_mismatch() {
        // `fs::canonicalize` answers `\\?\C:\...` on Windows while an operator
        // writes `C:\...`; without this normalization every Windows deployment
        // would report a false mismatch.
        assert_eq!(
            normalize_text(r"\\?\C:\ProgramData\sdkwork\webserver\acme-webroot"),
            normalize_text(r"C:\ProgramData\sdkwork\webserver\acme-webroot")
        );
        assert_eq!(
            normalize_text(r"\\?\UNC\server\share\acme"),
            normalize_text(r"\\server\share\acme")
        );
    }

    #[test]
    fn path_case_follows_the_platform_filesystem() {
        let upper = normalize_text("/var/lib/ACME");
        let lower = normalize_text("/var/lib/acme");
        if cfg!(windows) {
            assert_eq!(upper, lower);
        } else {
            assert_ne!(upper, lower, "Linux paths are case-sensitive");
        }
    }

    #[test]
    fn lexical_clean_never_climbs_past_the_root() {
        assert_eq!(lexical_clean(Path::new("/../etc")), PathBuf::from("/etc"));
        assert_eq!(
            lexical_clean(Path::new("/var/lib/./acme/../acme")),
            PathBuf::from("/var/lib/acme")
        );
        // A relative path has no root to protect, so its `..` is preserved.
        assert_eq!(lexical_clean(Path::new("../x")), PathBuf::from("../x"));
        assert_eq!(lexical_clean(Path::new("a/../../b")), PathBuf::from("../b"));
    }

    const ACME_WEBROOT_ENV_NAME: &str = crate::runtime_env::ACME_WEBROOT_ENV;
}
