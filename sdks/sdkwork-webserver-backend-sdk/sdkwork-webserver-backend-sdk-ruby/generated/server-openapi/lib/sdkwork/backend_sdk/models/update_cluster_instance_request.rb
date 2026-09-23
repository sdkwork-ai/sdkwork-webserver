module Sdkwork
  module BackendSdk
    module Models
      class UpdateClusterInstanceRequest
              attr_accessor :name, :status, :public_endpoint, :routing_enabled, :draining, :probe_url, :labels, :routing_weight, :maintenance_note

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @status = attributes['status']
                @public_endpoint = attributes['publicEndpoint']
                @routing_enabled = attributes['routingEnabled']
                @draining = attributes['draining']
                @probe_url = attributes['probeUrl']
                @labels = attributes['labels'].is_a?(Hash) ? attributes['labels'].transform_values { |item| item } : {}
                @routing_weight = attributes['routingWeight']
                @maintenance_note = attributes['maintenanceNote']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'status' => @status,
                  'publicEndpoint' => @public_endpoint,
                  'routingEnabled' => @routing_enabled,
                  'draining' => @draining,
                  'probeUrl' => @probe_url,
                  'labels' => @labels.is_a?(Hash) ? @labels.transform_values { |item| item } : {},
                  'routingWeight' => @routing_weight,
                  'maintenanceNote' => @maintenance_note,
                }
              end
            end
    end
  end
end
