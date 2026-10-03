//! The root-domain cloud-account reference: its wire shape and its three-state
//! filter.
//!
//! These are the two places the Domains page's cloud-account facet depends on and
//! that no database test can reach: the *reading* of the query member, and the
//! *meaning* of an absent member versus an explicit `null` in the partial edit.
//! Both are pure, and both fail silently when they are wrong — a filter that reads
//! `ALL` as an account id answers an empty list, and an edit that cannot tell
//! "leave it alone" from "unbind" quietly keeps a pin the operator cleared.

use sdkwork_webserver_contract::{
    cloud_account_id_shape_error, CreateRootDomainRequest, ListRootDomainsQuery,
    RootDomainCloudAccountFilter, UpdateRootDomainRequest, CLOUD_ACCOUNT_ID_MAX_CHARS,
    ROOT_DOMAIN_CLOUD_ACCOUNT_UNASSIGNED,
};

fn filter(query: &str) -> RootDomainCloudAccountFilter {
    let parsed: ListRootDomainsQuery = serde_json::from_str(query).expect("query member parses");
    parsed.cloud_account_filter()
}

#[test]
fn the_filter_reads_absent_blank_reserved_and_named_members_apart() {
    // An absent member filters nothing.
    assert_eq!(filter("{}"), RootDomainCloudAccountFilter::Any);
    // A blank member is the same omission, because that is what a cleared form
    // field submits; reading it as an account named "" would answer an empty list.
    assert_eq!(
        filter(r#"{"cloud_account_id":""}"#),
        RootDomainCloudAccountFilter::Any
    );
    assert_eq!(
        filter(r#"{"cloud_account_id":"   "}"#),
        RootDomainCloudAccountFilter::Any
    );
    // The reserved literal is the "pinned to nothing" arm.
    assert_eq!(
        filter(r#"{"cloud_account_id":"UNASSIGNED"}"#),
        RootDomainCloudAccountFilter::Unassigned
    );
    // Anything else names an account, and the name is kept verbatim: it is a
    // reference into another module's table, so this surface does not re-case it.
    assert_eq!(
        filter(r#"{"cloud_account_id":"Aliyun-Prod"}"#),
        RootDomainCloudAccountFilter::Assigned("Aliyun-Prod".to_owned())
    );
}

#[test]
fn the_reserved_literal_is_read_before_any_account_id() {
    // The reserved word passes the id shape rule, so the shape rule is *not* what
    // keeps the two apart — the reserved reading simply comes first, and that
    // ordering is the whole safety argument. It is pinned here because reversing
    // the two match arms would leave every test that only names ordinary accounts
    // passing while "unassigned" silently became an account lookup.
    assert!(cloud_account_id_shape_error(ROOT_DOMAIN_CLOUD_ACCOUNT_UNASSIGNED).is_none());
    assert_eq!(
        filter(r#"{"cloud_account_id":"UNASSIGNED"}"#),
        RootDomainCloudAccountFilter::Unassigned,
    );

    // A different spelling is an account id, not the reserved arm: account ids are
    // case-sensitive, so folding case here would make this word mean something the
    // caller's own account id `unassigned` does not.
    assert_eq!(
        filter(r#"{"cloud_account_id":"unassigned"}"#),
        RootDomainCloudAccountFilter::Assigned("unassigned".to_owned()),
    );
    assert_eq!(
        filter(r#"{"cloud_account_id":"UNASSIGNED "}"#),
        RootDomainCloudAccountFilter::Unassigned,
        "surrounding whitespace is trimmed before the reserved word is read",
    );
}

#[test]
fn the_account_id_shape_rule_bounds_and_filters_the_value() {
    assert!(cloud_account_id_shape_error("aliyun-prod").is_none());
    assert!(cloud_account_id_shape_error("a").is_none());
    assert!(cloud_account_id_shape_error(&"a".repeat(CLOUD_ACCOUNT_ID_MAX_CHARS)).is_none());

    // Empty, over-long, whitespace-bearing, and control-bearing values are all
    // refused, and a value that does not start alphanumeric is refused because the
    // column's own CHECK declares that shape.
    for rejected in [
        "",
        " ",
        "aliyun prod",
        "aliyun\tprod",
        "-aliyun",
        "_aliyun",
        "aliyun\nprod",
    ] {
        assert!(
            cloud_account_id_shape_error(rejected).is_some(),
            "{rejected:?} must be refused"
        );
    }
    assert!(cloud_account_id_shape_error(&"a".repeat(CLOUD_ACCOUNT_ID_MAX_CHARS + 1)).is_some());
}

#[test]
fn the_partial_edit_tells_an_absent_binding_from_an_explicit_unbind() {
    // Absent: leave the stored binding alone. This is the reading every other
    // member of the request has, and it is the one that must not be lost.
    let untouched: UpdateRootDomainRequest = serde_json::from_str(r#"{"displayName":"SDKWork"}"#)
        .expect("descriptive edit parses");
    assert_eq!(untouched.cloud_account_id, None);

    // Explicit null: unbind. The outer `Some` is "the caller named this member";
    // the inner `None` is the binding to store, which is no binding.
    let unbind: UpdateRootDomainRequest =
        serde_json::from_str(r#"{"cloudAccountId":null}"#).expect("unbind parses");
    assert_eq!(unbind.cloud_account_id, Some(None));

    // A named account: bind it.
    let bind: UpdateRootDomainRequest =
        serde_json::from_str(r#"{"cloudAccountId":"aliyun-prod"}"#).expect("bind parses");
    assert_eq!(bind.cloud_account_id, Some(Some("aliyun-prod".to_owned())));
}

#[test]
fn a_created_root_domain_may_name_an_account_or_none_at_all() {
    let bare: CreateRootDomainRequest =
        serde_json::from_str(r#"{"hostname":"example.com"}"#).expect("bare create parses");
    assert_eq!(bare.cloud_account_id, None);

    let named: CreateRootDomainRequest =
        serde_json::from_str(r#"{"hostname":"example.com","cloudAccountId":"aliyun-prod"}"#)
            .expect("named create parses");
    assert_eq!(named.cloud_account_id.as_deref(), Some("aliyun-prod"));
}
