//! Adaptive Web surface selection (SDKWORK_DEPLOY_SPEC.md §8, adaptive-web.maps.conf).
//!
//! Mobile prefers H5; desktop/tablet prefers PC. `Sec-CH-UA-Mobile=?1` forces
//! H5. iPad user-agents often contain "Mobile" but default to PC.

pub fn prefer_h5_surface(user_agent: Option<&str>, sec_ch_ua_mobile: Option<&str>) -> bool {
    if sec_ch_ua_mobile
        .map(str::trim)
        .is_some_and(|value| value == "?1")
    {
        return true;
    }
    let Some(user_agent) = user_agent else {
        return false;
    };
    if user_agent.to_ascii_lowercase().contains("ipad") {
        return false;
    }
    const MOBILE_MARKERS: &[&str] = &[
        "Mobile",
        "Android",
        "iPhone",
        "iPod",
        "webOS",
        "BlackBerry",
        "IEMobile",
        "Opera Mini",
        "MicroMessenger",
        "HuaweiBrowser",
        "HarmonyOS",
        "UCBrowser",
        "Quark",
    ];
    MOBILE_MARKERS
        .iter()
        .any(|marker| user_agent.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::prefer_h5_surface;

    #[test]
    fn desktop_defaults_to_pc() {
        assert!(!prefer_h5_surface(
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0"),
            None
        ));
    }

    #[test]
    fn iphone_selects_h5() {
        assert!(prefer_h5_surface(
            Some("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Mobile/15E148"),
            None
        ));
    }

    #[test]
    fn ipad_selects_pc_even_when_ua_contains_mobile() {
        assert!(!prefer_h5_surface(
            Some("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) Mobile/15E148"),
            None
        ));
    }

    #[test]
    fn client_hint_forces_h5() {
        assert!(prefer_h5_surface(
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
            Some("?1")
        ));
    }
}
