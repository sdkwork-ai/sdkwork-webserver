package http

import (
    "bufio"
    "bytes"
    "encoding/json"
    "fmt"
    "io"
    "mime/multipart"
    "net/http"
    "net/url"
    "strings"
    "time"

    common "github.com/sdkwork/sdk-common-go/common"
)


// Config wraps sdk-common Go config and adds SDK auth fields.
type Config struct {
    common.SdkConfig
    AuthToken   string
    AccessToken string
}

func NewDefaultConfig(baseURL string) Config {
    timeout := common.DefaultTimeout
    return Config{
        SdkConfig: common.SdkConfig{
            HttpClientConfig: common.HttpClientConfig{
                BaseURL: baseURL,
                Timeout: &timeout,
                Headers: common.HttpHeaders{},
            },
        },
    }
}

type Client struct {
    baseURL    string
    httpClient *http.Client
    headers    common.HttpHeaders
}

func NewClient(config Config) *Client {
    timeoutMs := common.DefaultTimeout
    if config.Timeout != nil && *config.Timeout > 0 {
        timeoutMs = *config.Timeout
    }

    headers := common.HttpHeaders{}
    for key, value := range config.Headers {
        headers[key] = value
    }

    client := &Client{
        baseURL: strings.TrimRight(config.BaseURL, "/"),
        httpClient: &http.Client{
            Timeout: time.Duration(timeoutMs) * time.Millisecond,
        },
        headers: headers,
    }
    if config.AuthToken != "" {
        client.SetAuthToken(config.AuthToken)
    }
    if config.AccessToken != "" {
        client.SetAccessToken(config.AccessToken)
    }

    return client
}

func (c *Client) SetAuthToken(token string) {
    c.headers["Authorization"] = "Bearer " + token
}

func (c *Client) SetAccessToken(token string) {
    c.headers["Access-Token"] = token
}

func (c *Client) SetHeader(key, value string) {
    c.headers[key] = value
}

func (c *Client) Get(path string, query map[string]interface{}, requestHeaders map[string]string) (interface{}, error) {
    return c.request("GET", path, query, nil, requestHeaders, "", false, false)
}

func (c *Client) Post(
    path string,
    body interface{},
    query map[string]interface{},
    requestHeaders map[string]string,
    contentType string,
) (interface{}, error) {
    return c.request("POST", path, query, body, requestHeaders, contentType, false, false)
}

func (c *Client) Put(
    path string,
    body interface{},
    query map[string]interface{},
    requestHeaders map[string]string,
    contentType string,
) (interface{}, error) {
    return c.request("PUT", path, query, body, requestHeaders, contentType, false, false)
}

func (c *Client) Delete(path string, query map[string]interface{}, requestHeaders map[string]string) (interface{}, error) {
    return c.request("DELETE", path, query, nil, requestHeaders, "", false, false)
}

func (c *Client) Patch(
    path string,
    body interface{},
    query map[string]interface{},
    requestHeaders map[string]string,
    contentType string,
) (interface{}, error) {
    return c.request("PATCH", path, query, body, requestHeaders, contentType, false, false)
}

func (c *Client) Request(
    method string,
    path string,
    body interface{},
    query map[string]interface{},
    requestHeaders map[string]string,
    contentType string,
    skipAuth bool,
    accessTokenOnly bool,
) (interface{}, error) {
    return c.request(method, path, query, body, requestHeaders, contentType, skipAuth, accessTokenOnly)
}

func (c *Client) RequestBytes(
    method string,
    path string,
    body interface{},
    query map[string]interface{},
    requestHeaders map[string]string,
    contentType string,
    skipAuth bool,
    accessTokenOnly bool,
) ([]byte, error) {
    raw, err := c.request(method, path, query, body, requestHeaders, contentType, skipAuth, accessTokenOnly)
    if err != nil {
        return nil, err
    }
    switch value := raw.(type) {
    case nil:
        return []byte{}, nil
    case []byte:
        return value, nil
    case string:
        return []byte(value), nil
    default:
        return nil, fmt.Errorf("expected binary response, got %T", raw)
    }
}

type SSEStream[T any] struct {
    scanner *bufio.Scanner
    body    io.ReadCloser
}

func (s *SSEStream[T]) Next() (T, bool, error) {
    var zero T
    for s.scanner.Scan() {
        line := strings.TrimSpace(s.scanner.Text())
        if line == "" || strings.HasPrefix(line, ":") {
            continue
        }
        if !strings.HasPrefix(line, "data:") {
            continue
        }
        data := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
        if data == "[DONE]" {
            _ = s.Close()
            return zero, false, nil
        }
        var event T
        if err := json.Unmarshal([]byte(data), &event); err != nil {
            return zero, false, err
        }
        return event, true, nil
    }
    if err := s.scanner.Err(); err != nil {
        return zero, false, err
    }
    _ = s.Close()
    return zero, false, nil
}

func (s *SSEStream[T]) Close() error {
    if s.body == nil {
        return nil
    }
    err := s.body.Close()
    s.body = nil
    return err
}

func Stream[T any](
    c *Client,
    method string,
    path string,
    body interface{},
    query map[string]interface{},
    requestHeaders map[string]string,
    contentType string,
    skipAuth bool,
    accessTokenOnly bool,
) (*SSEStream[T], error) {
    requestURL, err := url.Parse(c.baseURL + path)
    if err != nil {
        return nil, err
    }

    if len(query) > 0 {
        q := requestURL.Query()
        for key, value := range query {
            q.Set(key, fmt.Sprint(value))
        }
        requestURL.RawQuery = q.Encode()
    }

    reqBody, resolvedContentType, bodyErr := c.buildRequestBody(body, contentType)
    if bodyErr != nil {
        return nil, bodyErr
    }

    req, requestErr := http.NewRequest(method, requestURL.String(), reqBody)
    if requestErr != nil {
        return nil, requestErr
    }

    mergedHeaders, headerErr := c.mergeHeaders(requestHeaders, skipAuth, accessTokenOnly)
    if headerErr != nil {
        return nil, headerErr
    }
    for key, value := range mergedHeaders {
        req.Header.Set(key, value)
    }
    req.Header.Set("Accept", "text/event-stream")
    if reqBody != nil && resolvedContentType != "" && req.Header.Get("Content-Type") == "" {
        req.Header.Set("Content-Type", resolvedContentType)
    }

    resp, doErr := c.httpClient.Do(req)
    if doErr != nil {
        return nil, doErr
    }

    if resp.StatusCode < 200 || resp.StatusCode >= 300 {
        defer resp.Body.Close()
        responseBody, _ := io.ReadAll(resp.Body)
        return nil, fmt.Errorf("http status %d: %s", resp.StatusCode, string(responseBody))
    }

    return &SSEStream[T]{
        scanner: bufio.NewScanner(resp.Body),
        body:    resp.Body,
    }, nil
}

func (c *Client) mergeHeaders(requestHeaders map[string]string, skipAuth, accessTokenOnly bool) (common.HttpHeaders, error) {
    merged := common.HttpHeaders{}
    if !skipAuth && !accessTokenOnly {
        for key, value := range c.headers {
            merged[key] = value
        }
    }
    for key, value := range requestHeaders {
        if (!skipAuth && !accessTokenOnly) || !isCredentialHeader(key) {
            merged[key] = value
        }
    }
    if accessTokenOnly {
        accessToken := ""
        for key, value := range c.headers {
            if strings.EqualFold(key, "Access-Token") {
                accessToken = strings.TrimSpace(value)
                break
            }
        }
        if accessToken == "" {
            return nil, fmt.Errorf("access-token-only request requires Access-Token before request dispatch")
        }
        merged["Access-Token"] = accessToken
    }
    return merged, nil
}

func isCredentialHeader(key string) bool {
    switch strings.ToLower(key) {
    case "authorization", "access-token", "x-api-key", "x-tenant-id", "x-organization-id", "x-platform", "x-user-id", "x-sdkwork-tenant-id", "x-sdkwork-organization-id", "x-sdkwork-user-id":
        return true
    default:
        return false
    }
}

func (c *Client) buildMultipartBody(body interface{}) (io.Reader, string, error) {
    var buffer bytes.Buffer
    writer := multipart.NewWriter(&buffer)

    writeField := func(name string, value interface{}) error {
        switch typed := value.(type) {
        case nil:
            return writer.WriteField(name, "")
        case []string:
            for _, item := range typed {
                if err := writer.WriteField(name, item); err != nil {
                    return err
                }
            }
            return nil
        case []interface{}:
            for _, item := range typed {
                if err := writer.WriteField(name, fmt.Sprint(item)); err != nil {
                    return err
                }
            }
            return nil
        case []byte:
            part, err := writer.CreateFormFile(name, name)
            if err != nil {
                return err
            }
            _, err = part.Write(typed)
            return err
        default:
            return writer.WriteField(name, fmt.Sprint(typed))
        }
    }

    switch payload := body.(type) {
    case map[string]interface{}:
        for key, value := range payload {
            if err := writeField(key, value); err != nil {
                return nil, "", err
            }
        }
    case map[string]string:
        for key, value := range payload {
            if err := writeField(key, value); err != nil {
                return nil, "", err
            }
        }
    default:
        if body != nil {
            if err := writeField("value", body); err != nil {
                return nil, "", err
            }
        }
    }

    if err := writer.Close(); err != nil {
        return nil, "", err
    }
    return &buffer, writer.FormDataContentType(), nil
}

func (c *Client) buildFormBody(body interface{}) (io.Reader, string, error) {
    values := url.Values{}
    appendField := func(name string, value interface{}) {
        switch typed := value.(type) {
        case nil:
            values.Add(name, "")
        case []string:
            for _, item := range typed {
                values.Add(name, item)
            }
        case []interface{}:
            for _, item := range typed {
                values.Add(name, fmt.Sprint(item))
            }
        default:
            values.Add(name, fmt.Sprint(typed))
        }
    }

    switch payload := body.(type) {
    case map[string]interface{}:
        for key, value := range payload {
            appendField(key, value)
        }
    case map[string]string:
        for key, value := range payload {
            appendField(key, value)
        }
    default:
        if body != nil {
            appendField("value", body)
        }
    }

    return strings.NewReader(values.Encode()), "application/x-www-form-urlencoded", nil
}

func (c *Client) buildRequestBody(body interface{}, contentType string) (io.Reader, string, error) {
    if body == nil {
        return nil, "", nil
    }

    normalizedContentType := strings.ToLower(strings.TrimSpace(contentType))
    if normalizedContentType == "" {
        normalizedContentType = "application/json"
    }

    switch {
    case strings.HasPrefix(normalizedContentType, "multipart/form-data"):
        return c.buildMultipartBody(body)
    case strings.HasPrefix(normalizedContentType, "application/x-www-form-urlencoded"):
        return c.buildFormBody(body)
    default:
        switch typed := body.(type) {
        case []byte:
            return bytes.NewBuffer(typed), normalizedContentType, nil
        case string:
            return strings.NewReader(typed), normalizedContentType, nil
        default:
            jsonBody, marshalErr := json.Marshal(body)
            if marshalErr != nil {
                return nil, "", marshalErr
            }
            return bytes.NewBuffer(jsonBody), "application/json", nil
        }
    }
}

func (c *Client) request(
    method,
    path string,
    query map[string]interface{},
    body interface{},
    requestHeaders map[string]string,
    contentType string,
    skipAuth bool,
    accessTokenOnly bool,
) (interface{}, error) {
    requestURL, err := url.Parse(c.baseURL + path)
    if err != nil {
        return nil, err
    }

    if len(query) > 0 {
        q := requestURL.Query()
        for key, value := range query {
            q.Set(key, fmt.Sprint(value))
        }
        requestURL.RawQuery = q.Encode()
    }

    reqBody, resolvedContentType, bodyErr := c.buildRequestBody(body, contentType)
    if bodyErr != nil {
        return nil, bodyErr
    }

    req, requestErr := http.NewRequest(method, requestURL.String(), reqBody)
    if requestErr != nil {
        return nil, requestErr
    }

    mergedHeaders, headerErr := c.mergeHeaders(requestHeaders, skipAuth, accessTokenOnly)
    if headerErr != nil {
        return nil, headerErr
    }
    for key, value := range mergedHeaders {
        req.Header.Set(key, value)
    }
    if reqBody != nil && resolvedContentType != "" && req.Header.Get("Content-Type") == "" {
        req.Header.Set("Content-Type", resolvedContentType)
    }

    resp, doErr := c.httpClient.Do(req)
    if doErr != nil {
        return nil, doErr
    }
    defer resp.Body.Close()

    if resp.StatusCode < 200 || resp.StatusCode >= 300 {
        responseBody, _ := io.ReadAll(resp.Body)
        return nil, fmt.Errorf("http status %d: %s", resp.StatusCode, string(responseBody))
    }

    if resp.StatusCode == 204 {
        return nil, nil
    }

    if resp.ContentLength == 0 {
        return nil, nil
    }

    responseBody, readErr := io.ReadAll(resp.Body)
    if readErr != nil {
        return nil, readErr
    }
    if len(responseBody) == 0 {
        return nil, nil
    }

    contentType := resp.Header.Get("Content-Type")
    if strings.Contains(strings.ToLower(contentType), "application/json") {
        var result interface{}
        if decodeErr := json.Unmarshal(responseBody, &result); decodeErr != nil {
            return nil, decodeErr
        }
        return result, nil
    }

    return string(responseBody), nil
}
