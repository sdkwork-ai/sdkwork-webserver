module Sdkwork
  module BackendSdk
    module Models
      class MetricsSeries
              # One entity metric's per-day arrivals over `MetricsSeriesWindow`, days ascending. Points are **sparse**: a day the metric gained nothing may carry no point at all, because a `GROUP BY` cannot report a day it never saw. A day inside the window with no point is a real `0`; emitting an explicit zero for every day would make the response's size a function of the window rather than of the data.
              attr_accessor :metric, :unit, :points

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @metric = attributes['metric']
                @unit = attributes['unit']
                @points = attributes['points'].is_a?(Array) ? attributes['points'].map { |item| item.is_a?(Hash) ? MetricsSeriesPoint.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'metric' => @metric,
                  'unit' => @unit,
                  'points' => @points.is_a?(Array) ? @points.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
