module Sdkwork
  module BackendSdk
    module Models
      class CreateClusterRequest
              attr_accessor :name, :code, :description, :heartbeat_interval_seconds, :offline_threshold_seconds

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @code = attributes['code']
                @description = attributes['description']
                @heartbeat_interval_seconds = attributes['heartbeatIntervalSeconds']
                @offline_threshold_seconds = attributes['offlineThresholdSeconds']
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
                }
              end
            end
    end
  end
end
