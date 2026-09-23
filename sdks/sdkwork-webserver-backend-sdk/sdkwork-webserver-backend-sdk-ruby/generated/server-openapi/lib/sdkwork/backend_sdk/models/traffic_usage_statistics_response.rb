module Sdkwork
  module BackendSdk
    module Models
      class TrafficUsageStatisticsResponse
              # Aggregated traffic usage over a **half-open** date window (`dateFrom <= day < dateTo`). Every view is derived from the same append-only traffic facts, so the totals, the daily series, and the per-app breakdown are consistent with each other by construction rather than by a reconciliation job having run most recently.
              attr_accessor :date_from, :date_to, :platform_scope, :totals, :daily, :apps, :tenants

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @date_from = attributes['dateFrom']
                @date_to = attributes['dateTo']
                @platform_scope = attributes['platformScope']
                @totals = attributes['totals'].is_a?(Array) ? attributes['totals'].map { |item| item.is_a?(Hash) ? TrafficUsageTotal.from_hash(item) : item } : []
                @daily = attributes['daily'].is_a?(Array) ? attributes['daily'].map { |item| item.is_a?(Hash) ? TrafficUsageDailyPoint.from_hash(item) : item } : []
                @apps = attributes['apps'].is_a?(Array) ? attributes['apps'].map { |item| item.is_a?(Hash) ? TrafficUsageAppTotal.from_hash(item) : item } : []
                @tenants = attributes['tenants'].is_a?(Array) ? attributes['tenants'].map { |item| item.is_a?(Hash) ? TrafficUsageTenantTotal.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'dateFrom' => @date_from,
                  'dateTo' => @date_to,
                  'platformScope' => @platform_scope,
                  'totals' => @totals.is_a?(Array) ? @totals.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'daily' => @daily.is_a?(Array) ? @daily.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'apps' => @apps.is_a?(Array) ? @apps.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'tenants' => @tenants.is_a?(Array) ? @tenants.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
