module Sdkwork
  module BackendSdk
    module Models
      class CreateClusterRequest
              attr_accessor :name, :code, :description, :heartbeat_interval_seconds, :offline_threshold_seconds, :lb_strategy, :served_domains

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @code = attributes['code']
                @description = attributes['description']
                @heartbeat_interval_seconds = attributes['heartbeatIntervalSeconds']
                @offline_threshold_seconds = attributes['offlineThresholdSeconds']
                @lb_strategy = attributes['lbStrategy']
                @served_domains = attributes['servedDomains'].is_a?(Array) ? attributes['servedDomains'].map { |item| item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'code' => @code,
                  'description' => @description,
                  'heartbeatIntervalSeconds' => @heartbeat_interval_seconds,
                  'offlineThresholdSeconds' => @offline_threshold_seconds,
                  'lbStrategy' => @lb_strategy,
                  'servedDomains' => @served_domains.is_a?(Array) ? @served_domains.map { |item| item } : [],
                }
              end
            end
    end
  end
end
