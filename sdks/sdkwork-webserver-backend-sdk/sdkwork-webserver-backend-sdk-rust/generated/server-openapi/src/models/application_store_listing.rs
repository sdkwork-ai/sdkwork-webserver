use serde::{Deserialize, Serialize};

use crate::models::{MediaResource};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApplicationStoreListing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<MediaResource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaResource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previews: Option<Vec<MediaResource>>,

    #[serde(rename = "shortDescription")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,

    #[serde(rename = "fullDescription")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_description: Option<String>,

    #[serde(rename = "releaseNotes")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,

    #[serde(rename = "supportUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_url: Option<String>,

    #[serde(rename = "privacyPolicyUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_policy_url: Option<String>,

    #[serde(rename = "officialWebsiteUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_website_url: Option<String>,
}
