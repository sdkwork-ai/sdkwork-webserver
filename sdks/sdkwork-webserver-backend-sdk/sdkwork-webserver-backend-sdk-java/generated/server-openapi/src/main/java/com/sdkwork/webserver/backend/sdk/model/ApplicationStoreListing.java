package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class ApplicationStoreListing {
    private MediaResource icon;
    private MediaResource cover;
    private List<MediaResource> previews;
    private String shortDescription;
    private String fullDescription;
    private String releaseNotes;
    private String category;
    private List<String> keywords;
    private String supportUrl;
    private String privacyPolicyUrl;
    private String officialWebsiteUrl;

    public MediaResource getIcon() {
        return this.icon;
    }

    public void setIcon(MediaResource icon) {
        this.icon = icon;
    }

    public MediaResource getCover() {
        return this.cover;
    }

    public void setCover(MediaResource cover) {
        this.cover = cover;
    }

    public List<MediaResource> getPreviews() {
        return this.previews;
    }

    public void setPreviews(List<MediaResource> previews) {
        this.previews = previews;
    }

    public String getShortDescription() {
        return this.shortDescription;
    }

    public void setShortDescription(String shortDescription) {
        this.shortDescription = shortDescription;
    }

    public String getFullDescription() {
        return this.fullDescription;
    }

    public void setFullDescription(String fullDescription) {
        this.fullDescription = fullDescription;
    }

    public String getReleaseNotes() {
        return this.releaseNotes;
    }

    public void setReleaseNotes(String releaseNotes) {
        this.releaseNotes = releaseNotes;
    }

    public String getCategory() {
        return this.category;
    }

    public void setCategory(String category) {
        this.category = category;
    }

    public List<String> getKeywords() {
        return this.keywords;
    }

    public void setKeywords(List<String> keywords) {
        this.keywords = keywords;
    }

    public String getSupportUrl() {
        return this.supportUrl;
    }

    public void setSupportUrl(String supportUrl) {
        this.supportUrl = supportUrl;
    }

    public String getPrivacyPolicyUrl() {
        return this.privacyPolicyUrl;
    }

    public void setPrivacyPolicyUrl(String privacyPolicyUrl) {
        this.privacyPolicyUrl = privacyPolicyUrl;
    }

    public String getOfficialWebsiteUrl() {
        return this.officialWebsiteUrl;
    }

    public void setOfficialWebsiteUrl(String officialWebsiteUrl) {
        this.officialWebsiteUrl = officialWebsiteUrl;
    }
}
