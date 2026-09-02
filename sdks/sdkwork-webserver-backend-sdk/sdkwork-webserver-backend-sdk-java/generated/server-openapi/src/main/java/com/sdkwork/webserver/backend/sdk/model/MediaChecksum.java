package com.sdkwork.webserver.backend.sdk.model;


public class MediaChecksum {
    private String algorithm;
    private String value;

    public String getAlgorithm() {
        return this.algorithm;
    }

    public void setAlgorithm(String algorithm) {
        this.algorithm = algorithm;
    }

    public String getValue() {
        return this.value;
    }

    public void setValue(String value) {
        this.value = value;
    }
}
