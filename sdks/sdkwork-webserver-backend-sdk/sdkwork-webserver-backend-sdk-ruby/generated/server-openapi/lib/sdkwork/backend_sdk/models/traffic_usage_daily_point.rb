module Sdkwork
  module BackendSdk
    module Models
      class TrafficUsageDailyPoint
              attr_accessor :usage_date, :dimension, :quantity

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @usage_date = attributes['usageDate']
                @dimension = attributes['dimension']
                @quantity = attributes['quantity']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'usageDate' => @usage_date,
                  'dimension' => @dimension,
                  'quantity' => @quantity,
                }
              end
            end
    end
  end
end
