package api

import (
    "encoding/json"

    sdkhttp "github.com/sdkwork/sdkwork-webserver-backend-sdk/http"
)

type BaseApi struct {
    http     *sdkhttp.Client
    basePath string
}

func NewBaseApi(http *sdkhttp.Client, basePath string) *BaseApi {
    return &BaseApi{http: http, basePath: basePath}
}

func decodeResult[T any](raw interface{}) (T, error) {
    var zero T
    if raw == nil {
        return zero, nil
    }
    payload, err := json.Marshal(raw)
    if err != nil {
        return zero, err
    }
    var parsed T
    if err := json.Unmarshal(payload, &parsed); err != nil {
        return zero, err
    }
    return parsed, nil
}

func (b *BaseApi) Get(path string, query map[string]interface{}, headers map[string]string) (interface{}, error) {
    return b.http.Get(b.basePath+path, query, headers)
}

func (b *BaseApi) Post(
    path string,
    body interface{},
    query map[string]interface{},
    headers map[string]string,
    contentType string,
) (interface{}, error) {
    return b.http.Post(b.basePath+path, body, query, headers, contentType)
}

func (b *BaseApi) Put(
    path string,
    body interface{},
    query map[string]interface{},
    headers map[string]string,
    contentType string,
) (interface{}, error) {
    return b.http.Put(b.basePath+path, body, query, headers, contentType)
}

func (b *BaseApi) Delete(path string, query map[string]interface{}, headers map[string]string) (interface{}, error) {
    return b.http.Delete(b.basePath+path, query, headers)
}

func (b *BaseApi) Patch(
    path string,
    body interface{},
    query map[string]interface{},
    headers map[string]string,
    contentType string,
) (interface{}, error) {
    return b.http.Patch(b.basePath+path, body, query, headers, contentType)
}

func (b *BaseApi) Request(
    method string,
    path string,
    body interface{},
    query map[string]interface{},
    headers map[string]string,
    contentType string,
) (interface{}, error) {
    return b.http.Request(method, b.basePath+path, body, query, headers, contentType, false, false)
}
