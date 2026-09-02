module Sdkwork
  module BackendSdk
    module Models
      class CreateManagedDomainRequest
              attr_accessor :hostname, :application_id, :is_primary, :ssl_enabled, :ssl_provider

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @hostname = attributes['hostname']
                @application_id = attributes['applicationId']
                @is_primary = attributes['isPrimary']
                @ssl_enabled = attributes['sslEnabled']
                @ssl_provider = attributes['sslProvider']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'hostname' => @hostname,
                  'applicationId' => @application_id,
                  'isPrimary' => @is_primary,
                  'sslEnabled' => @ssl_enabled,
                  'sslProvider' => @ssl_provider,
                }
              end
            end
    end
  end
end
