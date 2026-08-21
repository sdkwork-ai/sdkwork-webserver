#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalMetricDimensions {
    pub environment: String,
    pub deployment_profile: String,
    pub runtime_target: String,
}

impl CanonicalMetricDimensions {
    pub(crate) fn from_env() -> Result<Self, String> {
        Self::new(
            std::env::var("SDKWORK_WEBSERVER_ENVIRONMENT")
                .ok()
                .as_deref(),
            std::env::var("SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE")
                .ok()
                .as_deref(),
            std::env::var("SDKWORK_WEBSERVER_RUNTIME_TARGET")
                .ok()
                .as_deref(),
        )
    }

    pub(crate) fn new(
        environment: Option<&str>,
        deployment_profile: Option<&str>,
        runtime_target: Option<&str>,
    ) -> Result<Self, String> {
        Ok(Self {
            environment: validated_metric_dimension(
                "SDKWORK_WEBSERVER_ENVIRONMENT",
                environment,
                "development",
                &["development", "test", "staging", "production"],
            )?,
            deployment_profile: validated_metric_dimension(
                "SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE",
                deployment_profile,
                "standalone",
                &["standalone", "cloud"],
            )?,
            runtime_target: validated_metric_dimension(
                "SDKWORK_WEBSERVER_RUNTIME_TARGET",
                runtime_target,
                "server",
                &["server", "container"],
            )?,
        })
    }
}

impl Default for CanonicalMetricDimensions {
    fn default() -> Self {
        Self {
            environment: "development".to_owned(),
            deployment_profile: "standalone".to_owned(),
            runtime_target: "server".to_owned(),
        }
    }
}

fn validated_metric_dimension(
    key: &str,
    value: Option<&str>,
    default: &str,
    allowed: &[&str],
) -> Result<String, String> {
    let value = value.unwrap_or(default).trim().to_ascii_lowercase();
    if allowed.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(format!(
            "{key} has unsupported metrics label value; allowed values: {}",
            allowed.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::CanonicalMetricDimensions;

    #[test]
    fn dimensions_are_normalized_and_fail_closed() {
        assert_eq!(
            CanonicalMetricDimensions::new(
                Some(" Production "),
                Some(" CLOUD "),
                Some("container"),
            ),
            Ok(CanonicalMetricDimensions {
                environment: "production".to_owned(),
                deployment_profile: "cloud".to_owned(),
                runtime_target: "container".to_owned(),
            })
        );
        assert!(CanonicalMetricDimensions::new(None, None, Some("docker")).is_err());
    }
}
