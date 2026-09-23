module Sdkwork
  module BackendSdk
    module Models
      class TrafficUsageTotal
              attr_accessor :dimension, :quantity, :unit

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @dimension = attributes['dimension']
                @quantity = attributes['quantity']
                @unit = attributes['unit']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'dimension' => @dimension,
                  'quantity' => @quantity,
                  'unit' => @unit,
                }
              end
            end
    end
  end
end
