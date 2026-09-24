package api

import (
    "encoding/json"
    "fmt"
    "net/url"
    "strings"
    sdktypes "github.com/sdkwork/sdkwork-webserver-backend-sdk/types"
    sdkhttp "github.com/sdkwork/sdkwork-webserver-backend-sdk/http"
)

type ClusterApi struct {
    client *sdkhttp.Client
}

func NewClusterApi(client *sdkhttp.Client) *ClusterApi {
    return &ClusterApi{client: client}
}

// List Web Server clusters
func (a *ClusterApi) ClustersList(page *int, pageSize *int) (sdktypes.ClustersListResponse, error) {
    query := BuildQueryString([]QueryParameterSpec{
        {Name: "page", Value: func() interface{} { if page == nil { return nil }; return *page }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "page_size", Value: func() interface{} { if pageSize == nil { return nil }; return *pageSize }(), Style: "form", Explode: true, AllowReserved: false},
    })
    raw, err := a.client.Get(AppendQueryString(BackendApiPath("/clusters"), query), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersListResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersListResponse](raw)
}

// Create a Web Server cluster
func (a *ClusterApi) ClustersCreate(body sdktypes.CreateClusterRequest, idempotencyKey string) (sdktypes.ClustersCreateResponse201, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Post(BackendApiPath("/clusters"), body, nil, headers, "application/json")
    if err != nil {
        var zero sdktypes.ClustersCreateResponse201
        return zero, err
    }
    return decodeResult[sdktypes.ClustersCreateResponse201](raw)
}

// Retrieve a Web Server cluster
func (a *ClusterApi) ClustersRetrieve(clusterId string) (sdktypes.ClustersRetrieveResponse, error) {
    raw, err := a.client.Get(BackendApiPath(fmt.Sprintf("/clusters/%s", SerializePathParameter(clusterId, PathParameterSpec{Name: "clusterId", Style: "simple", Explode: false}))), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersRetrieveResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersRetrieveResponse](raw)
}

// Update a Web Server cluster
func (a *ClusterApi) ClustersUpdate(clusterId string, body sdktypes.UpdateClusterRequest, idempotencyKey string) (sdktypes.ClustersUpdateResponse, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Patch(BackendApiPath(fmt.Sprintf("/clusters/%s", SerializePathParameter(clusterId, PathParameterSpec{Name: "clusterId", Style: "simple", Explode: false}))), body, nil, headers, "application/json")
    if err != nil {
        var zero sdktypes.ClustersUpdateResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersUpdateResponse](raw)
}

// Delete an empty Web Server cluster
func (a *ClusterApi) ClustersDelete(clusterId string, idempotencyKey string) (struct{}, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Delete(BackendApiPath(fmt.Sprintf("/clusters/%s", SerializePathParameter(clusterId, PathParameterSpec{Name: "clusterId", Style: "simple", Explode: false}))), nil, headers)
    if err != nil {
        var zero struct{}
        return zero, err
    }
    return decodeResult[struct{}](raw)
}

// Publish a desired-state revision to every instance of the cluster
func (a *ClusterApi) ClustersSync(clusterId string, body sdktypes.PublishClusterSyncRequest) (sdktypes.ClustersSyncResponse, error) {
    raw, err := a.client.Post(BackendApiPath(fmt.Sprintf("/clusters/%s/sync", SerializePathParameter(clusterId, PathParameterSpec{Name: "clusterId", Style: "simple", Explode: false}))), body, nil, nil, "application/json")
    if err != nil {
        var zero sdktypes.ClustersSyncResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersSyncResponse](raw)
}

// List cluster hosts with system and network identity
func (a *ClusterApi) ClustersHostsList(pageSize *int, cursor *string, clusterId *string, status *int) (sdktypes.ClustersHostsListResponse, error) {
    query := BuildQueryString([]QueryParameterSpec{
        {Name: "page_size", Value: func() interface{} { if pageSize == nil { return nil }; return *pageSize }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cursor", Value: func() interface{} { if cursor == nil { return nil }; return *cursor }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cluster_id", Value: func() interface{} { if clusterId == nil { return nil }; return *clusterId }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "status", Value: func() interface{} { if status == nil { return nil }; return *status }(), Style: "form", Explode: true, AllowReserved: false},
    })
    raw, err := a.client.Get(AppendQueryString(BackendApiPath("/clusters/hosts"), query), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersHostsListResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersHostsListResponse](raw)
}

// Retrieve a cluster host
func (a *ClusterApi) ClustersHostsRetrieve(hostId string) (sdktypes.ClustersHostsRetrieveResponse, error) {
    raw, err := a.client.Get(BackendApiPath(fmt.Sprintf("/clusters/hosts/%s", SerializePathParameter(hostId, PathParameterSpec{Name: "hostId", Style: "simple", Explode: false}))), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersHostsRetrieveResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersHostsRetrieveResponse](raw)
}

// Rename a host or reassign it to another cluster
func (a *ClusterApi) ClustersHostsUpdate(hostId string, body sdktypes.UpdateClusterHostRequest, idempotencyKey string) (sdktypes.ClustersHostsUpdateResponse, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Patch(BackendApiPath(fmt.Sprintf("/clusters/hosts/%s", SerializePathParameter(hostId, PathParameterSpec{Name: "hostId", Style: "simple", Explode: false}))), body, nil, headers, "application/json")
    if err != nil {
        var zero sdktypes.ClustersHostsUpdateResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersHostsUpdateResponse](raw)
}

// Remove an instance-free host from the cluster inventory
func (a *ClusterApi) ClustersHostsDelete(hostId string, idempotencyKey string) (struct{}, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Delete(BackendApiPath(fmt.Sprintf("/clusters/hosts/%s", SerializePathParameter(hostId, PathParameterSpec{Name: "hostId", Style: "simple", Explode: false}))), nil, headers)
    if err != nil {
        var zero struct{}
        return zero, err
    }
    return decodeResult[struct{}](raw)
}

// List webserver process instances with liveness state
func (a *ClusterApi) ClustersInstancesList(pageSize *int, cursor *string, clusterId *string, hostId *string, status *int, healthState *string) (sdktypes.ClustersInstancesListResponse, error) {
    query := BuildQueryString([]QueryParameterSpec{
        {Name: "page_size", Value: func() interface{} { if pageSize == nil { return nil }; return *pageSize }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cursor", Value: func() interface{} { if cursor == nil { return nil }; return *cursor }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cluster_id", Value: func() interface{} { if clusterId == nil { return nil }; return *clusterId }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "host_id", Value: func() interface{} { if hostId == nil { return nil }; return *hostId }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "status", Value: func() interface{} { if status == nil { return nil }; return *status }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "health_state", Value: func() interface{} { if healthState == nil { return nil }; return *healthState }(), Style: "form", Explode: true, AllowReserved: false},
    })
    raw, err := a.client.Get(AppendQueryString(BackendApiPath("/clusters/instances"), query), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersInstancesListResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesListResponse](raw)
}

// Retrieve a webserver process instance
func (a *ClusterApi) ClustersInstancesRetrieve(instanceId string) (sdktypes.ClustersInstancesRetrieveResponse, error) {
    raw, err := a.client.Get(BackendApiPath(fmt.Sprintf("/clusters/instances/%s", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersInstancesRetrieveResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesRetrieveResponse](raw)
}

// Update an instance display name, status, or advertised endpoint
func (a *ClusterApi) ClustersInstancesUpdate(instanceId string, body sdktypes.UpdateClusterInstanceRequest, idempotencyKey string) (sdktypes.ClustersInstancesUpdateResponse, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Patch(BackendApiPath(fmt.Sprintf("/clusters/instances/%s", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), body, nil, headers, "application/json")
    if err != nil {
        var zero sdktypes.ClustersInstancesUpdateResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesUpdateResponse](raw)
}

// Unregister a webserver process instance
func (a *ClusterApi) ClustersInstancesDelete(instanceId string, idempotencyKey string) (struct{}, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Delete(BackendApiPath(fmt.Sprintf("/clusters/instances/%s", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), nil, headers)
    if err != nil {
        var zero struct{}
        return zero, err
    }
    return decodeResult[struct{}](raw)
}

// List cluster lifecycle events
func (a *ClusterApi) ClustersEventsList(pageSize *int, cursor *string, clusterId *string, severity *string) (sdktypes.ClustersEventsListResponse, error) {
    query := BuildQueryString([]QueryParameterSpec{
        {Name: "page_size", Value: func() interface{} { if pageSize == nil { return nil }; return *pageSize }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cursor", Value: func() interface{} { if cursor == nil { return nil }; return *cursor }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cluster_id", Value: func() interface{} { if clusterId == nil { return nil }; return *clusterId }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "severity", Value: func() interface{} { if severity == nil { return nil }; return *severity }(), Style: "form", Explode: true, AllowReserved: false},
    })
    raw, err := a.client.Get(AppendQueryString(BackendApiPath("/clusters/events"), query), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersEventsListResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersEventsListResponse](raw)
}

// Retrieve the cluster health overview for status polling
func (a *ClusterApi) ClustersOverviewRetrieve() (sdktypes.ClustersOverviewRetrieveResponse, error) {
    raw, err := a.client.Get(BackendApiPath("/clusters/overview"), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersOverviewRetrieveResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersOverviewRetrieveResponse](raw)
}

// List one instance's stored heartbeat samples
func (a *ClusterApi) ClustersInstancesHeartbeatsList(instanceId string, pageSize *int, cursor *string) (sdktypes.ClustersInstancesHeartbeatsListResponse, error) {
    query := BuildQueryString([]QueryParameterSpec{
        {Name: "page_size", Value: func() interface{} { if pageSize == nil { return nil }; return *pageSize }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cursor", Value: func() interface{} { if cursor == nil { return nil }; return *cursor }(), Style: "form", Explode: true, AllowReserved: false},
    })
    raw, err := a.client.Get(AppendQueryString(BackendApiPath(fmt.Sprintf("/clusters/instances/%s/heartbeats", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), query), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersInstancesHeartbeatsListResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesHeartbeatsListResponse](raw)
}

// List one instance's heartbeat metric samples for trend charts
func (a *ClusterApi) ClustersInstancesMetricsList(instanceId string, pageSize *int, cursor *string) (sdktypes.ClustersInstancesMetricsListResponse, error) {
    query := BuildQueryString([]QueryParameterSpec{
        {Name: "page_size", Value: func() interface{} { if pageSize == nil { return nil }; return *pageSize }(), Style: "form", Explode: true, AllowReserved: false},
        {Name: "cursor", Value: func() interface{} { if cursor == nil { return nil }; return *cursor }(), Style: "form", Explode: true, AllowReserved: false},
    })
    raw, err := a.client.Get(AppendQueryString(BackendApiPath(fmt.Sprintf("/clusters/instances/%s/metrics/history", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), query), nil, nil)
    if err != nil {
        var zero sdktypes.ClustersInstancesMetricsListResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesMetricsListResponse](raw)
}

// Probe one instance's connectivity and record the outcome
func (a *ClusterApi) ClustersInstancesProbe(instanceId string, body *sdktypes.ProbeClusterInstanceRequest) (sdktypes.ClustersInstancesProbeResponse, error) {
    raw, err := a.client.Post(BackendApiPath(fmt.Sprintf("/clusters/instances/%s/probe", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), body, nil, nil, "application/json")
    if err != nil {
        var zero sdktypes.ClustersInstancesProbeResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesProbeResponse](raw)
}

// Gracefully drain one instance out of routing
func (a *ClusterApi) ClustersInstancesDrain(instanceId string) (sdktypes.ClustersInstancesDrainResponse, error) {
    raw, err := a.client.Post(BackendApiPath(fmt.Sprintf("/clusters/instances/%s/drain", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), nil, nil, nil, "")
    if err != nil {
        var zero sdktypes.ClustersInstancesDrainResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesDrainResponse](raw)
}

// Clear the drain flag and restore routing participation
func (a *ClusterApi) ClustersInstancesUndrain(instanceId string) (sdktypes.ClustersInstancesUndrainResponse, error) {
    raw, err := a.client.Post(BackendApiPath(fmt.Sprintf("/clusters/instances/%s/undrain", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), nil, nil, nil, "")
    if err != nil {
        var zero sdktypes.ClustersInstancesUndrainResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesUndrainResponse](raw)
}

// Cordon one instance out of routing without draining it
func (a *ClusterApi) ClustersInstancesCordon(instanceId string) (sdktypes.ClustersInstancesCordonResponse, error) {
    raw, err := a.client.Post(BackendApiPath(fmt.Sprintf("/clusters/instances/%s/cordon", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), nil, nil, nil, "")
    if err != nil {
        var zero sdktypes.ClustersInstancesCordonResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesCordonResponse](raw)
}

// Uncordon one instance back into routing
func (a *ClusterApi) ClustersInstancesUncordon(instanceId string) (sdktypes.ClustersInstancesUncordonResponse, error) {
    raw, err := a.client.Post(BackendApiPath(fmt.Sprintf("/clusters/instances/%s/uncordon", SerializePathParameter(instanceId, PathParameterSpec{Name: "instanceId", Style: "simple", Explode: false}))), nil, nil, nil, "")
    if err != nil {
        var zero sdktypes.ClustersInstancesUncordonResponse
        return zero, err
    }
    return decodeResult[sdktypes.ClustersInstancesUncordonResponse](raw)
}

// Enqueue a peer message to one instance or broadcast to online members
func (a *ClusterApi) ClustersMessagesCreate(body sdktypes.EnqueueClusterPeerMessagesRequest, idempotencyKey string) (sdktypes.ClustersMessagesCreateResponse201, error) {
    headers := BuildRequestHeaders(
        map[string]ParameterSpec{"Idempotency-Key": ParameterSpec{Value: idempotencyKey, Style: "simple", Explode: false},},
        map[string]ParameterSpec{},
    )
    raw, err := a.client.Post(BackendApiPath("/clusters/messages"), body, nil, headers, "application/json")
    if err != nil {
        var zero sdktypes.ClustersMessagesCreateResponse201
        return zero, err
    }
    return decodeResult[sdktypes.ClustersMessagesCreateResponse201](raw)
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
type QueryParameterSpec struct {
    Name          string
    Value         interface{}
    Style         string
    Explode       bool
    AllowReserved bool
    ContentType   string
}

func BuildQueryString(parameters []QueryParameterSpec) string {
    pairs := make([]string, 0)
    for _, parameter := range parameters {
        AppendSerializedParameter(&pairs, parameter)
    }
    return strings.Join(pairs, "&")
}

func AppendSerializedParameter(pairs *[]string, parameter QueryParameterSpec) {
    if parameter.Value == nil {
        return
    }

    if parameter.ContentType != "" {
        encoded, _ := json.Marshal(parameter.Value)
        *pairs = append(*pairs, url.QueryEscape(parameter.Name)+"="+EncodeQueryValue(string(encoded), parameter.AllowReserved))
        return
    }

    style := parameter.Style
    if style == "" {
        style = "form"
    }

    switch value := parameter.Value.(type) {
    case []string:
        AppendArrayParameter(pairs, parameter.Name, stringSliceToInterface(value), style, parameter.Explode, parameter.AllowReserved)
    case []int:
        AppendArrayParameter(pairs, parameter.Name, intSliceToInterface(value), style, parameter.Explode, parameter.AllowReserved)
    case []interface{}:
        AppendArrayParameter(pairs, parameter.Name, value, style, parameter.Explode, parameter.AllowReserved)
    case map[string]int:
        AppendObjectParameter(pairs, parameter.Name, intMapToInterface(value), style, parameter.Explode, parameter.AllowReserved)
    case map[string]string:
        AppendObjectParameter(pairs, parameter.Name, stringMapToInterface(value), style, parameter.Explode, parameter.AllowReserved)
    case map[string]interface{}:
        if style == "deepObject" {
            AppendDeepObjectParameter(pairs, parameter.Name, value, parameter.AllowReserved)
        } else {
            AppendObjectParameter(pairs, parameter.Name, value, style, parameter.Explode, parameter.AllowReserved)
        }
    default:
        *pairs = append(*pairs, url.QueryEscape(parameter.Name)+"="+EncodeQueryValue(fmt.Sprint(value), parameter.AllowReserved))
    }
}

func AppendArrayParameter(pairs *[]string, name string, value []interface{}, style string, explode bool, allowReserved bool) {
    values := make([]string, 0, len(value))
    for _, item := range value {
        if item != nil {
            values = append(values, fmt.Sprint(item))
        }
    }
    if len(values) == 0 {
        return
    }
    if style == "form" && explode {
        for _, item := range values {
            *pairs = append(*pairs, url.QueryEscape(name)+"="+EncodeQueryValue(item, allowReserved))
        }
        return
    }
    *pairs = append(*pairs, url.QueryEscape(name)+"="+EncodeQueryValue(strings.Join(values, ","), allowReserved))
}

func AppendObjectParameter(pairs *[]string, name string, value map[string]interface{}, style string, explode bool, allowReserved bool) {
    entries := make([]string, 0, len(value)*2)
    for key, item := range value {
        if item == nil {
            continue
        }
        if style == "form" && explode {
            *pairs = append(*pairs, url.QueryEscape(key)+"="+EncodeQueryValue(fmt.Sprint(item), allowReserved))
            continue
        }
        entries = append(entries, key, fmt.Sprint(item))
    }
    if len(entries) == 0 {
        return
    }
    if !(style == "form" && explode) {
        *pairs = append(*pairs, url.QueryEscape(name)+"="+EncodeQueryValue(strings.Join(entries, ","), allowReserved))
    }
}

func AppendDeepObjectParameter(pairs *[]string, name string, value map[string]interface{}, allowReserved bool) {
    for key, item := range value {
        if item == nil {
            continue
        }
        *pairs = append(*pairs, url.QueryEscape(fmt.Sprintf("%s[%s]", name, key))+"="+EncodeQueryValue(fmt.Sprint(item), allowReserved))
    }
}

func EncodeQueryValue(value string, allowReserved bool) string {
    encoded := url.QueryEscape(value)
    if !allowReserved {
        return encoded
    }
    replacements := map[string]string{
        "%3A": ":", "%2F": "/", "%3F": "?", "%23": "#",
        "%5B": "[", "%5D": "]", "%40": "@", "%21": "!",
        "%24": "$", "%26": "&", "%27": "'", "%28": "(",
        "%29": ")", "%2A": "*", "%2B": "+", "%2C": ",",
        "%3B": ";", "%3D": "=",
    }
    for escaped, reserved := range replacements {
        encoded = strings.ReplaceAll(encoded, escaped, reserved)
    }
    return encoded
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
