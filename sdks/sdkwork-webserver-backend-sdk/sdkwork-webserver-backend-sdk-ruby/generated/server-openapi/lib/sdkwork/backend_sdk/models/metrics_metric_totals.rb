module Sdkwork
  module BackendSdk
    module Models
      class MetricsMetricTotals
              # One metric across every window. `metric` is one of the entity ids (`users`, `tenants`, `applications`, `agents`), one of the storage ids (`storage.used_bytes`, `storage.object_count`), or a metered dimension (`traffic.requests`, …), which is left open on the same terms as the traffic readings: a dimension this contract has not heard of reaches the surface instead of failing the response.
              attr_accessor :metric, :unit, :values

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @metric = attributes['metric']
                @unit = attributes['unit']
                @values = attributes['values'].is_a?(Array) ? attributes['values'].map { |item| item.is_a?(Hash) ? MetricsWindowValue.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'metric' => @metric,
                  'unit' => @unit,
                  'values' => @values.is_a?(Array) ? @values.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
