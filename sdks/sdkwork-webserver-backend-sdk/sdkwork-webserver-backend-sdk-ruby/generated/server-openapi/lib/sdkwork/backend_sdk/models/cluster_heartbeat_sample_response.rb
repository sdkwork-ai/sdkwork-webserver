module Sdkwork
  module BackendSdk
    module Models
      class ClusterHeartbeatSampleResponse
              attr_accessor :id, :status, :latency_ms, :metrics, :reported_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @status = attributes['status']
                @latency_ms = attributes['latencyMs']
                @metrics = attributes['metrics'].is_a?(Hash) ? attributes['metrics'] : {}
                @reported_at = attributes['reportedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'status' => @status,
                  'latencyMs' => @latency_ms,
                  'metrics' => @metrics,
                  'reportedAt' => @reported_at,
                }
              end
            end
    end
  end
end
