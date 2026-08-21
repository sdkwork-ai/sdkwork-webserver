//! Process-owned Adaptive Web surface selection for the standalone console.
//! Authority: SDKWORK_DEPLOY_SPEC.md §8 (sdkwork-webserver expose.mode: api exception).

use axum::http::{
    header::{HeaderMap, HeaderName, USER_AGENT},
    HeaderValue,
};

pub(crate) const SEC_CH_UA_MOBILE: HeaderName = HeaderName::from_static("sec-ch-ua-mobile");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdaptiveClientClass {
    Mobile,
    Desktop,
    Tablet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdaptiveSurface {
    Pc,
    H5,
    Static,
}

/// Prefer H5 on mobile, PC on desktop/tablet (unless tablet prefers H5).
/// Missing preferred surfaces collapse to the other SPA, then static-fallback.
pub(crate) fn select_adaptive_surface(
    client: AdaptiveClientClass,
    tablet_prefers_h5: bool,
    pc_ready: bool,
    h5_ready: bool,
    static_ready: bool,
) -> Option<AdaptiveSurface> {
    let prefer_h5 = match client {
        AdaptiveClientClass::Mobile => true,
        AdaptiveClientClass::Tablet => tablet_prefers_h5,
        AdaptiveClientClass::Desktop => false,
    };
    if prefer_h5 {
        if h5_ready {
            return Some(AdaptiveSurface::H5);
        }
        if pc_ready {
            return Some(AdaptiveSurface::Pc);
        }
    } else {
        if pc_ready {
            return Some(AdaptiveSurface::Pc);
        }
        if h5_ready {
            return Some(AdaptiveSurface::H5);
        }
    }
    if static_ready {
        Some(AdaptiveSurface::Static)
    } else {
        None
    }
}

pub(crate) fn classify_adaptive_client(headers: &HeaderMap) -> AdaptiveClientClass {
    if let Some(mobile) = header_str(headers, &SEC_CH_UA_MOBILE, 8) {
        match mobile.trim() {
            "?1" => return AdaptiveClientClass::Mobile,
            "?0" => {
                // Continue to UA so iPad with ?0 can still classify as Tablet.
            }
            _ => {}
        }
    }
    let Some(user_agent) = header_str(headers, &USER_AGENT, 512) else {
        return AdaptiveClientClass::Desktop;
    };
    let lower = user_agent.to_ascii_lowercase();
    // iPad before mobile markers: many iPad UAs also contain "Mobile".
    if lower.contains("ipad") || lower.contains("tablet") {
        return AdaptiveClientClass::Tablet;
    }
    if matches_adaptive_mobile_user_agent(&lower) {
        return AdaptiveClientClass::Mobile;
    }
    AdaptiveClientClass::Desktop
}

fn matches_adaptive_mobile_user_agent(lower_user_agent: &str) -> bool {
    const MOBILE_MARKERS: [&str; 13] = [
        "mobile",
        "android",
        "iphone",
        "ipod",
        "webos",
        "blackberry",
        "iemobile",
        "opera mini",
        "micromessenger",
        "huaweibrowser",
        "harmonyos",
        "ucbrowser",
        "quark",
    ];
    MOBILE_MARKERS
        .iter()
        .any(|marker| lower_user_agent.contains(marker))
}

fn header_str<'a>(headers: &'a HeaderMap, name: &HeaderName, max_bytes: usize) -> Option<&'a str> {
    let value = headers.get(name)?;
    let text = value.to_str().ok()?;
    if text.len() > max_bytes {
        return None;
    }
    Some(text)
}

pub(crate) fn adaptive_vary_header() -> HeaderValue {
    HeaderValue::from_static("User-Agent, Sec-CH-UA-Mobile")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn mobile_prefers_h5_then_pc_then_static() {
        assert_eq!(
            select_adaptive_surface(AdaptiveClientClass::Mobile, false, true, true, true),
            Some(AdaptiveSurface::H5)
        );
        assert_eq!(
            select_adaptive_surface(AdaptiveClientClass::Mobile, false, true, false, true),
            Some(AdaptiveSurface::Pc)
        );
        assert_eq!(
            select_adaptive_surface(AdaptiveClientClass::Mobile, false, false, false, true),
            Some(AdaptiveSurface::Static)
        );
    }

    #[test]
    fn desktop_prefers_pc_then_h5_then_static() {
        assert_eq!(
            select_adaptive_surface(AdaptiveClientClass::Desktop, false, true, true, true),
            Some(AdaptiveSurface::Pc)
        );
        assert_eq!(
            select_adaptive_surface(AdaptiveClientClass::Desktop, false, false, true, true),
            Some(AdaptiveSurface::H5)
        );
    }

    #[test]
    fn tablet_defaults_to_pc_unless_override() {
        assert_eq!(
            select_adaptive_surface(AdaptiveClientClass::Tablet, false, true, true, false),
            Some(AdaptiveSurface::Pc)
        );
        assert_eq!(
            select_adaptive_surface(AdaptiveClientClass::Tablet, true, true, true, false),
            Some(AdaptiveSurface::H5)
        );
    }

    #[test]
    fn classifies_ipad_with_mobile_substring_as_tablet() {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
        );
        assert_eq!(
            classify_adaptive_client(&headers),
            AdaptiveClientClass::Tablet
        );
    }

    #[test]
    fn client_hint_mobile_forces_mobile_class() {
        let mut headers = HeaderMap::new();
        headers.insert(SEC_CH_UA_MOBILE, HeaderValue::from_static("?1"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );
        assert_eq!(
            classify_adaptive_client(&headers),
            AdaptiveClientClass::Mobile
        );
    }
}
