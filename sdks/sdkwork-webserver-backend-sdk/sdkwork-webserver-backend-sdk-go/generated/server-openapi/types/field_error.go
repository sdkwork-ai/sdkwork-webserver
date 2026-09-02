package types


type FieldError struct {
	Field string `json:"field"`
	Message string `json:"message"`
	Code int `json:"code"`
}
