package types


type SourceVersionConfigSnapshot struct {
	AppConfigPath string `json:"appConfigPath"`
	DeploymentConfigPath string `json:"deploymentConfigPath"`
	AppConfigDetected bool `json:"appConfigDetected"`
	DeploymentConfigDetected bool `json:"deploymentConfigDetected"`
}
