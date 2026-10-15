package api

import (
    "encoding/json"
    "fmt"
    "net/url"
    "strings"
    sdktypes "github.com/sdkwork/sdkwork-webserver-backend-sdk/types"
    sdkhttp "github.com/sdkwork/sdkwork-webserver-backend-sdk/http"
)

type TunnelApi struct {
    client *sdkhttp.Client
}

func NewTunnelApi(client *sdkhttp.Client) *TunnelApi {
    return &TunnelApi{client: client}
}

// List the tunnel connector machines this gateway manages
func (a *TunnelApi) MachinesList() (sdktypes.TunnelMachinesListResponse, error) {
    raw, err := a.client.Get(BackendApiPath("/tunnel_machines"), nil, nil)
    if err != nil {
        var zero sdktypes.TunnelMachinesListResponse
        return zero, err
    }
    return decodeResult[sdktypes.TunnelMachinesListResponse](raw)
}

// Retrieve one managed tunnel machine
func (a *TunnelApi) MachinesRetrieve(deviceId string) (sdktypes.TunnelMachinesRetrieveResponse, error) {
    raw, err := a.client.Get(BackendApiPath(fmt.Sprintf("/tunnel_machines/%s", SerializePathParameter(deviceId, PathParameterSpec{Name: "deviceId", Style: "simple", Explode: false}))), nil, nil)
    if err != nil {
        var zero sdktypes.TunnelMachinesRetrieveResponse
        return zero, err
    }
    return decodeResult[sdktypes.TunnelMachinesRetrieveResponse](raw)
}

// Switch one tunnel machine on or off
func (a *TunnelApi) MachinesEnabledUpdate(deviceId string, body sdktypes.SetTunnelMachineEnabledRequest) (sdktypes.TunnelMachinesEnabledUpdateResponse, error) {
    raw, err := a.client.Put(BackendApiPath(fmt.Sprintf("/tunnel_machines/%s/enabled", SerializePathParameter(deviceId, PathParameterSpec{Name: "deviceId", Style: "simple", Explode: false}))), body, nil, nil, "application/json")
    if err != nil {
        var zero sdktypes.TunnelMachinesEnabledUpdateResponse
        return zero, err
    }
    return decodeResult[sdktypes.TunnelMachinesEnabledUpdateResponse](raw)
}

// Declare a route for one tunnel machine
func (a *TunnelApi) MachinesRoutesCreate(deviceId string, body sdktypes.CreateTunnelMachineRouteRequest, idempotencyKey string) (sdktypes.TunnelMachinesRoutesCreateResponse201, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Post(BackendApiPath(fmt.Sprintf("/tunnel_machines/%s/routes", SerializePathParameter(deviceId, PathParameterSpec{Name: "deviceId", Style: "simple", Explode: false}))), body, nil, headers, "application/json")
    if err != nil {
        var zero sdktypes.TunnelMachinesRoutesCreateResponse201
        return zero, err
    }
    return decodeResult[sdktypes.TunnelMachinesRoutesCreateResponse201](raw)
}

// Remove one route from a tunnel machine
func (a *TunnelApi) MachinesRoutesDelete(deviceId string, routeId string, idempotencyKey string) (struct{}, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Delete(BackendApiPath(fmt.Sprintf("/tunnel_machines/%s/routes/%s", SerializePathParameter(deviceId, PathParameterSpec{Name: "deviceId", Style: "simple", Explode: false}), SerializePathParameter(routeId, PathParameterSpec{Name: "routeId", Style: "simple", Explode: false}))), nil, headers)
    if err != nil {
        var zero struct{}
        return zero, err
    }
    return decodeResult[struct{}](raw)
}

type PathParameterSpec struct {
    Name    string
    Style   string
    Explode bool
}

func SerializePathParameter(value interface{}, spec PathParameterSpec) string {
    if value == nil {
        return ""
    }
    style := spec.Style
    if style == "" {
        style = "simple"
    }

    switch typed := value.(type) {
    case []string:
        return SerializePathArray(spec.Name, stringSliceToInterface(typed), style, spec.Explode)
    case []int:
        return SerializePathArray(spec.Name, intSliceToInterface(typed), style, spec.Explode)
    case []interface{}:
        return SerializePathArray(spec.Name, typed, style, spec.Explode)
    case map[string]string:
        return SerializePathObject(spec.Name, stringMapToInterface(typed), style, spec.Explode)
    case map[string]int:
        return SerializePathObject(spec.Name, intMapToInterface(typed), style, spec.Explode)
    case map[string]interface{}:
        return SerializePathObject(spec.Name, typed, style, spec.Explode)
    default:
        return PathPrefix(spec.Name, style) + url.PathEscape(fmt.Sprint(value))
    }
}

func SerializePathArray(name string, values []interface{}, style string, explode bool) string {
    serialized := make([]string, 0, len(values))
    for _, item := range values {
        if item != nil {
            serialized = append(serialized, url.PathEscape(fmt.Sprint(item)))
        }
    }
    if len(serialized) == 0 {
        return PathPrefix(name, style)
    }
    if style == "matrix" {
        if explode {
            parts := make([]string, 0, len(serialized))
            for _, item := range serialized {
                parts = append(parts, ";"+name+"="+item)
            }
            return strings.Join(parts, "")
        }
        return ";" + name + "=" + strings.Join(serialized, ",")
    }
    separator := ","
    if explode {
        separator = "."
    }
    return PathPrefix(name, style) + strings.Join(serialized, separator)
}

func SerializePathObject(name string, values map[string]interface{}, style string, explode bool) string {
    entries := make([]string, 0, len(values)*2)
    exploded := make([]string, 0, len(values))
    for key, value := range values {
        if value == nil {
            continue
        }
        escapedKey := url.PathEscape(key)
        escapedValue := url.PathEscape(fmt.Sprint(value))
        if explode {
            if style == "matrix" {
                exploded = append(exploded, ";"+escapedKey+"="+escapedValue)
            } else {
                exploded = append(exploded, escapedKey+"="+escapedValue)
            }
        } else {
            entries = append(entries, escapedKey, escapedValue)
        }
    }
    if style == "matrix" {
        if explode {
            return strings.Join(exploded, "")
        }
        return ";" + name + "=" + strings.Join(entries, ",")
    }
    if explode {
        separator := ","
        if style == "label" {
            separator = "."
        }
        return PathPrefix(name, style) + strings.Join(exploded, separator)
    }
    return PathPrefix(name, style) + strings.Join(entries, ",")
}

func PathPrefix(name string, style string) string {
    if style == "label" {
        return "."
    }
    if style == "matrix" {
        return ";" + name
    }
    return ""
}

type ParameterSpec struct {
    Value       interface{}
    Style       string
    Explode     bool
    ContentType string
}

func BuildRequestHeaders(headers map[string]ParameterSpec, cookies map[string]ParameterSpec) map[string]string {
    requestHeaders := map[string]string{}
    for name, parameter := range headers {
        if serialized, ok := SerializeParameterValue(parameter); ok {
            requestHeaders[name] = serialized
        }
    }

    if cookieHeader := BuildCookieHeader(cookies); cookieHeader != "" {
        if existing, ok := requestHeaders["Cookie"]; ok && existing != "" {
            requestHeaders["Cookie"] = existing + "; " + cookieHeader
        } else {
            requestHeaders["Cookie"] = cookieHeader
        }
    }

    if len(requestHeaders) == 0 {
        return nil
    }
    return requestHeaders
}

func BuildCookieHeader(cookies map[string]ParameterSpec) string {
    pairs := make([]string, 0, len(cookies))
    for name, parameter := range cookies {
        if serialized, ok := SerializeParameterValue(parameter); ok {
            pairs = append(pairs, url.QueryEscape(name)+"="+url.QueryEscape(serialized))
        }
    }
    return strings.Join(pairs, "; ")
}

func SerializeParameterValue(parameter ParameterSpec) (string, bool) {
    value := parameter.Value
    if value == nil {
        return "", false
    }
    if parameter.ContentType != "" {
        encoded, _ := json.Marshal(value)
        return string(encoded), true
    }
    switch typed := value.(type) {
    case string:
        return typed, true
    case fmt.Stringer:
        return typed.String(), true
    case []string:
        return strings.Join(typed, ","), true
    case []int:
        values := make([]string, 0, len(typed))
        for _, item := range typed {
            values = append(values, fmt.Sprint(item))
        }
        return strings.Join(values, ","), true
    case map[string]string:
        return SerializeHeaderObject(stringMapToInterface(typed), parameter.Explode), true
    case map[string]int:
        return SerializeHeaderObject(intMapToInterface(typed), parameter.Explode), true
    case map[string]interface{}:
        return SerializeHeaderObject(typed, parameter.Explode), true
    default:
        return fmt.Sprint(value), true
    }
}

func SerializeHeaderObject(values map[string]interface{}, explode bool) string {
    serialized := make([]string, 0, len(values)*2)
    for key, value := range values {
        if value == nil {
            continue
        }
        if explode {
            serialized = append(serialized, key+"="+fmt.Sprint(value))
        } else {
            serialized = append(serialized, key, fmt.Sprint(value))
        }
    }
    return strings.Join(serialized, ",")
}
func stringSliceToInterface(values []string) []interface{} {
    result := make([]interface{}, 0, len(values))
    for _, value := range values {
        result = append(result, value)
    }
    return result
}

func intSliceToInterface(values []int) []interface{} {
    result := make([]interface{}, 0, len(values))
    for _, value := range values {
        result = append(result, value)
    }
    return result
}

func stringMapToInterface(values map[string]string) map[string]interface{} {
    result := make(map[string]interface{}, len(values))
    for key, value := range values {
        result[key] = value
    }
    return result
}

func intMapToInterface(values map[string]int) map[string]interface{} {
    result := make(map[string]interface{}, len(values))
    for key, value := range values {
        result[key] = value
    }
    return result
}
