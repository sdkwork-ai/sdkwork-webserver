module Sdkwork
  module BackendSdk
    module Models
      class WebserverConfigCatalog
              attr_accessor :config_root, :items, :truncated, :page_info

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @config_root = attributes['configRoot']
                @items = attributes['items'].is_a?(Array) ? attributes['items'].map { |item| item.is_a?(Hash) ? WebserverConfigEntry.from_hash(item) : item } : []
                @truncated = attributes['truncated']
                @page_info = attributes['pageInfo'].is_a?(Hash) ? PageInfo.from_hash(attributes['pageInfo']) : nil
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'configRoot' => @config_root,
                  'items' => @items.is_a?(Array) ? @items.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'truncated' => @truncated,
                  'pageInfo' => @page_info&.to_hash,
                }
              end
            end
    end
  end
end
