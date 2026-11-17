module Sdkwork
  module BackendSdk
    module Models
      class CreateApplicationRequest
              # Create input for a carrier row. There is no `appKind` member: a carrier row's identity is `deployApplicationId`, and its application kind is a server-owned default (`STATIC_WEB`, which derives `siteType = 1` and the site carrier application type `WEB`). The member was removed rather than made optional so no caller can re-declare a second application identity on the carrier row (`MIG-2026-0071` deletion-list item 6).
              attr_accessor :name, :slug, :description, :deploy_application_id, :runtime_config, :store_listing

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @slug = attributes['slug']
                @description = attributes['description']
                @deploy_application_id = attributes['deployApplicationId']
                @runtime_config = attributes['runtimeConfig'].is_a?(Hash) ? attributes['runtimeConfig'] : {}
                @store_listing = attributes['storeListing'].is_a?(Hash) ? ApplicationStoreListing.from_hash(attributes['storeListing']) : nil
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'slug' => @slug,
                  'description' => @description,
                  'deployApplicationId' => @deploy_application_id,
                  'runtimeConfig' => @runtime_config,
                  'storeListing' => @store_listing&.to_hash,
                }
              end
            end
    end
  end
end
