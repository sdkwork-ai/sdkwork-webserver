package types


type WebserverConfigWriteResult struct {
	Id string `json:"id"`
	Path string `json:"path"`
	Size string `json:"size"`
	Sha256 string `json:"sha256"`
	BackupPath string `json:"backupPath"`
	UpdatedAt string `json:"updatedAt"`
}
