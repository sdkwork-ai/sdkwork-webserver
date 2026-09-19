module Sdkwork
  module BackendSdk
    module Models
      class ClusterResponse
              attr_accessor :id, :name, :code, :description, :status, :heartbeat_interval_seconds, :offline_threshold_seconds, :host_count, :instance_count, :online_instance_count, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @name = attributes['name']
                @code = attributes['code']
                @description = attributes['description']
                @status = attributes['status']
                @heartbeat_interval_seconds = attributes['heartbeatIntervalSeconds']
                @offline_threshold_seconds = attributes['offlineThresholdSeconds']
                @host_count = attributes['hostCount']
                @instance_count = attributes['instanceCount']
                @online_instance_count = attributes['onlineInstanceCount']
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'name' => @name,
                  'code' => @code,
                  'description' => @description,
                  'status' => @status,
                  'heartbeatIntervalSeconds' => @heartbeat_interval_seconds,
                  'offlineThresholdSeconds' => @offline_threshold_seconds,
                  'hostCount' => @host_count,
                  'instanceCount' => @instance_count,
                  'onlineInstanceCount' => @online_instance_count,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end
