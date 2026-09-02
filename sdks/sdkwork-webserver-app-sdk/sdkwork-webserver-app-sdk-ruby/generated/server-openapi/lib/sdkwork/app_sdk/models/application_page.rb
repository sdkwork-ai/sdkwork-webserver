module Sdkwork
  module AppSdk
    module Models
      class ApplicationPage
              attr_accessor :items, :total, :page, :page_size

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @items = attributes['items'].is_a?(Array) ? attributes['items'].map { |item| item.is_a?(Hash) ? ApplicationResponse.from_hash(item) : item } : []
                @total = attributes['total']
                @page = attributes['page']
                @page_size = attributes['pageSize']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'items' => @items.is_a?(Array) ? @items.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'total' => @total,
                  'page' => @page,
                  'pageSize' => @page_size,
                }
              end
            end
    end
  end
end
