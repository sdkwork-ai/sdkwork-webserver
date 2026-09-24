module Sdkwork
  module BackendSdk
    module Models
      class MetricsWindowValue
              attr_accessor :window, :quantity, :unit

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @window = attributes['window']
                @quantity = attributes['quantity']
                @unit = attributes['unit']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'window' => @window,
                  'quantity' => @quantity,
                  'unit' => @unit,
                }
              end
            end
    end
  end
end
