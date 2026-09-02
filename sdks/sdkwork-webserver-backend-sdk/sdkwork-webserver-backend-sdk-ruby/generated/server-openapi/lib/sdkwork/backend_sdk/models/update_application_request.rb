module Sdkwork
  module BackendSdk
    module Models
      class UpdateApplicationRequest
              attr_accessor :name, :description, :runtime_config, :store_listing

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @description = attributes['description']
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
                  'description' => @description,
                  'runtimeConfig' => @runtime_config,
                  'storeListing' => @store_listing&.to_hash,
                }
              end
            end
    end
  end
end
