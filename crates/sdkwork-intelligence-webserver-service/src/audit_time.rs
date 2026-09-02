//! Audit-log filter timestamp normalization.
//!
//! Adaptive Web date filters emit `YYYY-MM-DD`; the OpenAPI contract also
//! accepts RFC 3339 instants. Both shapes normalize to UTC RFC 3339 before
//! repository comparison so Query extraction stays string-safe and SQL never
//! sees ambiguous calendar dates.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditInstantBound {
    StartInclusive,
    EndExclusive,
}

pub fn normalize_audit_instant(value: &str, bound: AuditInstantBound) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err("must contain 1..64 characters".to_string());
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Ok(parsed.with_timezone(&chrono::Utc).to_rfc3339());
    }
    let date = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map_err(|_| "must be an RFC 3339 timestamp or YYYY-MM-DD date".to_string())?;
    let utc_date = match bound {
        AuditInstantBound::StartInclusive => date,
        AuditInstantBound::EndExclusive => date
            .succ_opt()
            .ok_or_else(|| "date is out of range".to_string())?,
    };
    Ok(utc_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "date is out of range".to_string())?
        .and_utc()
        .to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::{normalize_audit_instant, AuditInstantBound};

    #[test]
    fn accepts_rfc3339_and_date_only() {
        assert_eq!(
            normalize_audit_instant("2026-07-01T12:00:00Z", AuditInstantBound::StartInclusive)
                .expect("rfc3339"),
            "2026-07-01T12:00:00+00:00"
        );
        assert_eq!(
            normalize_audit_instant("2026-07-01", AuditInstantBound::StartInclusive)
                .expect("start"),
            "2026-07-01T00:00:00+00:00"
        );
        assert_eq!(
            normalize_audit_instant("2026-07-01", AuditInstantBound::EndExclusive).expect("end"),
            "2026-07-02T00:00:00+00:00"
        );
    }
}
