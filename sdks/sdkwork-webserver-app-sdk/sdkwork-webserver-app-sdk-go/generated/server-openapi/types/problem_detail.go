package types


type ProblemDetail struct {
	Type string `json:"type"`
	Title string `json:"title"`
	Status int `json:"status"`
	Detail string `json:"detail"`
	Instance string `json:"instance"`
	Code SdkWorkPlatformErrorCode `json:"code"`
	TraceId string `json:"traceId"`
	Errors []FieldError `json:"errors"`
}
