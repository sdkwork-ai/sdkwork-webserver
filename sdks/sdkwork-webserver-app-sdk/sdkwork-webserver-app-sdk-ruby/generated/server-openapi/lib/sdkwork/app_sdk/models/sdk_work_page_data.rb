module Sdkwork
  module AppSdk
    module Models
      class SdkWorkPageData
              attr_accessor :items, :page_info

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @items = attributes['items'].is_a?(Array) ? attributes['items'].map { |item| item.is_a?(Hash) ? item : {} } : []
                @page_info = attributes['pageInfo'].is_a?(Hash) ? PageInfo.from_hash(attributes['pageInfo']) : nil
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'items' => @items.is_a?(Array) ? @items.map { |item| item } : [],
                  'pageInfo' => @page_info&.to_hash,
                }
              end
            end
    end
  end
end
