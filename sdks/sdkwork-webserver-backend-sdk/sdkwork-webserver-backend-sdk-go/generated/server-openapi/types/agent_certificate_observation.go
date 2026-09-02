package types


type AgentCertificateObservation struct {
	CertificateId string `json:"certificateId"`
	Fingerprint string `json:"fingerprint"`
	SyncVersion string `json:"syncVersion"`
	State string `json:"state"`
	ObservedAt string `json:"observedAt"`
	FailureCode string `json:"failureCode"`
}
