module Sdkwork
  module BackendSdk
    module Models
      class ApplicationResponse
              attr_accessor :id, :name, :slug, :description, :app_kind, :site_type, :status, :runtime_config, :store_listing, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @name = attributes['name']
                @slug = attributes['slug']
                @description = attributes['description']
                @app_kind = attributes['appKind']
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
                  'appKind' => @app_kind,
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
