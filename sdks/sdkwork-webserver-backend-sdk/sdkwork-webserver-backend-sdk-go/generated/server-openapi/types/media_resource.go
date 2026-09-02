package types


type MediaResource struct {
	Id string `json:"id"`
	Kind string `json:"kind"`
	Source string `json:"source"`
	Url string `json:"url"`
	PublicUrl string `json:"publicUrl"`
	Uri string `json:"uri"`
	ObjectBlobId string `json:"objectBlobId"`
	FileName string `json:"fileName"`
	MimeType string `json:"mimeType"`
	SizeBytes string `json:"sizeBytes"`
	Checksum MediaChecksum `json:"checksum"`
	Width int `json:"width"`
	Height int `json:"height"`
	DurationSeconds float64 `json:"durationSeconds"`
	AltText string `json:"altText"`
	Title string `json:"title"`
	Metadata map[string]interface{} `json:"metadata"`
}
