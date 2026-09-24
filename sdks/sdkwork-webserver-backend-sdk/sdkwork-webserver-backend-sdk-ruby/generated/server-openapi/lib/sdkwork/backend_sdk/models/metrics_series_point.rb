module Sdkwork
  module BackendSdk
    module Models
      class MetricsSeriesPoint
              attr_accessor :date, :quantity

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @date = attributes['date']
                @quantity = attributes['quantity']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'date' => @date,
                  'quantity' => @quantity,
                }
              end
            end
    end
  end
end
