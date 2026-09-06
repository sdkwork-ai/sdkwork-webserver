package types


type WebserverConfigCatalog struct {
	ConfigRoot string `json:"configRoot"`
	Items []WebserverConfigEntry `json:"items"`
}
