module Sdkwork
  module BackendSdk
    module Models
      class AuditLogPage
              attr_accessor :items, :total

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @items = attributes['items'].is_a?(Array) ? attributes['items'].map { |item| item.is_a?(Hash) ? AuditLogResponse.from_hash(item) : item } : []
                @total = attributes['total']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'items' => @items.is_a?(Array) ? @items.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'total' => @total,
                }
              end
            end
    end
  end
end
