package types


type ApplicationStoreListing struct {
	Icon MediaResource `json:"icon"`
	Cover MediaResource `json:"cover"`
	Previews []MediaResource `json:"previews"`
	ShortDescription string `json:"shortDescription"`
	FullDescription string `json:"fullDescription"`
	ReleaseNotes string `json:"releaseNotes"`
	Category string `json:"category"`
	Keywords []string `json:"keywords"`
	SupportUrl string `json:"supportUrl"`
	PrivacyPolicyUrl string `json:"privacyPolicyUrl"`
	OfficialWebsiteUrl string `json:"officialWebsiteUrl"`
}
