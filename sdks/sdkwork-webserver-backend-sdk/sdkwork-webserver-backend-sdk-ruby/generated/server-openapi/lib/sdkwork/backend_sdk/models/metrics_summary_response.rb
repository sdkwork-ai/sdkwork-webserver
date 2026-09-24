module Sdkwork
  module BackendSdk
    module Models
      class MetricsSummaryResponse
              # The dashboard's cardinal metrics, each reported across the same four **half-open** windows (`today`, `last_7_days`, `current_month`, `lifetime`). Three groups share the window vocabulary but not its meaning: for the entity counts a window names when the subjects arrived, and `lifetime` is therefore the live population rather than an arrival count; for the metered traffic every window is a volume accumulated inside it, and `lifetime` is everything the facts cover — which is why `trafficSince` reports the day they start on; for the storage figures a narrow window is the part of the current holding added inside it, and `lifetime` is what is held *now* — a held figure rather than a running total, since an object removed from storage stops counting in every window including the one it was born in. `series` reports the same entity metrics at **per-day** granularity, over the window named by `seriesWindow` rather than over the four above: the four card windows belong to the product and are resolved server-side from the clock, while the series window belongs to the caller and comes from the query string, because the plot it feeds also draws the traffic reading's per-day series and that plot has one x domain. A caller that draws both must pass one pair of bounds to both operations rather than omitting them: each operation resolves an omitted bound from the clock on its own, and two defaults are two windows. The two are reported separately so a surface cannot label a day's figure with a period it was not cut against.
              attr_accessor :as_of, :platform_scope, :traffic_since, :windows, :entities, :traffic, :storage, :series, :series_window, :unassembled_metrics

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @as_of = attributes['asOf']
                @platform_scope = attributes['platformScope']
                @traffic_since = attributes['trafficSince']
                @windows = attributes['windows'].is_a?(Array) ? attributes['windows'].map { |item| item.is_a?(Hash) ? MetricsWindowBounds.from_hash(item) : item } : []
                @entities = attributes['entities'].is_a?(Array) ? attributes['entities'].map { |item| item.is_a?(Hash) ? MetricsMetricTotals.from_hash(item) : item } : []
                @traffic = attributes['traffic'].is_a?(Array) ? attributes['traffic'].map { |item| item.is_a?(Hash) ? MetricsMetricTotals.from_hash(item) : item } : []
                @storage = attributes['storage'].is_a?(Array) ? attributes['storage'].map { |item| item.is_a?(Hash) ? MetricsMetricTotals.from_hash(item) : item } : []
                @series = attributes['series'].is_a?(Array) ? attributes['series'].map { |item| item.is_a?(Hash) ? MetricsSeries.from_hash(item) : item } : []
                @series_window = attributes['seriesWindow'].is_a?(Hash) ? MetricsSeriesWindow.from_hash(attributes['seriesWindow']) : nil
                @unassembled_metrics = attributes['unassembledMetrics'].is_a?(Array) ? attributes['unassembledMetrics'].map { |item| item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'asOf' => @as_of,
                  'platformScope' => @platform_scope,
                  'trafficSince' => @traffic_since,
                  'windows' => @windows.is_a?(Array) ? @windows.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'entities' => @entities.is_a?(Array) ? @entities.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'traffic' => @traffic.is_a?(Array) ? @traffic.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'storage' => @storage.is_a?(Array) ? @storage.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'series' => @series.is_a?(Array) ? @series.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'seriesWindow' => @series_window&.to_hash,
                  'unassembledMetrics' => @unassembled_metrics.is_a?(Array) ? @unassembled_metrics.map { |item| item } : [],
                }
              end
            end
    end
  end
end
