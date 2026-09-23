module Sdkwork
  module BackendSdk
    module Models
      class ClusterProbeRunResponse
              attr_accessor :healthy, :latency_ms, :failures, :ejected, :recovered

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @healthy = attributes['healthy']
                @latency_ms = attributes['latencyMs']
                @failures = attributes['failures']
                @ejected = attributes['ejected']
                @recovered = attributes['recovered']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'healthy' => @healthy,
                  'latencyMs' => @latency_ms,
                  'failures' => @failures,
                  'ejected' => @ejected,
                  'recovered' => @recovered,
                }
              end
            end
    end
  end
end
