package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;
import java.util.Map;

public class NginxValidateResponse {
    private Boolean valid;
    private List<Map<String, Object>> errors;

    public Boolean getValid() {
        return this.valid;
    }

    public void setValid(Boolean valid) {
        this.valid = valid;
    }

    public List<Map<String, Object>> getErrors() {
        return this.errors;
    }

    public void setErrors(List<Map<String, Object>> errors) {
        this.errors = errors;
    }
}
