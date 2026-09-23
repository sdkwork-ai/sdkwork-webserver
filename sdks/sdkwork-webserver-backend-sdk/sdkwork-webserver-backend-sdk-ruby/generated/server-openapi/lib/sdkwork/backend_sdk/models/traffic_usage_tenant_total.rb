module Sdkwork
  module BackendSdk
    module Models
      class TrafficUsageTenantTotal
              attr_accessor :tenant_id, :dimension, :quantity, :unit

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @tenant_id = attributes['tenantId']
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
                  'tenantId' => @tenant_id,
                  'dimension' => @dimension,
                  'quantity' => @quantity,
                  'unit' => @unit,
                }
              end
            end
    end
  end
end
