module Sdkwork
  module AppSdk
    module Models
      class ApplicationResponse
              attr_accessor :id, :name, :slug, :description, :site_id, :application_type, :site_type, :status, :runtime_config, :store_listing, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @name = attributes['name']
                @slug = attributes['slug']
                @description = attributes['description']
                @site_id = attributes['siteId']
                @application_type = attributes['applicationType']
                @site_type = attributes['siteType']
                @status = attributes['status']
                @runtime_config = attributes['runtimeConfig'].is_a?(Hash) ? attributes['runtimeConfig'] : {}
                @store_listing = attributes['storeListing'].is_a?(Hash) ? ApplicationStoreListing.from_hash(attributes['storeListing']) : nil
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'name' => @name,
                  'slug' => @slug,
                  'description' => @description,
                  'siteId' => @site_id,
                  'applicationType' => @application_type,
                  'siteType' => @site_type,
                  'status' => @status,
                  'runtimeConfig' => @runtime_config,
                  'storeListing' => @store_listing&.to_hash,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end
