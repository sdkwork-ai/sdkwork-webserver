module Sdkwork
  module AppSdk
    module Models
      class ApplicationStoreListing
              attr_accessor :icon, :cover, :previews, :short_description, :full_description, :release_notes, :category, :keywords, :support_url, :privacy_policy_url, :official_website_url

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @icon = attributes['icon'].is_a?(Hash) ? MediaResource.from_hash(attributes['icon']) : nil
                @cover = attributes['cover'].is_a?(Hash) ? MediaResource.from_hash(attributes['cover']) : nil
                @previews = attributes['previews'].is_a?(Array) ? attributes['previews'].map { |item| item.is_a?(Hash) ? MediaResource.from_hash(item) : item } : []
                @short_description = attributes['shortDescription']
                @full_description = attributes['fullDescription']
                @release_notes = attributes['releaseNotes']
                @category = attributes['category']
                @keywords = attributes['keywords'].is_a?(Array) ? attributes['keywords'].map { |item| item } : []
                @support_url = attributes['supportUrl']
                @privacy_policy_url = attributes['privacyPolicyUrl']
                @official_website_url = attributes['officialWebsiteUrl']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'icon' => @icon&.to_hash,
                  'cover' => @cover&.to_hash,
                  'previews' => @previews.is_a?(Array) ? @previews.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'shortDescription' => @short_description,
                  'fullDescription' => @full_description,
                  'releaseNotes' => @release_notes,
                  'category' => @category,
                  'keywords' => @keywords.is_a?(Array) ? @keywords.map { |item| item } : [],
                  'supportUrl' => @support_url,
                  'privacyPolicyUrl' => @privacy_policy_url,
                  'officialWebsiteUrl' => @official_website_url,
                }
              end
            end
    end
  end
end
