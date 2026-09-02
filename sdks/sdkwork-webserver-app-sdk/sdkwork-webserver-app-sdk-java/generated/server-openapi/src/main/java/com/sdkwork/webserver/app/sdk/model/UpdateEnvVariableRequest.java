package com.sdkwork.webserver.app.sdk.model;


public class UpdateEnvVariableRequest {
    private String value;
    private Boolean isSecret;

    public String getValue() {
        return this.value;
    }

    public void setValue(String value) {
        this.value = value;
    }

    public Boolean getIsSecret() {
        return this.isSecret;
    }

    public void setIsSecret(Boolean isSecret) {
        this.isSecret = isSecret;
    }
}
